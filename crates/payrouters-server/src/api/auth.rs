use std::time::Instant;

use axum::extract::{FromRequestParts, Request, State};
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use axum::middleware::Next;
use axum::response::Response;
use payrouters_types::{Mode, Provider};
use uuid::Uuid;

use crate::error::AppError;
use crate::rate_limit::{ADMIN_FAIL, API_KEY_FAIL, client_key_from};
use crate::services::payments::ProductCtx;
use crate::state::AppState;
use crate::util::{secure_eq, sha256_hex};

fn bearer(parts: &Parts) -> Option<&str> {
    parts
        .headers
        .get(AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .map(str::trim)
}

/// An authenticated product API key. The key's mode decides which data plane it can touch.
#[derive(Debug, Clone)]
pub struct ProductAuth {
    pub product: ProductCtx,
    pub mode: Mode,
}

impl FromRequestParts<AppState> for ProductAuth {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, AppError> {
        let rate = &state.config.rate;
        let client = client_key_from(&parts.extensions).to_string();
        let now = Instant::now();
        state
            .limiter
            .check_locked(API_KEY_FAIL, &client, rate.api_key_fails, now)
            .map_err(AppError::RateLimited)?;
        match authenticate(parts, state).await {
            Err(AppError::Unauthorized(msg)) => {
                state.limiter.record_failure(
                    API_KEY_FAIL,
                    &client,
                    rate.api_key_fails,
                    rate.fail_window,
                    now,
                );
                Err(AppError::Unauthorized(msg))
            }
            other => other,
        }
    }
}

async fn authenticate(parts: &Parts, state: &AppState) -> Result<ProductAuth, AppError> {
    let token =
        bearer(parts).ok_or_else(|| AppError::Unauthorized("missing bearer API key".into()))?;
    let well_formed = token.starts_with("prk_test_") || token.starts_with("prk_live_");
    if !well_formed {
        return Err(AppError::Unauthorized("invalid API key".into()));
    }
    let row: Option<(Uuid, String, Uuid, String, Option<String>)> = sqlx::query_as(
        "SELECT k.id, k.mode, p.id, p.name, p.default_provider
         FROM api_keys k JOIN products p ON p.id = k.product_id
         WHERE k.key_hash = $1 AND k.revoked_at IS NULL AND p.archived_at IS NULL",
    )
    .bind(sha256_hex(token))
    .fetch_optional(&state.pool)
    .await?;
    let (key_id, mode, product_id, name, default_provider) =
        row.ok_or_else(|| AppError::Unauthorized("invalid API key".into()))?;
    let mode: Mode = mode
        .parse()
        .map_err(|e: String| AppError::Internal(anyhow::anyhow!(e)))?;

    let pool = state.pool.clone();
    tokio::spawn(async move {
        let _ = sqlx::query(
            "UPDATE api_keys SET last_used_at = now()
             WHERE id = $1 AND (last_used_at IS NULL OR last_used_at < now() - interval '1 minute')",
        )
        .bind(key_id)
        .execute(&pool)
        .await;
    });

    Ok(ProductAuth {
        product: ProductCtx {
            id: product_id,
            name,
            default_provider: default_provider.and_then(|p| p.parse::<Provider>().ok()),
        },
        mode,
    })
}

/// Guards `/admin/*` with the static admin token. Failed attempts are counted per client; once
/// the limit is reached every request from that client is refused until the window ends,
/// including one carrying the right token.
pub async fn require_admin(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let rate = &state.config.rate;
    let client = client_key_from(req.extensions()).to_string();
    let now = Instant::now();
    state
        .limiter
        .check_locked(ADMIN_FAIL, &client, rate.auth_fails, now)
        .map_err(AppError::RateLimited)?;

    let token = req
        .headers()
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::trim)
        .unwrap_or_default();
    if token.is_empty() || !secure_eq(token, &state.config.admin_token) {
        state
            .limiter
            .record_failure(ADMIN_FAIL, &client, rate.auth_fails, rate.fail_window, now);
        return Err(AppError::Unauthorized("invalid credentials".into()));
    }
    Ok(next.run(req).await)
}
