use std::collections::BTreeMap;

use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use chrono::{DateTime, Utc};
use payrouters_types::{FeeSchedule, Method, Mode, Provider};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::types::Json as SqlJson;
use uuid::Uuid;

use crate::error::{ApiJson, AppError, Result, is_fk_violation, is_unique_violation};
use crate::models::AccountRow;
use crate::providers::{self, ProviderError};
use crate::services::accounts::{self, AccountRecord, encrypt_creds};
use crate::state::AppState;
use crate::util::{mask_secret, new_id};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/providers", get(list).post(create))
        .route(
            "/providers/{id}",
            axum::routing::patch(update).delete(remove),
        )
        .route("/providers/{id}/test", post(test))
}

#[derive(Serialize)]
struct AccountView {
    id: Uuid,
    provider: Provider,
    mode: Mode,
    label: String,
    enabled: bool,
    credentials: BTreeMap<String, String>,
    fees: FeeSchedule,
    webhook_url: String,
    created_at: DateTime<Utc>,
}

fn view(state: &AppState, rec: &AccountRecord) -> AccountView {
    let fields = providers::adapter(rec.provider).info().credential_fields;
    let credentials = fields
        .iter()
        .map(|f| {
            let raw = rec
                .creds
                .0
                .get(f.key)
                .map(String::as_str)
                .unwrap_or_default();
            let shown = if f.secret {
                mask_secret(raw)
            } else {
                raw.to_string()
            };
            (f.key.to_string(), shown)
        })
        .collect();
    AccountView {
        id: rec.id,
        provider: rec.provider,
        mode: rec.mode,
        label: rec.label.clone(),
        enabled: rec.enabled,
        credentials,
        fees: rec.fees.clone(),
        webhook_url: state.provider_webhook_url(rec.id),
        created_at: rec.created_at,
    }
}

pub(crate) async fn list(State(state): State<AppState>) -> Result<Json<Value>> {
    let rows: Vec<AccountRow> =
        sqlx::query_as("SELECT * FROM provider_accounts ORDER BY provider, mode")
            .fetch_all(&state.pool)
            .await?;
    let mut data = Vec::with_capacity(rows.len());
    for row in rows {
        data.push(view(&state, &AccountRecord::from_row(&state, row)?));
    }
    Ok(Json(json!({ "data": data })))
}

fn validate_fees(fees: &FeeSchedule) -> Result<()> {
    for (key, rule) in fees {
        if key != "*" && key.parse::<Method>().is_err() {
            return Err(AppError::bad_request(format!(
                "fees: unknown method `{key}`"
            )));
        }
        if rule.fixed < 0 || !(0.0..=100.0).contains(&rule.percent) || rule.percent.is_nan() {
            return Err(AppError::bad_request(format!(
                "fees.{key}: fixed must be >= 0 and percent within 0-100"
            )));
        }
    }
    Ok(())
}

fn check_required(provider: Provider, creds: &BTreeMap<String, String>) -> Result<()> {
    for f in providers::adapter(provider).info().credential_fields {
        if f.required && creds.get(f.key).is_none_or(|v| v.trim().is_empty()) {
            return Err(AppError::bad_request(format!(
                "credential `{}` is required",
                f.key
            )));
        }
    }
    Ok(())
}

fn label_or_default(label: Option<String>, provider: Provider, mode: Mode) -> Result<String> {
    match label.map(|l| l.trim().to_string()) {
        Some(l) if l.is_empty() || l.chars().count() > 80 => {
            Err(AppError::bad_request("label must be 1-80 characters"))
        }
        Some(l) => Ok(l),
        None => Ok(format!("{} {mode}", provider.label())),
    }
}

#[derive(Deserialize)]
struct CreateAccount {
    provider: Provider,
    mode: Mode,
    label: Option<String>,
    #[serde(default)]
    credentials: BTreeMap<String, String>,
    #[serde(default)]
    fees: FeeSchedule,
}

async fn create(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<CreateAccount>,
) -> Result<(StatusCode, Json<AccountView>)> {
    let info = providers::adapter(body.provider).info();
    if body.mode == Mode::Live && body.provider == Provider::Mock {
        return Err(AppError::unprocessable(
            "unsupported_mode",
            "the mock provider is sandbox-only",
        ));
    }
    if body.mode == Mode::Sandbox && !info.supports_sandbox {
        return Err(AppError::unprocessable(
            "sandbox_unsupported",
            format!("{} has no sandbox", body.provider.label()),
        ));
    }
    let mut creds = BTreeMap::new();
    for (k, v) in &body.credentials {
        if !info.credential_fields.iter().any(|f| f.key == k) {
            return Err(AppError::bad_request(format!("unknown credential `{k}`")));
        }
        if !v.trim().is_empty() {
            creds.insert(k.clone(), v.trim().to_string());
        }
    }
    check_required(body.provider, &creds)?;
    validate_fees(&body.fees)?;
    let label = label_or_default(body.label, body.provider, body.mode)?;
    let id = new_id();
    let res = sqlx::query(
        "INSERT INTO provider_accounts (id, provider, mode, label, credentials_enc, fees)
         VALUES ($1,$2,$3,$4,$5,$6)",
    )
    .bind(id)
    .bind(body.provider.as_str())
    .bind(body.mode.as_str())
    .bind(label)
    .bind(encrypt_creds(&state, &creds)?)
    .bind(SqlJson(&body.fees))
    .execute(&state.pool)
    .await;
    match res {
        Ok(_) => {}
        Err(e) if is_unique_violation(&e) => {
            return Err(AppError::conflict(
                "provider_account_exists",
                format!(
                    "a {} {} account already exists; edit it instead",
                    body.provider, body.mode
                ),
            ));
        }
        Err(e) => return Err(e.into()),
    }
    let rec = accounts::load(&state, id).await?;
    Ok((StatusCode::CREATED, Json(view(&state, &rec))))
}

#[derive(Deserialize)]
struct UpdateAccount {
    label: Option<String>,
    enabled: Option<bool>,
    credentials: Option<BTreeMap<String, String>>,
    fees: Option<FeeSchedule>,
}

async fn update(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    ApiJson(body): ApiJson<UpdateAccount>,
) -> Result<Json<AccountView>> {
    let rec = accounts::load(&state, id).await?;
    let info = providers::adapter(rec.provider).info();

    let mut merged = rec.creds.0.clone();
    for (k, v) in body.credentials.unwrap_or_default() {
        let field = info
            .credential_fields
            .iter()
            .find(|f| f.key == k)
            .ok_or_else(|| AppError::bad_request(format!("unknown credential `{k}`")))?;
        let v = v.trim().to_string();
        if field.secret {
            let existing = merged.get(&k).map(String::as_str).unwrap_or_default();
            // blank or the masked echo of the stored value means "unchanged"
            if v.is_empty() || v == mask_secret(existing) {
                continue;
            }
            merged.insert(k, v);
        } else if v.is_empty() {
            merged.remove(&k);
        } else {
            merged.insert(k, v);
        }
    }
    check_required(rec.provider, &merged)?;
    let fees = match body.fees {
        Some(f) => {
            validate_fees(&f)?;
            f
        }
        None => rec.fees.clone(),
    };
    let label = match body.label {
        Some(l) => label_or_default(Some(l), rec.provider, rec.mode)?,
        None => rec.label.clone(),
    };
    sqlx::query(
        "UPDATE provider_accounts
         SET label = $2, enabled = $3, credentials_enc = $4, fees = $5, updated_at = now()
         WHERE id = $1",
    )
    .bind(id)
    .bind(label)
    .bind(body.enabled.unwrap_or(rec.enabled))
    .bind(encrypt_creds(&state, &merged)?)
    .bind(SqlJson(&fees))
    .execute(&state.pool)
    .await?;
    // New credentials may have activated what was refused before: ask the provider again.
    providers::availability::clear(id);
    let rec = accounts::load(&state, id).await?;
    Ok(Json(view(&state, &rec)))
}

async fn remove(State(state): State<AppState>, Path(id): Path<Uuid>) -> Result<StatusCode> {
    match sqlx::query("DELETE FROM provider_accounts WHERE id = $1")
        .bind(id)
        .execute(&state.pool)
        .await
    {
        Ok(r) if r.rows_affected() == 0 => Err(AppError::not_found("provider account")),
        Ok(_) => Ok(StatusCode::NO_CONTENT),
        Err(e) if is_fk_violation(&e) => Err(AppError::conflict(
            "provider_account_in_use",
            "payments reference this account; disable it instead of deleting",
        )),
        Err(e) => Err(e.into()),
    }
}

async fn test(State(state): State<AppState>, Path(id): Path<Uuid>) -> Result<Json<Value>> {
    let rec = accounts::load(&state, id).await?;
    let result = providers::adapter(rec.provider)
        .test_connection(&rec.ctx(&state))
        .await;
    Ok(Json(match result {
        Ok(message) => json!({ "ok": true, "message": message }),
        Err(
            ProviderError::Rejected(m)
            | ProviderError::Unavailable(m)
            | ProviderError::ChannelInactive(m)
            | ProviderError::Config(m),
        ) => {
            json!({ "ok": false, "message": m })
        }
    }))
}
