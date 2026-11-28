//! Inbound provider webhooks: one URL per provider account.

use axum::Json;
use axum::Router;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::error::{AppError, Result};
use crate::models::PaymentRow;
use crate::providers::{self, Lookup, WebhookError};
use crate::services::accounts;
use crate::services::payments::{self, ApplyResult};
use crate::state::AppState;
use crate::util::new_id;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/{account_id}", post(receive))
        .fallback(crate::error::json_not_found)
}

const REDACTED_HEADERS: [&str; 5] = [
    "authorization",
    "cookie",
    "x-callback-token",
    "idrx-api-key",
    "idrx-api-sig",
];

/// Headers as stored in the inbox. Known credential headers are redacted by name, and so is any
/// header whose value is one of the account's stored secrets (a provider may carry its webhook
/// token in a header of its own choosing).
fn headers_json(headers: &HeaderMap, secrets: &[&str]) -> Value {
    let mut map = serde_json::Map::new();
    for (k, v) in headers {
        let name = k.as_str().to_ascii_lowercase();
        let raw = String::from_utf8_lossy(v.as_bytes());
        let is_secret = secrets.iter().any(|s| !s.is_empty() && raw.contains(s));
        let value = if REDACTED_HEADERS.contains(&name.as_str()) || is_secret {
            "[redacted]".to_string()
        } else {
            raw.into_owned()
        };
        map.insert(name, Value::String(value));
    }
    Value::Object(map)
}

struct Record<'a> {
    account_id: Uuid,
    provider: &'a str,
    payment_id: Option<Uuid>,
    verified: bool,
    outcome: &'static str,
    error: Option<String>,
    /// Stored secret credentials of the account, never written to the inbox.
    secrets: &'a [&'a str],
}

async fn record(state: &AppState, r: Record<'_>, headers: &HeaderMap, body: &[u8]) {
    let res = sqlx::query(
        "INSERT INTO provider_events
           (id, provider_account_id, provider, payment_id, verified, outcome, error, headers, body_text)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)",
    )
    .bind(new_id())
    .bind(r.account_id)
    .bind(r.provider)
    .bind(r.payment_id)
    .bind(r.verified)
    .bind(r.outcome)
    .bind(r.error)
    .bind(headers_json(headers, r.secrets))
    // bodies are small JSON; cap anyway so a hostile caller cannot bloat the table
    .bind(String::from_utf8_lossy(&body[..body.len().min(65_536)]).into_owned())
    .execute(&state.pool)
    .await;
    if let Err(e) = res {
        tracing::error!(error = ?e, "failed to store provider event");
    }
}

async fn find_payment(
    state: &AppState,
    account_id: Uuid,
    lookup: &Lookup,
) -> Result<Option<PaymentRow>> {
    Ok(match lookup {
        Lookup::PaymentId(id) => {
            sqlx::query_as::<_, PaymentRow>(
                "SELECT * FROM payments WHERE id = $1 AND provider_account_id = $2",
            )
            .bind(id)
            .bind(account_id)
            .fetch_optional(&state.pool)
            .await?
        }
        Lookup::ProviderRef(r) if !r.is_empty() => {
            sqlx::query_as::<_, PaymentRow>(
                "SELECT * FROM payments WHERE provider_account_id = $1 AND provider_ref = $2
                 ORDER BY created_at DESC LIMIT 1",
            )
            .bind(account_id)
            .bind(r)
            .fetch_optional(&state.pool)
            .await?
        }
        _ => None,
    })
}

async fn receive(
    State(state): State<AppState>,
    Path(account_id): Path<Uuid>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response> {
    let account = accounts::load(&state, account_id)
        .await
        .map_err(|_| AppError::not_found("webhook endpoint"))?;
    let provider = account.provider;
    let adapter = providers::adapter(provider);
    let ctx = account.ctx(&state);
    let pname = provider.as_str();
    let secrets: Vec<&str> = adapter
        .info()
        .credential_fields
        .iter()
        .filter(|f| f.secret)
        .filter_map(|f| account.creds.get(f.key))
        .collect();

    let parsed = match adapter.parse_webhook(&ctx, &headers, &body).await {
        Ok(p) => p,
        Err(e) => {
            let (status, outcome) = match &e {
                WebhookError::Unauthorized(_) => (StatusCode::UNAUTHORIZED, "rejected"),
                WebhookError::Malformed(_) => (StatusCode::BAD_REQUEST, "rejected"),
                // provider should redeliver once the verification dependency is back
                WebhookError::Upstream(_) => (StatusCode::BAD_GATEWAY, "error"),
            };
            record(
                &state,
                Record {
                    account_id,
                    provider: pname,
                    payment_id: None,
                    verified: false,
                    outcome,
                    error: Some(e.to_string()),
                    secrets: &secrets,
                },
                &headers,
                &body,
            )
            .await;
            tracing::warn!(account = %account_id, provider = pname, error = %e, "webhook not accepted");
            // Upstream detail (transport errors, URLs) stays in the log and the event record.
            let (code, message) = match (status, &e) {
                (StatusCode::UNAUTHORIZED, _) => ("unauthorized", e.to_string()),
                (StatusCode::BAD_REQUEST, _) => ("invalid_request", e.to_string()),
                _ => (
                    "provider_error",
                    "could not verify the webhook; retry later".to_string(),
                ),
            };
            return Ok((
                status,
                Json(json!({ "error": { "code": code, "message": message } })),
            )
                .into_response());
        }
    };

    let payment = find_payment(&state, account_id, &parsed.lookup).await?;
    let (outcome, error, payment_id) = match (&payment, parsed.update) {
        (None, _) => (
            if matches!(parsed.lookup, Lookup::Ignore) {
                "noop"
            } else {
                "unmatched"
            },
            None,
            None,
        ),
        (Some(p), None) => ("noop", None, Some(p.id)),
        (Some(p), Some(update)) => match payments::apply_update(&state, p.id, update).await? {
            ApplyResult::Applied(_) => ("applied", None, Some(p.id)),
            ApplyResult::Noop => ("noop", None, Some(p.id)),
            ApplyResult::Rejected(why) => ("rejected", Some(why), Some(p.id)),
        },
    };
    record(
        &state,
        Record {
            account_id,
            provider: pname,
            payment_id,
            verified: true,
            outcome,
            error,
            secrets: &secrets,
        },
        &headers,
        &body,
    )
    .await;
    Ok(Json(json!({ "ok": true })).into_response())
}
