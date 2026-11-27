//! Admin API: `/admin/*` for the dashboard (bearer `ADMIN_TOKEN`).

use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::middleware;
use axum::routing::get;
use chrono::NaiveDate;
use payrouters_types::{MethodInfo, Provider};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Value, json};

use super::auth::require_admin;
use crate::providers::{self, CredentialField};
use crate::state::AppState;

pub(crate) mod oauth_admin;
pub(crate) mod payments;
pub(crate) mod products;
pub(crate) mod providers_admin;
pub mod stats;

pub fn routes(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/me", get(me))
        .route("/meta", get(meta))
        .merge(products::routes())
        .merge(providers_admin::routes())
        .merge(payments::routes())
        .merge(stats::routes())
        .merge(oauth_admin::routes())
        .fallback(crate::error::json_not_found)
        .layer(middleware::from_fn_with_state(state, require_admin))
}

/// `Some(None)` = explicit JSON `null` (clear the field); `None` = field absent (leave alone).
pub fn double_option<'de, T, D>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

pub fn parse_date(label: &str, s: &str) -> crate::error::Result<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map_err(|_| crate::error::AppError::bad_request(format!("{label} must be YYYY-MM-DD")))
}

async fn me(State(state): State<AppState>) -> Json<Value> {
    Json(json!({
        "ok": true,
        "version": env!("CARGO_PKG_VERSION"),
        "public_base_url": state.config.public_base_url,
        "mcp_url": state.mcp_url(),
        "timezone": state.config.timezone.name(),
    }))
}

#[derive(Serialize)]
struct ProviderMeta {
    id: Provider,
    label: &'static str,
    supports_sandbox: bool,
    supports_provider_checkout: bool,
    credential_fields: Vec<CredentialField>,
    methods: Vec<MethodInfo>,
}

async fn meta() -> Json<Value> {
    let providers: Vec<ProviderMeta> = providers::PROVIDER_ORDER
        .into_iter()
        .map(|p| {
            let info = providers::adapter(p).info();
            ProviderMeta {
                id: p,
                label: p.label(),
                supports_sandbox: info.supports_sandbox,
                supports_provider_checkout: info.supports_provider_checkout,
                credential_fields: info.credential_fields,
                methods: info.methods.iter().map(|m| m.info()).collect(),
            }
        })
        .collect();
    let methods: Vec<MethodInfo> = payrouters_types::Method::ALL
        .iter()
        .map(|m| m.info())
        .collect();
    Json(json!({
        "providers": providers,
        "methods": methods,
        "statuses": payrouters_types::PaymentStatus::ALL.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
    }))
}
