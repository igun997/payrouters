//! Public Snap API: the customer-facing payment page talks to these endpoints.
//! The unguessable token in the URL is the only credential.

use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::response::Redirect;
use axum::routing::{get, post};
use chrono::{DateTime, Utc};
use payrouters_types::{Instructions, Item, Method, MethodInfo, Outcome, PaymentStatus, Provider};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{ApiJson, AppError, Result};
use crate::models::PaymentRow;
use crate::providers;
use crate::services::{options, payments};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/{token}", get(show))
        .route("/{token}/pay", post(pay))
        .route("/{token}/simulate", post(simulate))
        .route("/{token}/return", get(returned))
        .route("/{token}/cancel", get(cancelled))
        .fallback(crate::error::json_not_found)
}

#[derive(Serialize)]
struct SnapPayment {
    id: Uuid,
    order_id: String,
    status: PaymentStatus,
    livemode: bool,
    amount: i64,
    payable_amount: Option<i64>,
    currency: String,
    provider: Provider,
    method: Option<Method>,
    description: Option<String>,
    items: Vec<Item>,
    instructions: Option<Instructions>,
    expires_at: DateTime<Utc>,
    paid_at: Option<DateTime<Utc>>,
    return_url: Option<String>,
}

#[derive(Serialize)]
struct SnapView {
    merchant_name: String,
    payment: SnapPayment,
    /// Standard methods the customer can pick.
    methods: Vec<MethodInfo>,
    /// Provider-hosted checkouts the customer can pick (PayPal, Mayar, ...).
    checkouts: Vec<SnapCheckout>,
    allow_simulate: bool,
}

#[derive(Serialize)]
struct SnapCheckout {
    provider: Provider,
    label: &'static str,
}

/// What the customer can still pick: the product's payment options for a payment created from
/// them, otherwise the methods of the payment's own provider. Methods a provider account has been
/// seen to refuse are left out while anything else remains.
async fn picker(
    state: &AppState,
    row: &PaymentRow,
    provider: Provider,
) -> Result<(Vec<MethodInfo>, Vec<SnapCheckout>)> {
    if row.routed
        && let Some(usable) = options::usable(state, row.product_id, row.mode()?).await?
        && !usable.is_empty()
    {
        let p = options::picker(&usable);
        return Ok((
            p.methods.iter().map(|(m, _)| m.info()).collect(),
            p.checkouts
                .iter()
                .map(|p| SnapCheckout {
                    provider: *p,
                    label: p.label(),
                })
                .collect(),
        ));
    }
    let all = providers::adapter(provider).info().methods;
    let methods = providers::availability::offered(row.provider_account_id, &all);
    Ok((methods.iter().map(|m| m.info()).collect(), vec![]))
}

async fn view(state: &AppState, row: &PaymentRow) -> Result<SnapView> {
    let status = row.status()?;
    let provider = row.provider()?;
    let mode = row.mode()?;
    let selectable = status == PaymentStatus::Pending
        && row.instructions.is_none()
        && row.method.is_none()
        && row.expires_at > Utc::now();
    let (methods, checkouts) = if selectable {
        picker(state, row, provider).await?
    } else {
        (vec![], vec![])
    };
    Ok(SnapView {
        merchant_name: row.product_name.clone().unwrap_or_default(),
        payment: SnapPayment {
            id: row.id,
            order_id: row.order_id.clone(),
            status,
            livemode: mode.livemode(),
            amount: row.amount,
            payable_amount: row.payable_amount,
            currency: row.currency.clone(),
            provider,
            method: row.method()?,
            description: row.description.clone(),
            items: row.items.0.clone(),
            instructions: row.instructions.as_ref().map(|j| j.0.clone()),
            expires_at: row.expires_at,
            paid_at: row.paid_at,
            return_url: row.return_url.clone(),
        },
        methods,
        checkouts,
        allow_simulate: provider == Provider::Mock && status == PaymentStatus::Pending,
    })
}

async fn show(State(state): State<AppState>, Path(token): Path<String>) -> Result<Json<SnapView>> {
    let row = payments::get_by_token(&state, &token).await?;
    Ok(Json(view(&state, &row).await?))
}

/// The customer's pick: exactly one of a standard `method` or a hosted-checkout `provider`.
#[derive(Deserialize)]
struct PayBody {
    method: Option<Method>,
    provider: Option<Provider>,
}

async fn pay(
    State(state): State<AppState>,
    Path(token): Path<String>,
    ApiJson(body): ApiJson<PayBody>,
) -> Result<Json<SnapView>> {
    let choice = match (body.method, body.provider) {
        (Some(m), None) => payments::Choice::Method(m),
        (None, Some(p)) => payments::Choice::Checkout(p),
        _ => {
            return Err(AppError::bad_request(
                "send exactly one of `method` or `provider`",
            ));
        }
    };
    let row = payments::choose(&state, &token, choice).await?;
    Ok(Json(view(&state, &row).await?))
}

#[derive(Deserialize)]
struct SimulateBody {
    outcome: Outcome,
}

async fn simulate(
    State(state): State<AppState>,
    Path(token): Path<String>,
    ApiJson(body): ApiJson<SimulateBody>,
) -> Result<Json<SnapView>> {
    let row = payments::get_by_token(&state, &token).await?;
    if row.provider()? != Provider::Mock {
        return Err(AppError::unprocessable(
            "simulator_unsupported",
            "the simulator is only available for the mock provider",
        ));
    }
    let row = payments::simulate(&state, row.id, body.outcome).await?;
    Ok(Json(view(&state, &row).await?))
}

/// Where a provider sends the payer back after approving a hosted checkout (PayPal). Approval does
/// not collect anything until the order is captured, so the payment is settled with the provider
/// first and the Snap page opens on the result instead of a payment that still looks pending.
async fn returned(State(state): State<AppState>, Path(token): Path<String>) -> Result<Redirect> {
    let row = payments::get_by_token(&state, &token).await?;
    if row.status()? == PaymentStatus::Pending && row.instructions.is_some() {
        // The page polls and the worker retries, so a provider hiccup here must not strand the payer.
        if let Err(e) = payments::sync(&state, row.id).await {
            tracing::warn!(payment = %row.id, error = %e, "sync on return failed");
        }
    }
    Ok(Redirect::to(&state.snap_url(&token)))
}

/// The payer backed out of the provider's page. The Snap page opens with the automatic hop to the
/// provider paused, so they can pick it up again or go back to the merchant.
async fn cancelled(State(state): State<AppState>, Path(token): Path<String>) -> Result<Redirect> {
    payments::get_by_token(&state, &token).await?;
    Ok(Redirect::to(&format!(
        "{}?paused=1",
        state.snap_url(&token)
    )))
}
