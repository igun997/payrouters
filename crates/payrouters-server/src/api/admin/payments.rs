use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use chrono::{DateTime, Utc};
use payrouters_types::{
    CreatePaymentRequest, ListResponse, Mode, Outcome, Payment, PaymentStatus, Provider,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Postgres, QueryBuilder};
use uuid::Uuid;

use super::parse_date;
use crate::api::v1::clamp_limit;
use crate::error::{ApiJson, ApiQuery, AppError, Result};
use crate::models::{CashInRow, DeliveryRow, EventRow, PaymentRow};
use crate::services::payments::{self, ProductCtx};
use crate::state::AppState;
use crate::util::Cursor;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/payments", get(list).post(create))
        .route("/payments/{id}", get(detail))
        .route("/payments/{id}/sync", post(sync))
        .route("/payments/{id}/simulate", post(simulate))
        .route("/webhook-events", get(events))
        .route("/deliveries", get(deliveries))
        .route("/deliveries/{id}/retry", post(retry_delivery))
}

#[derive(Serialize)]
pub struct ProductRef {
    pub id: Uuid,
    pub name: String,
}

#[derive(Serialize)]
pub struct AdminPayment {
    #[serde(flatten)]
    pub payment: Payment,
    pub product: ProductRef,
    pub provider_account_id: Uuid,
}

pub fn admin_view(state: &AppState, row: &PaymentRow) -> Result<AdminPayment> {
    Ok(AdminPayment {
        payment: row.view(&state.config.public_base_url)?,
        product: ProductRef {
            id: row.product_id,
            name: row.product_name.clone().unwrap_or_default(),
        },
        provider_account_id: row.provider_account_id,
    })
}

#[derive(Serialize)]
pub struct CashInView {
    id: Uuid,
    payment_id: Uuid,
    order_id: String,
    product: ProductRef,
    provider: String,
    method: Option<String>,
    mode: String,
    currency: String,
    gross_amount: i64,
    fee_amount: i64,
    net_amount: i64,
    paid_at: DateTime<Utc>,
    provider_ref: Option<String>,
}

pub fn cash_in_view(r: CashInRow) -> CashInView {
    CashInView {
        id: r.id,
        payment_id: r.payment_id,
        order_id: r.order_id.unwrap_or_default(),
        product: ProductRef {
            id: r.product_id,
            name: r.product_name.unwrap_or_default(),
        },
        provider: r.provider,
        method: r.method,
        mode: r.mode,
        currency: r.currency,
        gross_amount: r.gross_amount,
        fee_amount: r.fee_amount,
        net_amount: r.net_amount,
        paid_at: r.paid_at,
        provider_ref: r.provider_ref,
    }
}

#[derive(Debug, Deserialize)]
struct ListQuery {
    mode: Option<Mode>,
    status: Option<PaymentStatus>,
    provider: Option<Provider>,
    product_id: Option<Uuid>,
    q: Option<String>,
    from: Option<String>,
    to: Option<String>,
    limit: Option<i64>,
    cursor: Option<String>,
}

async fn list(
    State(state): State<AppState>,
    ApiQuery(q): ApiQuery<ListQuery>,
) -> Result<Json<ListResponse<AdminPayment>>> {
    let limit = clamp_limit(q.limit);
    let tz = state.config.timezone;
    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(payments::SELECT);
    qb.push(" WHERE 1=1");
    if let Some(m) = q.mode {
        qb.push(" AND p.mode = ").push_bind(m.as_str());
    }
    if let Some(s) = q.status {
        qb.push(" AND p.status = ").push_bind(s.as_str());
    }
    if let Some(p) = q.provider {
        qb.push(" AND p.provider = ").push_bind(p.as_str());
    }
    if let Some(id) = q.product_id {
        qb.push(" AND p.product_id = ").push_bind(id);
    }
    if let Some(term) = q.q.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
        let like = format!(
            "%{}%",
            term.replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_")
        );
        qb.push(" AND (p.order_id ILIKE ").push_bind(like.clone());
        qb.push(" OR p.id::text ILIKE ").push_bind(like.clone());
        qb.push(" OR p.provider_ref ILIKE ")
            .push_bind(like)
            .push(")");
    }
    if let Some(f) = &q.from {
        let (start, _) =
            super::stats::day_bounds(parse_date("from", f)?, parse_date("from", f)?, tz);
        qb.push(" AND p.created_at >= ").push_bind(start);
    }
    if let Some(t) = &q.to {
        let (_, end) = super::stats::day_bounds(parse_date("to", t)?, parse_date("to", t)?, tz);
        qb.push(" AND p.created_at < ").push_bind(end);
    }
    if let Some(raw) = &q.cursor {
        let c = Cursor::decode(raw).ok_or_else(|| AppError::bad_request("invalid cursor"))?;
        qb.push(" AND (p.created_at, p.id) < (")
            .push_bind(c.ts)
            .push(", ")
            .push_bind(c.id)
            .push(")");
    }
    qb.push(" ORDER BY p.created_at DESC, p.id DESC LIMIT ")
        .push_bind(limit + 1);
    let mut rows: Vec<PaymentRow> = qb.build_query_as().fetch_all(&state.pool).await?;
    let next_cursor = if rows.len() as i64 > limit {
        rows.truncate(limit as usize);
        rows.last().map(|r| {
            Cursor {
                ts: r.created_at,
                id: r.id,
            }
            .encode()
        })
    } else {
        None
    };
    let data = rows
        .iter()
        .map(|r| admin_view(&state, r))
        .collect::<Result<Vec<_>>>()?;
    Ok(Json(ListResponse { data, next_cursor }))
}

async fn detail(State(state): State<AppState>, Path(id): Path<Uuid>) -> Result<Json<Value>> {
    let row = payments::get(&state, id).await?;
    let events: Vec<EventRow> = sqlx::query_as(
        "SELECT * FROM provider_events WHERE payment_id = $1 ORDER BY received_at DESC, id DESC LIMIT 100",
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await?;
    let deliveries: Vec<DeliveryRow> = sqlx::query_as(
        "SELECT * FROM webhook_deliveries WHERE payment_id = $1 ORDER BY created_at DESC",
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await?;
    let cash_in: Option<CashInRow> = sqlx::query_as(
        "SELECT c.*, p.order_id, pr.name AS product_name
         FROM cash_ins c JOIN payments p ON p.id = c.payment_id JOIN products pr ON pr.id = c.product_id
         WHERE c.payment_id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?;
    Ok(Json(json!({
        "payment": admin_view(&state, &row)?,
        "events": events.into_iter().map(|e| json!({
            "id": e.id,
            "received_at": e.received_at,
            "outcome": e.outcome,
            "verified": e.verified,
            "error": e.error,
        })).collect::<Vec<_>>(),
        "deliveries": deliveries,
        "cash_in": cash_in.map(cash_in_view),
    })))
}

#[derive(Deserialize)]
struct AdminCreate {
    product_id: Uuid,
    mode: Mode,
    #[serde(flatten)]
    request: CreatePaymentRequest,
}

async fn create(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<AdminCreate>,
) -> Result<(StatusCode, Json<AdminPayment>)> {
    let row: Option<(String, Option<String>)> = sqlx::query_as(
        "SELECT name, default_provider FROM products WHERE id = $1 AND archived_at IS NULL",
    )
    .bind(body.product_id)
    .fetch_optional(&state.pool)
    .await?;
    let (name, default_provider) = row.ok_or_else(|| AppError::not_found("product"))?;
    let product = ProductCtx {
        id: body.product_id,
        name,
        default_provider: default_provider.and_then(|p| p.parse().ok()),
    };
    let (row, replayed) = payments::create(&state, &product, body.mode, body.request).await?;
    let status = if replayed {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    Ok((status, Json(admin_view(&state, &row)?)))
}

async fn sync(State(state): State<AppState>, Path(id): Path<Uuid>) -> Result<Json<AdminPayment>> {
    let row = payments::sync(&state, id).await?;
    Ok(Json(admin_view(&state, &row)?))
}

#[derive(Deserialize)]
struct SimulateBody {
    outcome: Outcome,
}

async fn simulate(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    ApiJson(body): ApiJson<SimulateBody>,
) -> Result<Json<AdminPayment>> {
    let row = payments::simulate(&state, id, body.outcome).await?;
    Ok(Json(admin_view(&state, &row)?))
}

#[derive(Debug, Deserialize)]
struct EventsQuery {
    provider_account_id: Option<Uuid>,
    outcome: Option<String>,
    limit: Option<i64>,
    cursor: Option<String>,
}

async fn events(
    State(state): State<AppState>,
    ApiQuery(q): ApiQuery<EventsQuery>,
) -> Result<Json<Value>> {
    let limit = clamp_limit(q.limit);
    let mut qb: QueryBuilder<Postgres> =
        QueryBuilder::new("SELECT * FROM provider_events WHERE 1=1");
    if let Some(a) = q.provider_account_id {
        qb.push(" AND provider_account_id = ").push_bind(a);
    }
    if let Some(o) = q.outcome {
        qb.push(" AND outcome = ").push_bind(o);
    }
    if let Some(raw) = &q.cursor {
        let c = Cursor::decode(raw).ok_or_else(|| AppError::bad_request("invalid cursor"))?;
        qb.push(" AND (received_at, id) < (")
            .push_bind(c.ts)
            .push(", ")
            .push_bind(c.id)
            .push(")");
    }
    qb.push(" ORDER BY received_at DESC, id DESC LIMIT ")
        .push_bind(limit + 1);
    let mut rows: Vec<EventRow> = qb.build_query_as().fetch_all(&state.pool).await?;
    let next_cursor = if rows.len() as i64 > limit {
        rows.truncate(limit as usize);
        rows.last().map(|r| {
            Cursor {
                ts: r.received_at,
                id: r.id,
            }
            .encode()
        })
    } else {
        None
    };
    let data: Vec<Value> = rows
        .into_iter()
        .map(|e| {
            let body = serde_json::from_str::<Value>(&e.body_text)
                .unwrap_or(Value::String(e.body_text.clone()));
            json!({
                "id": e.id,
                "provider": e.provider,
                "provider_account_id": e.provider_account_id,
                "payment_id": e.payment_id,
                "verified": e.verified,
                "outcome": e.outcome,
                "error": e.error,
                "headers": e.headers,
                "body": body,
                "received_at": e.received_at,
            })
        })
        .collect();
    Ok(Json(json!({ "data": data, "next_cursor": next_cursor })))
}

#[derive(Debug, Deserialize)]
struct DeliveriesQuery {
    status: Option<String>,
    product_id: Option<Uuid>,
    payment_id: Option<Uuid>,
    limit: Option<i64>,
    cursor: Option<String>,
}

async fn deliveries(
    State(state): State<AppState>,
    ApiQuery(q): ApiQuery<DeliveriesQuery>,
) -> Result<Json<Value>> {
    let limit = clamp_limit(q.limit);
    let mut qb: QueryBuilder<Postgres> =
        QueryBuilder::new("SELECT * FROM webhook_deliveries WHERE 1=1");
    if let Some(s) = q.status {
        qb.push(" AND status = ").push_bind(s);
    }
    if let Some(p) = q.product_id {
        qb.push(" AND product_id = ").push_bind(p);
    }
    if let Some(p) = q.payment_id {
        qb.push(" AND payment_id = ").push_bind(p);
    }
    if let Some(raw) = &q.cursor {
        let c = Cursor::decode(raw).ok_or_else(|| AppError::bad_request("invalid cursor"))?;
        qb.push(" AND (created_at, id) < (")
            .push_bind(c.ts)
            .push(", ")
            .push_bind(c.id)
            .push(")");
    }
    qb.push(" ORDER BY created_at DESC, id DESC LIMIT ")
        .push_bind(limit + 1);
    let mut rows: Vec<DeliveryRow> = qb.build_query_as().fetch_all(&state.pool).await?;
    let next_cursor = if rows.len() as i64 > limit {
        rows.truncate(limit as usize);
        rows.last().map(|r| {
            Cursor {
                ts: r.created_at,
                id: r.id,
            }
            .encode()
        })
    } else {
        None
    };
    Ok(Json(json!({ "data": rows, "next_cursor": next_cursor })))
}

async fn retry_delivery(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<DeliveryRow>> {
    let row: DeliveryRow = sqlx::query_as(
        "UPDATE webhook_deliveries
         SET status = 'pending', attempts = 0, next_attempt_at = now(), last_error = NULL
         WHERE id = $1 AND status <> 'delivered'
         RETURNING *",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::not_found("pending or failed delivery"))?;
    Ok(Json(row))
}
