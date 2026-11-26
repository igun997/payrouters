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
use super::outbound;
use crate::error::{AppError, Result, is_unique_violation};
use crate::models::PaymentRow;
use crate::providers::{self, ChargeInput, ChargeOutput, PaymentRef, ProviderError, StatusUpdate};
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
        ProviderError::Config(m) => AppError::unprocessable("provider_unavailable", m),
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
    let account = resolve_account(state, mode, req.provider, product.default_provider).await?;
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
        provider_options: &provider_options,
        method: req.method,
    };
    adapter
        .validate(&account.creds, &input, v.checkout == Checkout::Provider)
        .map_err(AppError::BadRequest)?;

    let inserted = sqlx::query_as::<_, PaymentRow>(
        "INSERT INTO payments
           (id, product_id, provider_account_id, order_id, mode, provider, status, amount, checkout,
            snap_token, description, customer, items, metadata, return_url, provider_options,
            expires_at, method)
         VALUES ($1,$2,$3,$4,$5,$6,'pending',$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17)
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

    let ctx = account.ctx(state);
    let charge: Option<std::result::Result<ChargeOutput, ProviderError>> =
        match (v.checkout, req.method) {
            (Checkout::Provider, _) => Some(adapter.create_hosted(&ctx, &input).await),
            (Checkout::Snap, Some(_)) => Some(adapter.create_charge(&ctx, &input).await),
            (Checkout::Snap, None) => None,
        };
    let Some(charge) = charge else {
        return Ok((get(state, row.id).await?, false));
    };
    match charge {
        Ok(out) => Ok((store_charge(state, row.id, out, req.method).await?, false)),
        Err(e) => {
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
        && existing.provider == provider.as_str()
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
) -> Result<PaymentRow> {
    sqlx::query(
        "UPDATE payments
         SET instructions = $2, provider_ref = $3, provider_data = $4, payable_amount = $5,
             method = COALESCE($6, method), last_synced_at = now(), updated_at = now()
         WHERE id = $1",
    )
    .bind(id)
    .bind(Json(&out.instructions))
    .bind(&out.provider_ref)
    .bind(Json(&out.provider_data))
    .bind(out.payable_amount)
    .bind(method.map(|m| m.as_str()))
    .execute(&state.pool)
    .await?;
    get(state, id).await
}

/// Snap: the customer picked a method; create the provider charge.
pub async fn choose_method(state: &AppState, token: &str, method: Method) -> Result<PaymentRow> {
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
    let provider = row.provider()?;
    let adapter = providers::adapter(provider);
    if !adapter.info().methods.contains(&method) {
        return Err(AppError::unprocessable(
            "method_unsupported",
            format!("{} does not support {method}", provider.label()),
        ));
    }
    // Reserve the choice so concurrent taps cannot create two charges.
    let claimed = sqlx::query(
        "UPDATE payments SET method = $2, updated_at = now()
         WHERE id = $1 AND instructions IS NULL AND method IS NULL AND status = 'pending'",
    )
    .bind(row.id)
    .bind(method.as_str())
    .execute(&state.pool)
    .await?;
    if claimed.rows_affected() == 0 {
        return Err(AppError::conflict(
            "method_already_selected",
            "a payment method was already selected",
        ));
    }

    let account = accounts::load(state, row.provider_account_id).await?;
    let snap_url = state.snap_url(&row.snap_token);
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
        provider_options: &row.provider_options.0,
        method: Some(method),
    };
    let result = match adapter.validate(&account.creds, &input, false) {
        Ok(()) => adapter
            .create_charge(&account.ctx(state), &input)
            .await
            .map_err(provider_err),
        Err(m) => Err(AppError::BadRequest(m)),
    };
    match result {
        Ok(out) => store_charge(state, row.id, out, Some(method)).await,
        Err(e) => {
            sqlx::query("UPDATE payments SET method = NULL, updated_at = now() WHERE id = $1 AND instructions IS NULL")
                .bind(row.id)
                .execute(&state.pool)
                .await?;
            Err(e)
        }
    }
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
}
