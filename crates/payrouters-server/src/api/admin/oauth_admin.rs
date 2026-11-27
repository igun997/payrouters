//! Dashboard side of the MCP OAuth flow: the consent page asks here what to show and posts the
//! user's decision, and the Integrations page lists and revokes connections. All of it sits
//! behind the admin token, which is the signed-in dashboard session.

use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::error::{ApiJson, ApiQuery, AppError, Result};
use crate::services::oauth::{self, AuthorizeParams, Scopes, ValidAuthorize};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/oauth/authorize", get(preview).post(decide))
        .route("/oauth/grants", get(grants))
        .route("/oauth/grants/{id}", delete(revoke))
}

fn scope_catalog(requested: Scopes) -> Value {
    json!([
        {
            "key": "read",
            "label": "Read",
            "description": "See products, provider accounts (secrets stay masked), payments, webhook activity and stats.",
            "granted": true,
            "locked": true,
        },
        {
            "key": "write",
            "label": "Make changes",
            "description": "Create and edit products, create and revoke API keys, rotate webhook secrets, create test payments and retry webhook deliveries.",
            "granted": requested.write,
            "locked": false,
        },
        {
            "key": "live",
            "label": "Live mode",
            "description": "Extend reading and changes to live mode: live API keys, live payments and live data. Leave this off to keep the assistant on sandbox data.",
            "granted": requested.live,
            "locked": false,
        },
    ])
}

fn host_of(v: &ValidAuthorize) -> String {
    match v.redirect.host_str() {
        Some(host) => match v.redirect.port() {
            Some(port) => format!("{host}:{port}"),
            None => host.to_string(),
        },
        // custom schemes such as cursor://anysphere... keep their authority-less form
        None => format!("{}:", v.redirect.scheme()),
    }
}

async fn preview(
    State(state): State<AppState>,
    ApiQuery(params): ApiQuery<AuthorizeParams>,
) -> Result<Json<Value>> {
    let v = oauth::validate_authorize(&state, &params).await?;
    Ok(Json(json!({
        "client": { "client_id": v.client_id, "client_name": v.client_name },
        "redirect_host": host_of(&v),
        "resource": state.mcp_url(),
        "scopes": scope_catalog(v.requested),
    })))
}

#[derive(Deserialize)]
struct Decision {
    #[serde(flatten)]
    params: AuthorizeParams,
    decision: String,
}

async fn decide(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<Decision>,
) -> Result<Json<Value>> {
    let v = oauth::validate_authorize(&state, &body.params).await?;
    // In the decision `scope` is what the user ticked, not what the client asked for.
    let approved = match body.decision.as_str() {
        "allow" => Some(Scopes::parse(
            body.params.scope.as_deref().unwrap_or_default(),
        )),
        "deny" => None,
        _ => {
            return Err(AppError::bad_request(
                "decision must be \"allow\" or \"deny\"",
            ));
        }
    };
    let redirect_url = oauth::decide(&state, &v, approved).await?;
    Ok(Json(json!({ "redirect_url": redirect_url })))
}

async fn grants(State(state): State<AppState>) -> Result<Json<Value>> {
    Ok(Json(json!({ "data": oauth::list_grants(&state).await? })))
}

async fn revoke(State(state): State<AppState>, Path(id): Path<Uuid>) -> Result<StatusCode> {
    oauth::revoke_grant(&state, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
