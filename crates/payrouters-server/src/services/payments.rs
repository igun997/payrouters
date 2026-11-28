//! Payment lifecycle: create, snap method selection, provider sync, status transitions,
//! cash-in ledger and outbound events.

use chrono::{DateTime, Duration, Utc};
use payrouters_types::{
    Checkout, CreatePaymentRequest, FeeSchedule, Method, Mode, Outcome, PaymentStatus, Provider,
};
use serde_json::{Value, json};
use sqlx::types::Json;
use uuid::Uuid;

use super::accounts::{self, AccountRecord};
use super::options;
use super::outbound;
use crate::error::{AppError, Result, is_unique_violation};
use crate::models::PaymentRow;
use crate::providers::{
    self, ChargeInput, ChargeOutput, PaymentRef, ProviderAdapter, ProviderCtx, ProviderError,
    ProviderInfo, StatusUpdate, availability,
};
use crate::state::AppState;
use crate::util::{new_id, random_token};

macro_rules! payment_select {
    () => {
        "SELECT p.*, pr.name AS product_name FROM payments p JOIN products pr ON pr.id = p.product_id"
    };
}

pub const SELECT: &str = payment_select!();

const DEFAULT_EXPIRES_IN: i64 = 86_400;
const MIN_EXPIRES_IN: i64 = 300;
const MAX_EXPIRES_IN: i64 = 30 * 86_400;
const MAX_AMOUNT: i64 = 100_000_000_000;

#[derive(Debug, Clone)]
pub struct ProductCtx {
    pub id: Uuid,
    pub name: String,
    pub default_provider: Option<Provider>,
}

pub async fn get(state: &AppState, id: Uuid) -> Result<PaymentRow> {
    sqlx::query_as::<_, PaymentRow>(concat!(payment_select!(), " WHERE p.id = $1"))
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::not_found("payment"))
}

pub async fn get_by_token(state: &AppState, token: &str) -> Result<PaymentRow> {
    sqlx::query_as::<_, PaymentRow>(concat!(payment_select!(), " WHERE p.snap_token = $1"))
        .bind(token)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::not_found("payment"))
}

fn provider_err(e: ProviderError) -> AppError {
    match e {
        ProviderError::Rejected(m) | ProviderError::Unavailable(m) => AppError::Provider(m),
        ProviderError::ChannelInactive(m) => AppError::unprocessable("channel_not_activated", m),
        ProviderError::Config(m) => AppError::unprocessable("provider_unavailable", m),
    }
}

/// A charge for a standard method. When the provider says the account has not activated that channel
/// (Midtrans' Core API needs its own activation, whatever the dashboard lists), the method is
/// remembered as inactive for the account and the payment goes to the provider's hosted checkout
/// instead, where there is one. A method already known to be inactive skips the doomed call.
/// Returns the charge and whether the hosted checkout stood in for the method.
async fn charge_standard(
    adapter: &dyn ProviderAdapter,
    ctx: &ProviderCtx<'_>,
    input: &ChargeInput<'_>,
    info: &ProviderInfo,
) -> std::result::Result<(ChargeOutput, bool), ProviderError> {
    let Some(method) = input.method else {
        return Err(ProviderError::Rejected("method required".into()));
    };
    let hosted = ChargeInput {
        method: None,
        ..*input
    };
    if info.supports_provider_checkout && availability::is_inactive(ctx.account_id, method) {
        return adapter.create_hosted(ctx, &hosted).await.map(|o| (o, true));
    }
    match adapter.create_charge(ctx, input).await {
        Ok(out) => Ok((out, false)),
        Err(ProviderError::ChannelInactive(why)) => {
            availability::mark_inactive(ctx.account_id, method);
            tracing::warn!(account = %ctx.account_id, %method, %why, "channel not activated on the provider account");
            if info.supports_provider_checkout {
                adapter.create_hosted(ctx, &hosted).await.map(|o| (o, true))
            } else {
                Err(ProviderError::ChannelInactive(why))
            }
        }
        Err(e) => Err(e),
    }
}

struct Validated {
    checkout: Checkout,
    expires_in: i64,
    metadata: Value,
}

fn validate(req: &CreatePaymentRequest) -> Result<Validated> {
    let oid = req.order_id.trim();
    if oid.is_empty() || oid.chars().count() > 64 || oid.chars().any(char::is_control) {
        return Err(AppError::bad_request(
            "order_id must be 1-64 printable characters",
        ));
    }
    if req.amount < 1 || req.amount > MAX_AMOUNT {
        return Err(AppError::bad_request(
            "amount must be a positive integer number of rupiah",
        ));
    }
    if !req.items.is_empty() {
        let mut sum: i64 = 0;
        for it in &req.items {
            if it.name.trim().is_empty() || it.quantity < 1 || it.price < 0 {
                return Err(AppError::bad_request(
                    "items need a name, quantity >= 1 and price >= 0",
                ));
            }
            sum = it
                .price
                .checked_mul(it.quantity)
                .and_then(|l| sum.checked_add(l))
                .ok_or_else(|| AppError::bad_request("items total overflows"))?;
        }
        if sum != req.amount {
            return Err(AppError::bad_request(format!(
                "items total {sum} does not match amount {}",
                req.amount
            )));
        }
    }
    if let Some(c) = &req.customer
        && let Some(e) = &c.email
        && !e.contains('@')
    {
        return Err(AppError::bad_request(
            "customer.email is not a valid address",
        ));
    }
    if let Some(u) = &req.return_url {
        match url::Url::parse(u) {
            Ok(p) if matches!(p.scheme(), "http" | "https") => {}
            _ => return Err(AppError::bad_request("return_url must be an http(s) URL")),
        }
    }
    if let Some(d) = &req.description
        && d.chars().count() > 255
    {
        return Err(AppError::bad_request(
            "description is limited to 255 characters",
        ));
    }
    let metadata = match &req.metadata {
        None | Some(Value::Null) => json!({}),
        Some(v @ Value::Object(_)) => {
            if v.to_string().len() > 8192 {
                return Err(AppError::bad_request("metadata is limited to 8 KiB"));
            }
            v.clone()
        }
        Some(_) => return Err(AppError::bad_request("metadata must be a JSON object")),
    };
    let expires_in = req.expires_in.unwrap_or(DEFAULT_EXPIRES_IN);
    if !(MIN_EXPIRES_IN..=MAX_EXPIRES_IN).contains(&expires_in) {
        return Err(AppError::bad_request(format!(
            "expires_in must be between {MIN_EXPIRES_IN} and {MAX_EXPIRES_IN} seconds"
        )));
    }
    Ok(Validated {
        checkout: req.checkout.unwrap_or_default(),
        expires_in,
        metadata,
    })
}

async fn resolve_account(
    state: &AppState,
    mode: Mode,
    requested: Option<Provider>,
    default: Option<Provider>,
) -> Result<AccountRecord> {
    if let Some(p) = requested {
        if mode == Mode::Sandbox && !providers::adapter(p).info().supports_sandbox {
            return Err(AppError::unprocessable(
                "sandbox_unsupported",
                format!(
                    "{} has no sandbox; use a live key or the mock provider",
                    p.label()
                ),
            ));
        }
        return accounts::find_enabled(state, mode, p)
            .await?
            .ok_or_else(|| {
                AppError::unprocessable(
                    "provider_unavailable",
                    format!("no enabled {mode} account for provider `{p}`"),
                )
            });
    }
    let available = accounts::enabled_providers(state, mode).await?;
    let preferred = default
        .into_iter()
        .filter(|p| available.contains(p))
        .chain(
            providers::PROVIDER_ORDER
                .into_iter()
                // the simulator is only ever picked implicitly in sandbox
                .filter(|p| *p != Provider::Mock || mode == Mode::Sandbox)
                .filter(|p| available.contains(p)),
        )
        .next();
    match preferred {
        Some(p) => accounts::find_enabled(state, mode, p)
            .await?
            .ok_or_else(|| {
                AppError::unprocessable("provider_unavailable", "provider account disappeared")
            }),
        None => Err(AppError::unprocessable(
            "provider_unavailable",
            format!("no enabled provider account in {mode} mode"),
        )),
    }
}

/// Create a payment. Returns `(row, replayed)`; `replayed` means an identical pending payment
/// already existed for this `order_id` and is returned unchanged.
pub async fn create(
    state: &AppState,
    product: &ProductCtx,
    mode: Mode,
    req: CreatePaymentRequest,
) -> Result<(PaymentRow, bool)> {
    let v = validate(&req)?;
    let order_id = req.order_id.trim().to_string();
    // A product with payment options serves each method from the provider it chose for it; an
    // explicit `provider` in the request, or a product without options, keeps the old behaviour.
    let routing = options::route(
        state,
        product.id,
        mode,
        req.provider,
        req.method,
        v.checkout,
    )
    .await?;
    let account = match routing {
        Some(r) => accounts::find_enabled(state, mode, r.provider)
            .await?
            .ok_or_else(|| {
                AppError::unprocessable("provider_unavailable", "provider account disappeared")
            })?,
        None => resolve_account(state, mode, req.provider, product.default_provider).await?,
    };
    let routed = routing.is_some();
    let adapter = providers::adapter(account.provider);
    let info = adapter.info();

    if v.checkout == Checkout::Provider {
        if !info.supports_provider_checkout {
            return Err(AppError::unprocessable(
                "checkout_unsupported",
                format!(
                    "{} has no provider-hosted checkout",
                    account.provider.label()
                ),
            ));
        }
        if req.method.is_some() {
            return Err(AppError::bad_request(
                "method cannot be set with checkout=provider",
            ));
        }
    }
    if let Some(m) = req.method
        && !info.methods.contains(&m)
    {
        return Err(AppError::unprocessable(
            "method_unsupported",
            format!("{} does not support {m}", account.provider.label()),
        ));
    }

    // Idempotent replay / conflict.
    if let Some(existing) = find_by_order(state, product.id, mode, &order_id).await? {
        return replay_or_conflict(existing, &req, account.provider).map(|r| (r, true));
    }

    let id = new_id();
    let token = random_token::<24>();
    let snap_url = state.snap_url(&token);
    let snap_api_url = state.snap_api_url(&token);
    let expires_at = Utc::now() + Duration::seconds(v.expires_in);
    let provider_options = req.provider_options.clone().unwrap_or_else(|| json!({}));
    let input = ChargeInput {
        payment_id: id,
        order_id: &order_id,
        amount: req.amount,
        expires_at,
        description: req.description.as_deref(),
        customer: req.customer.as_ref(),
        items: &req.items,
        return_url: req.return_url.as_deref(),
        snap_url: &snap_url,
        snap_api_url: &snap_api_url,
        provider_options: &provider_options,
        method: req.method,
    };
    // A payment whose provider the customer still has to choose is checked when they choose.
    let provider_pending = routed && req.method.is_none() && !routing.is_some_and(|r| r.hosted_now);
    if !provider_pending {
        adapter
            .validate(&account.creds, &input, v.checkout == Checkout::Provider)
            .map_err(AppError::BadRequest)?;
    }

    let inserted = sqlx::query_as::<_, PaymentRow>(
        "INSERT INTO payments
           (id, product_id, provider_account_id, order_id, mode, provider, status, amount, checkout,
            snap_token, description, customer, items, metadata, return_url, provider_options,
            expires_at, method, routed)
         VALUES ($1,$2,$3,$4,$5,$6,'pending',$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18)
         RETURNING *",
    )
    .bind(id)
    .bind(product.id)
    .bind(account.id)
    .bind(&order_id)
    .bind(mode.as_str())
    .bind(account.provider.as_str())
    .bind(req.amount)
    .bind(v.checkout.as_str())
    .bind(&token)
    .bind(&req.description)
    .bind(req.customer.as_ref().map(Json))
    .bind(Json(&req.items))
    .bind(Json(&v.metadata))
    .bind(&req.return_url)
    .bind(Json(&provider_options))
    .bind(expires_at)
    .bind(req.method.map(|m| m.as_str()))
    .bind(routed)
    .fetch_one(&state.pool)
    .await;
    let row = match inserted {
        Ok(r) => r,
        Err(e) if is_unique_violation(&e) => {
            // Lost a race with an identical concurrent request.
            let existing = find_by_order(state, product.id, mode, &order_id)
                .await?
                .ok_or_else(|| AppError::Internal(e.into()))?;
            return replay_or_conflict(existing, &req, account.provider).map(|r| (r, true));
        }
        Err(e) => return Err(e.into()),
    };

    // Nothing for the customer to pick: a provider with no standard methods, or whose every method is
    // known to be refused (or, for a routed product, a single hosted option).
    let immediate = match routing {
        Some(r) => r.hosted_now,
        None => {
            info.methods.is_empty()
                || (info.supports_provider_checkout
                    && availability::all_inactive(account.id, &info.methods))
        }
    };
    let ctx = account.ctx(state);
    let charge: Option<std::result::Result<(ChargeOutput, bool), ProviderError>> =
        match (v.checkout, req.method) {
            (Checkout::Provider, _) => Some(
                adapter
                    .create_hosted(&ctx, &input)
                    .await
                    .map(|o| (o, false)),
            ),
            (Checkout::Snap, Some(_)) => Some(charge_standard(adapter, &ctx, &input, &info).await),
            // Nothing to pick: its hosted checkout is created straight away and the Snap page
            // shows the hop to it.
            (Checkout::Snap, None) if immediate => Some(
                adapter
                    .create_hosted(&ctx, &input)
                    .await
                    .map(|o| (o, false)),
            ),
            (Checkout::Snap, None) => None,
        };
    let Some(charge) = charge else {
        return Ok((get(state, row.id).await?, false));
    };
    match charge {
        Ok((out, fell_back)) => {
            let method = if fell_back { None } else { req.method };
            Ok((store_charge(state, row.id, out, method, None).await?, false))
        }
        Err(e) => {
            tracing::warn!(provider = %account.provider, payment = %row.id, error = %e, "provider charge failed");
            // The order never existed from the product's point of view: free the order_id.
            sqlx::query("DELETE FROM payments WHERE id = $1 AND instructions IS NULL")
                .bind(row.id)
                .execute(&state.pool)
                .await?;
            Err(provider_err(e))
        }
    }
}

fn replay_or_conflict(
    existing: PaymentRow,
    req: &CreatePaymentRequest,
    provider: Provider,
) -> Result<PaymentRow> {
    if existing.status == PaymentStatus::Pending.as_str()
        && existing.amount == req.amount
        && (existing.routed || existing.provider == provider.as_str())
    {
        Ok(existing)
    } else {
        Err(AppError::conflict(
            "order_id_conflict",
            "a different payment already exists for this order_id",
        ))
    }
}

async fn find_by_order(
    state: &AppState,
    product_id: Uuid,
    mode: Mode,
    order_id: &str,
) -> Result<Option<PaymentRow>> {
    Ok(sqlx::query_as::<_, PaymentRow>(concat!(
        payment_select!(),
        " WHERE p.product_id = $1 AND p.mode = $2 AND p.order_id = $3"
    ))
    .bind(product_id)
    .bind(mode.as_str())
    .bind(order_id)
    .fetch_optional(&state.pool)
    .await?)
}

async fn store_charge(
    state: &AppState,
    id: Uuid,
    out: ChargeOutput,
    method: Option<Method>,
    account: Option<&AccountRecord>,
) -> Result<PaymentRow> {
    // `account` re-points a routed payment at the provider that took the charge.
    sqlx::query(
        "UPDATE payments
         SET instructions = $2, provider_ref = $3, provider_data = $4, payable_amount = $5,
             method = $6, provider = COALESCE($7::text, provider),
             provider_account_id = COALESCE($8::uuid, provider_account_id),
             last_synced_at = now(), updated_at = now()
         WHERE id = $1",
    )
    .bind(id)
    .bind(Json(&out.instructions))
    .bind(&out.provider_ref)
    .bind(Json(&out.provider_data))
    .bind(out.payable_amount)
    .bind(method.map(|m| m.as_str()))
    .bind(account.map(|a| a.provider.as_str()))
    .bind(account.map(|a| a.id))
    .execute(&state.pool)
    .await?;
    get(state, id).await
}

/// What the customer picked on the Snap page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    /// A standard method (QRIS, a virtual account).
    Method(Method),
    /// A provider's hosted checkout page (PayPal, Mayar, ...).
    Checkout(Provider),
}

/// The provider that serves a choice. A payment created from the product's payment options goes to
/// the provider the product chose for that option; any other payment stays with its own provider.
async fn target_provider(state: &AppState, row: &PaymentRow, choice: Choice) -> Result<Provider> {
    if row.routed
        && let Some(options) = options::usable(state, row.product_id, row.mode()?).await?
        && !options.is_empty()
    {
        let found = options.iter().find(|u| match choice {
            Choice::Method(m) => u.option.kind == options::Kind::Method(m),
            Choice::Checkout(p) => {
                u.option.kind == options::Kind::Checkout && u.option.provider == p
            }
        });
        return found
            .map(|u| u.option.provider)
            .ok_or_else(|| match choice {
                Choice::Method(m) => AppError::unprocessable(
                    "method_unsupported",
                    format!("this product does not offer {m}"),
                ),
                Choice::Checkout(p) => AppError::unprocessable(
                    "checkout_unsupported",
                    format!("this product does not offer the {} checkout", p.label()),
                ),
            });
    }
    match choice {
        Choice::Method(_) => row.provider(),
        Choice::Checkout(_) => Err(AppError::unprocessable(
            "checkout_unsupported",
            "this payment has no hosted checkout to choose",
        )),
    }
}

/// Snap: the customer picked a method or a hosted checkout; create the provider charge.
pub async fn choose(state: &AppState, token: &str, choice: Choice) -> Result<PaymentRow> {
    let row = get_by_token(state, token).await?;
    if row.status()? != PaymentStatus::Pending {
        return Err(AppError::conflict(
            "payment_not_pending",
            "this payment is no longer payable",
        ));
    }
    if row.expires_at <= Utc::now() {
        return Err(AppError::conflict(
            "payment_expired",
            "this payment has expired",
        ));
    }
    if row.instructions.is_some() || row.method.is_some() {
        return Err(AppError::conflict(
            "method_already_selected",
            "a payment method was already selected",
        ));
    }
    let provider = target_provider(state, &row, choice).await?;
    let adapter = providers::adapter(provider);
    let info = adapter.info();
    match choice {
        Choice::Method(m) if !info.methods.contains(&m) => {
            return Err(AppError::unprocessable(
                "method_unsupported",
                format!("{} does not support {m}", provider.label()),
            ));
        }
        Choice::Checkout(_) if !info.supports_provider_checkout => {
            return Err(AppError::unprocessable(
                "checkout_unsupported",
                format!("{} has no provider-hosted checkout", provider.label()),
            ));
        }
        _ => {}
    }
    // Reserve the choice so concurrent taps cannot create two charges. A method is reserved in the
    // `method` column; a hosted checkout has none, so it leaves a marker in `provider_data`.
    let claimed = match choice {
        Choice::Method(m) => {
            sqlx::query(
                "UPDATE payments SET method = $2, updated_at = now()
                 WHERE id = $1 AND instructions IS NULL AND method IS NULL AND status = 'pending'
                   AND provider_data->>'choosing' IS NULL",
            )
            .bind(row.id)
            .bind(m.as_str())
            .execute(&state.pool)
            .await?
        }
        Choice::Checkout(_) => sqlx::query(
            "UPDATE payments SET provider_data = '{\"choosing\": true}'::jsonb, updated_at = now()
                 WHERE id = $1 AND instructions IS NULL AND method IS NULL AND status = 'pending'
                   AND provider_data->>'choosing' IS NULL",
        )
        .bind(row.id)
        .execute(&state.pool)
        .await?,
    };
    if claimed.rows_affected() == 0 {
        return Err(AppError::conflict(
            "method_already_selected",
            "a payment method was already selected",
        ));
    }

    match charge_choice(state, &row, choice, provider, adapter, &info).await {
        Ok(paid_row) => Ok(paid_row),
        Err(e) => {
            sqlx::query(
                "UPDATE payments SET method = NULL, provider_data = '{}'::jsonb, updated_at = now()
                 WHERE id = $1 AND instructions IS NULL",
            )
            .bind(row.id)
            .execute(&state.pool)
            .await?;
            Err(e)
        }
    }
}

async fn charge_choice(
    state: &AppState,
    row: &PaymentRow,
    choice: Choice,
    provider: Provider,
    adapter: &dyn ProviderAdapter,
    info: &ProviderInfo,
) -> Result<PaymentRow> {
    let account = if provider.as_str() == row.provider {
        accounts::load(state, row.provider_account_id).await?
    } else {
        accounts::find_enabled(state, row.mode()?, provider)
            .await?
            .ok_or_else(|| {
                AppError::unprocessable(
                    "provider_unavailable",
                    format!("no enabled {} account", provider.label()),
                )
            })?
    };
    let snap_url = state.snap_url(&row.snap_token);
    let snap_api_url = state.snap_api_url(&row.snap_token);
    let customer = row.customer.as_ref().map(|c| &c.0);
    let input = ChargeInput {
        payment_id: row.id,
        order_id: &row.order_id,
        amount: row.amount,
        expires_at: row.expires_at,
        description: row.description.as_deref(),
        customer,
        items: &row.items.0,
        return_url: row.return_url.as_deref(),
        snap_url: &snap_url,
        snap_api_url: &snap_api_url,
        provider_options: &row.provider_options.0,
        method: match choice {
            Choice::Method(m) => Some(m),
            Choice::Checkout(_) => None,
        },
    };
    let hosted = matches!(choice, Choice::Checkout(_));
    let ctx = account.ctx(state);
    let result = match adapter.validate(&account.creds, &input, hosted) {
        Ok(()) => {
            let charge = if hosted {
                adapter
                    .create_hosted(&ctx, &input)
                    .await
                    .map(|o| (o, false))
            } else {
                charge_standard(adapter, &ctx, &input, info).await
            };
            charge.map_err(|e| {
                tracing::warn!(%provider, payment = %row.id, error = %e, "provider charge failed");
                provider_err(e)
            })
        }
        Err(m) => Err(AppError::BadRequest(m)),
    };
    let (out, fell_back) = result?;
    // On a hosted checkout the customer chooses there: no method is recorded.
    let method = match choice {
        Choice::Method(m) if !fell_back => Some(m),
        _ => None,
    };
    store_charge(state, row.id, out, method, Some(&account)).await
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyResult {
    Applied(PaymentStatus),
    Noop,
    Rejected(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ledger {
    pub gross: i64,
    pub fee: i64,
    pub net: i64,
}

/// Cash-in amounts. A provider-quoted `payable` above `amount` means the provider added its fee
/// on top (customer paid `payable`, we receive `amount`); otherwise the reported fee, else the
/// account's fee schedule, is deducted from the amount.
pub fn compute_ledger(
    amount: i64,
    payable: Option<i64>,
    reported_fee: Option<i64>,
    schedule: &FeeSchedule,
    method: Option<Method>,
) -> Ledger {
    if let Some(p) = payable.filter(|p| *p > amount) {
        return Ledger {
            gross: p,
            fee: p - amount,
            net: amount,
        };
    }
    let fee = reported_fee
        .or_else(|| {
            method
                .and_then(|m| schedule.get(m.as_str()))
                .or_else(|| schedule.get("*"))
                .map(|r| r.apply(amount))
        })
        .unwrap_or(0)
        .clamp(0, amount);
    Ledger {
        gross: amount,
        fee,
        net: amount - fee,
    }
}

/// The single place where provider facts become payment state. Idempotent: replays are no-ops.
pub async fn apply_update(
    state: &AppState,
    payment_id: Uuid,
    update: StatusUpdate,
) -> Result<ApplyResult> {
    let mut tx = state.pool.begin().await?;
    let row: PaymentRow = sqlx::query_as("SELECT * FROM payments WHERE id = $1 FOR UPDATE")
        .bind(payment_id)
        .fetch_one(&mut *tx)
        .await?;
    let current = row.status()?;

    let transition = match (current, update.status) {
        (PaymentStatus::Paid, _) | (_, PaymentStatus::Pending) => None,
        (c, n) if c == n => None,
        (PaymentStatus::Pending, n) => Some(n),
        // money arrived after we had given up on the payment
        (_, PaymentStatus::Paid) => Some(PaymentStatus::Paid),
        _ => None,
    };

    if transition == Some(PaymentStatus::Paid)
        && let Some(reported) = update.amount
        && reported != row.amount
    {
        tx.rollback().await?;
        return Ok(ApplyResult::Rejected(format!(
            "amount mismatch: provider reports {reported}, payment is {}",
            row.amount
        )));
    }

    let method = row.method()?.or(update.method);
    let provider_ref = row
        .provider_ref
        .clone()
        .or_else(|| update.provider_ref.clone());

    let Some(new_status) = transition else {
        if method.map(|m| m.as_str()) != row.method.as_deref() || provider_ref != row.provider_ref {
            sqlx::query("UPDATE payments SET method = $2, provider_ref = $3, updated_at = now() WHERE id = $1")
                .bind(row.id)
                .bind(method.map(|m| m.as_str()))
                .bind(&provider_ref)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        return Ok(ApplyResult::Noop);
    };

    let updated: PaymentRow = if new_status == PaymentStatus::Paid {
        let schedule: Json<FeeSchedule> =
            sqlx::query_scalar("SELECT fees FROM provider_accounts WHERE id = $1")
                .bind(row.provider_account_id)
                .fetch_one(&mut *tx)
                .await?;
        let ledger = compute_ledger(
            row.amount,
            row.payable_amount,
            update.fee_amount,
            &schedule.0,
            method,
        );
        let paid_at: DateTime<Utc> = update.paid_at.unwrap_or_else(Utc::now);
        let updated = sqlx::query_as::<_, PaymentRow>(
            "UPDATE payments
             SET status = 'paid', paid_at = $2, fee_amount = $3, net_amount = $4, method = $5,
                 provider_ref = $6, updated_at = now()
             WHERE id = $1 RETURNING *",
        )
        .bind(row.id)
        .bind(paid_at)
        .bind(ledger.fee)
        .bind(ledger.net)
        .bind(method.map(|m| m.as_str()))
        .bind(&provider_ref)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO cash_ins
               (id, payment_id, product_id, provider_account_id, provider, mode, method, currency,
                gross_amount, fee_amount, net_amount, paid_at, provider_ref)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
             ON CONFLICT (payment_id) DO NOTHING",
        )
        .bind(new_id())
        .bind(row.id)
        .bind(row.product_id)
        .bind(row.provider_account_id)
        .bind(&row.provider)
        .bind(&row.mode)
        .bind(method.map(|m| m.as_str()))
        .bind(&row.currency)
        .bind(ledger.gross)
        .bind(ledger.fee)
        .bind(ledger.net)
        .bind(paid_at)
        .bind(&provider_ref)
        .execute(&mut *tx)
        .await?;
        updated
    } else {
        sqlx::query_as::<_, PaymentRow>(
            "UPDATE payments SET status = $2, method = $3, provider_ref = $4, updated_at = now()
             WHERE id = $1 RETURNING *",
        )
        .bind(row.id)
        .bind(new_status.as_str())
        .bind(method.map(|m| m.as_str()))
        .bind(&provider_ref)
        .fetch_one(&mut *tx)
        .await?
    };

    if let Some(event) = new_status.event_type() {
        let view = updated.view(&state.config.public_base_url)?;
        outbound::enqueue(&mut tx, row.product_id, &view, event).await?;
    }
    tx.commit().await?;
    tracing::info!(payment = %payment_id, from = %current, to = %new_status, "payment status changed");
    Ok(ApplyResult::Applied(new_status))
}

/// Ask the provider for the current state and apply it.
pub async fn sync(state: &AppState, id: Uuid) -> Result<PaymentRow> {
    let row = get(state, id).await?;
    if row.status()? == PaymentStatus::Paid || row.provider()? == Provider::Mock {
        return Ok(row);
    }
    let account = accounts::load(state, row.provider_account_id).await?;
    let adapter = providers::adapter(account.provider);
    let found = adapter
        .fetch_status(
            &account.ctx(state),
            &PaymentRef {
                id: row.id,
                provider_ref: row.provider_ref.as_deref(),
                provider_data: &row.provider_data.0,
            },
        )
        .await;
    sqlx::query("UPDATE payments SET last_synced_at = now() WHERE id = $1")
        .bind(id)
        .execute(&state.pool)
        .await?;
    if let Some(update) = found.map_err(provider_err)? {
        match apply_update(state, id, update).await? {
            ApplyResult::Rejected(why) => {
                tracing::warn!(payment = %id, %why, "sync update rejected")
            }
            ApplyResult::Applied(_) | ApplyResult::Noop => {}
        }
    }
    get(state, id).await
}

/// Sandbox simulator (mock provider only): pushes a status through the real pipeline.
pub async fn simulate(state: &AppState, id: Uuid, outcome: Outcome) -> Result<PaymentRow> {
    let row = get(state, id).await?;
    if row.provider()? != Provider::Mock {
        return Err(AppError::unprocessable(
            "simulator_unsupported",
            "only payments on the mock provider can be simulated; use the provider's own sandbox simulator",
        ));
    }
    if row.status()? != PaymentStatus::Pending {
        return Err(AppError::conflict(
            "payment_not_pending",
            "this payment is no longer pending",
        ));
    }
    if outcome == Outcome::Paid && row.instructions.is_none() {
        return Err(AppError::conflict(
            "method_not_selected",
            "choose a payment method before simulating payment",
        ));
    }
    sqlx::query(
        "INSERT INTO provider_events
           (id, provider_account_id, provider, payment_id, verified, outcome, headers, body_text)
         VALUES ($1, $2, 'mock', $3, true, 'applied', '{}'::jsonb, $4)",
    )
    .bind(new_id())
    .bind(row.provider_account_id)
    .bind(row.id)
    .bind(json!({ "simulated": outcome.as_str(), "payment_id": row.id }).to_string())
    .execute(&state.pool)
    .await?;
    let mut update = StatusUpdate::new(outcome.into());
    update.amount = Some(row.amount);
    apply_update(state, id, update).await?;
    get(state, id).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use payrouters_types::FeeRule;

    fn sched(entries: &[(&str, i64, f64)]) -> FeeSchedule {
        entries
            .iter()
            .map(|(k, fixed, percent)| {
                (
                    k.to_string(),
                    FeeRule {
                        fixed: *fixed,
                        percent: *percent,
                    },
                )
            })
            .collect()
    }

    #[test]
    fn ledger_uses_method_rule_then_default_then_zero() {
        let s = sched(&[("va_bca", 4000, 0.0), ("*", 0, 0.7)]);
        assert_eq!(
            compute_ledger(100_000, None, None, &s, Some(Method::VaBca)),
            Ledger {
                gross: 100_000,
                fee: 4000,
                net: 96_000
            }
        );
        assert_eq!(
            compute_ledger(100_000, None, None, &s, Some(Method::Qris)),
            Ledger {
                gross: 100_000,
                fee: 700,
                net: 99_300
            }
        );
        assert_eq!(
            compute_ledger(100_000, None, None, &sched(&[]), None),
            Ledger {
                gross: 100_000,
                fee: 0,
                net: 100_000
            }
        );
    }

    #[test]
    fn ledger_reported_fee_beats_schedule_and_is_clamped() {
        let s = sched(&[("*", 9999, 0.0)]);
        assert_eq!(compute_ledger(50_000, None, Some(300), &s, None).fee, 300);
        assert_eq!(compute_ledger(1000, None, Some(5000), &s, None).fee, 1000);
    }

    #[test]
    fn ledger_provider_added_fee() {
        // IDRX VA: customer pays 24 000 for 20 000 minted; the 4 000 is the provider's fee.
        let l = compute_ledger(20_000, Some(24_000), None, &sched(&[("*", 1, 0.0)]), None);
        assert_eq!(
            l,
            Ledger {
                gross: 24_000,
                fee: 4_000,
                net: 20_000
            }
        );
        // payable equal to amount means no on-top fee
        assert_eq!(
            compute_ledger(20_000, Some(20_000), None, &sched(&[]), None).fee,
            0
        );
    }

    #[test]
    fn validation_rules() {
        let ok = CreatePaymentRequest {
            order_id: "A1".into(),
            amount: 1000,
            ..Default::default()
        };
        assert!(validate(&ok).is_ok());
        let mut bad = ok.clone();
        bad.amount = 0;
        assert!(validate(&bad).is_err());
        let mut bad = ok.clone();
        bad.order_id = " ".into();
        assert!(validate(&bad).is_err());
        let mut bad = ok.clone();
        bad.expires_in = Some(10);
        assert!(validate(&bad).is_err());
        let mut bad = ok.clone();
        bad.metadata = Some(json!([1]));
        assert!(validate(&bad).is_err());
        let mut bad = ok.clone();
        bad.return_url = Some("ftp://x".into());
        assert!(validate(&bad).is_err());
        let mut items = ok.clone();
        items.items = vec![payrouters_types::Item {
            id: None,
            name: "x".into(),
            price: 400,
            quantity: 2,
        }];
        assert!(validate(&items).is_err());
        items.items[0].price = 500;
        assert!(validate(&items).is_ok());
    }

    // --- charge_standard ---------------------------------------------------------------------

    use crate::providers::{Credentials, ParsedWebhook, WebhookError};
    use async_trait::async_trait;
    use axum::http::HeaderMap;
    use payrouters_types::Instructions;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// An adapter whose core charge always answers with `refusal`, like a Midtrans account without
    /// Core API access.
    struct Fake {
        hosted: bool,
        refusal: fn() -> ProviderError,
        core_calls: AtomicUsize,
    }

    impl Fake {
        fn new(hosted: bool, refusal: fn() -> ProviderError) -> Fake {
            Fake {
                hosted,
                refusal,
                core_calls: AtomicUsize::new(0),
            }
        }

        fn core_calls(&self) -> usize {
            self.core_calls.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl ProviderAdapter for Fake {
        fn info(&self) -> ProviderInfo {
            ProviderInfo {
                provider: Provider::Mock,
                supports_sandbox: true,
                supports_provider_checkout: self.hosted,
                credential_fields: vec![],
                methods: vec![Method::Qris, Method::VaBca],
            }
        }

        async fn create_charge(
            &self,
            _ctx: &ProviderCtx<'_>,
            _input: &ChargeInput<'_>,
        ) -> std::result::Result<ChargeOutput, ProviderError> {
            self.core_calls.fetch_add(1, Ordering::SeqCst);
            Err((self.refusal)())
        }

        async fn create_hosted(
            &self,
            _ctx: &ProviderCtx<'_>,
            input: &ChargeInput<'_>,
        ) -> std::result::Result<ChargeOutput, ProviderError> {
            assert_eq!(input.method, None, "the hosted checkout has no method");
            Ok(ChargeOutput {
                instructions: Instructions::Redirect {
                    url: "https://pay.example/hosted".into(),
                },
                provider_ref: Some("snap-token".into()),
                provider_data: json!({ "kind": "snap" }),
                payable_amount: None,
            })
        }

        async fn fetch_status(
            &self,
            _ctx: &ProviderCtx<'_>,
            _payment: &PaymentRef<'_>,
        ) -> std::result::Result<Option<StatusUpdate>, ProviderError> {
            Ok(None)
        }

        async fn parse_webhook(
            &self,
            _ctx: &ProviderCtx<'_>,
            _headers: &HeaderMap,
            _body: &[u8],
        ) -> std::result::Result<ParsedWebhook, WebhookError> {
            Err(WebhookError::Malformed("unused".into()))
        }

        async fn test_connection(
            &self,
            _ctx: &ProviderCtx<'_>,
        ) -> std::result::Result<String, ProviderError> {
            Ok("ok".into())
        }
    }

    fn charge(method: Method) -> ChargeInput<'static> {
        ChargeInput {
            payment_id: Uuid::nil(),
            order_id: "INV-1",
            amount: 129_000,
            expires_at: Utc::now() + Duration::hours(1),
            description: None,
            customer: None,
            items: &[],
            return_url: None,
            snap_url: "https://pay.example.com/snap/t",
            snap_api_url: "https://pay.example.com/snap/api/t",
            provider_options: &Value::Null,
            method: Some(method),
        }
    }

    fn inactive() -> ProviderError {
        ProviderError::ChannelInactive(
            "Midtrans responded 402: Payment channel is not activated.".into(),
        )
    }

    #[tokio::test]
    async fn a_channel_the_provider_has_not_activated_falls_back_to_the_hosted_checkout() {
        let adapter = Fake::new(true, inactive);
        let (http, creds) = (reqwest::Client::new(), Credentials::default());
        let ctx = ProviderCtx {
            http: &http,
            account_id: new_id(),
            mode: Mode::Live,
            creds: &creds,
            public_base_url: "https://pay.example.com",
        };
        let info = adapter.info();

        let (out, fell_back) = charge_standard(&adapter, &ctx, &charge(Method::Qris), &info)
            .await
            .unwrap();
        assert!(fell_back);
        assert!(matches!(out.instructions, Instructions::Redirect { .. }));
        assert_eq!(adapter.core_calls(), 1);
        assert!(availability::is_inactive(ctx.account_id, Method::Qris));

        // Remembered: the next QRIS payment on this account goes straight to the hosted checkout.
        let (_, again) = charge_standard(&adapter, &ctx, &charge(Method::Qris), &info)
            .await
            .unwrap();
        assert!(again);
        assert_eq!(adapter.core_calls(), 1);

        // Another method has not been refused yet, so it is tried (and learned) on its own.
        let (_, other) = charge_standard(&adapter, &ctx, &charge(Method::VaBca), &info)
            .await
            .unwrap();
        assert!(other);
        assert_eq!(adapter.core_calls(), 2);

        // Another account is unaffected by what this one taught.
        let ctx2 = ProviderCtx {
            account_id: new_id(),
            ..ctx
        };
        charge_standard(&adapter, &ctx2, &charge(Method::Qris), &info)
            .await
            .unwrap();
        assert_eq!(adapter.core_calls(), 3);
    }

    #[tokio::test]
    async fn without_a_hosted_checkout_the_refusal_is_reported_and_retried_next_time() {
        let adapter = Fake::new(false, inactive);
        let (http, creds) = (reqwest::Client::new(), Credentials::default());
        let ctx = ProviderCtx {
            http: &http,
            account_id: new_id(),
            mode: Mode::Live,
            creds: &creds,
            public_base_url: "https://pay.example.com",
        };
        let info = adapter.info();
        for expected_calls in [1, 2] {
            let err = charge_standard(&adapter, &ctx, &charge(Method::Qris), &info)
                .await
                .err()
                .expect("no hosted checkout to fall back to");
            assert!(matches!(err, ProviderError::ChannelInactive(_)));
            assert_eq!(adapter.core_calls(), expected_calls);
        }
    }

    #[tokio::test]
    async fn other_refusals_are_neither_remembered_nor_replaced_by_the_hosted_checkout() {
        let adapter = Fake::new(true, || ProviderError::Rejected("amount too small".into()));
        let (http, creds) = (reqwest::Client::new(), Credentials::default());
        let ctx = ProviderCtx {
            http: &http,
            account_id: new_id(),
            mode: Mode::Live,
            creds: &creds,
            public_base_url: "https://pay.example.com",
        };
        let info = adapter.info();
        for expected_calls in [1, 2] {
            let err = charge_standard(&adapter, &ctx, &charge(Method::Qris), &info)
                .await
                .err()
                .expect("a plain rejection is returned");
            assert!(matches!(err, ProviderError::Rejected(_)));
            assert_eq!(adapter.core_calls(), expected_calls);
        }
        assert!(!availability::is_inactive(ctx.account_id, Method::Qris));
    }
}
