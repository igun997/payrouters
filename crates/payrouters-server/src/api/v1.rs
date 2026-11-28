//! Product API: `/v1/*`, authenticated with a product API key.

use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::routing::{get, post};
use payrouters_types::{
    CreatePaymentRequest, ListResponse, MethodCatalog, Payment, PaymentStatus, ProviderMethods,
};
use serde::Deserialize;
use sqlx::{Postgres, QueryBuilder};
use uuid::Uuid;

use super::auth::ProductAuth;
use crate::error::{ApiJson, ApiQuery, AppError, Result};
use crate::models::PaymentRow;
use crate::providers;
use crate::services::{accounts, options, payments};
use crate::state::AppState;
use crate::util::Cursor;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/payments", post(create).get(list))
        .route("/payments/{id}", get(get_one))
        .route("/payments/{id}/sync", post(sync))
        .route("/methods", get(methods))
        .fallback(crate::error::json_not_found)
}

pub const MAX_LIMIT: i64 = 200;

pub fn clamp_limit(limit: Option<i64>) -> i64 {
    limit.unwrap_or(50).clamp(1, MAX_LIMIT)
}

async fn owned(state: &AppState, auth: &ProductAuth, id: Uuid) -> Result<PaymentRow> {
    let row = payments::get(state, id).await?;
    if row.product_id != auth.product.id || row.mode != auth.mode.as_str() {
        return Err(AppError::not_found("payment"));
    }
    Ok(row)
}

async fn create(
    State(state): State<AppState>,
    auth: ProductAuth,
    ApiJson(req): ApiJson<CreatePaymentRequest>,
) -> Result<(StatusCode, HeaderMap, Json<Payment>)> {
    let (row, replayed) = payments::create(&state, &auth.product, auth.mode, req).await?;
    let mut headers = HeaderMap::new();
    let status = if replayed {
        headers.insert(
            "PayRouters-Idempotent-Replay",
            HeaderValue::from_static("true"),
        );
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    Ok((
        status,
        headers,
        Json(row.view(&state.config.public_base_url)?),
    ))
}

async fn get_one(
    State(state): State<AppState>,
    auth: ProductAuth,
    Path(id): Path<Uuid>,
) -> Result<Json<Payment>> {
    let row = owned(&state, &auth, id).await?;
    Ok(Json(row.view(&state.config.public_base_url)?))
}

async fn sync(
    State(state): State<AppState>,
    auth: ProductAuth,
    Path(id): Path<Uuid>,
) -> Result<Json<Payment>> {
    owned(&state, &auth, id).await?;
    let row = payments::sync(&state, id).await?;
    Ok(Json(row.view(&state.config.public_base_url)?))
}

#[derive(Debug, Deserialize)]
struct ListQuery {
    order_id: Option<String>,
    status: Option<PaymentStatus>,
    limit: Option<i64>,
    cursor: Option<String>,
}

async fn list(
    State(state): State<AppState>,
    auth: ProductAuth,
    ApiQuery(q): ApiQuery<ListQuery>,
) -> Result<Json<ListResponse<Payment>>> {
    let limit = clamp_limit(q.limit);
    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(payments::SELECT);
    qb.push(" WHERE p.product_id = ").push_bind(auth.product.id);
    qb.push(" AND p.mode = ").push_bind(auth.mode.as_str());
    if let Some(o) = q.order_id {
        qb.push(" AND p.order_id = ").push_bind(o);
    }
    if let Some(s) = q.status {
        qb.push(" AND p.status = ").push_bind(s.as_str());
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
        .map(|r| r.view(&state.config.public_base_url))
        .collect::<Result<Vec<_>>>()?;
    Ok(Json(ListResponse { data, next_cursor }))
}

/// What the product can use in the key's mode: its payment options when it has any with an enabled
/// provider account, otherwise every provider with an enabled account.
async fn methods(State(state): State<AppState>, auth: ProductAuth) -> Result<Json<MethodCatalog>> {
    if let Some(usable) = options::usable(&state, auth.product.id, auth.mode).await?
        && !usable.is_empty()
    {
        let data = options::catalog(&usable)
            .into_iter()
            .map(|(provider, checkout, methods)| ProviderMethods {
                provider,
                checkout_provider: checkout,
                methods: methods.iter().map(|m| m.info()).collect(),
            })
            .collect();
        return Ok(Json(MethodCatalog { data }));
    }
    let enabled = accounts::enabled_providers(&state, auth.mode).await?;
    let data = providers::PROVIDER_ORDER
        .into_iter()
        .filter(|p| enabled.contains(p))
        .map(|p| {
            let info = providers::adapter(p).info();
            ProviderMethods {
                provider: p,
                checkout_provider: info.supports_provider_checkout,
                methods: info.methods.iter().map(|m| m.info()).collect(),
            }
        })
        .collect();
    Ok(Json(MethodCatalog { data }))
}
