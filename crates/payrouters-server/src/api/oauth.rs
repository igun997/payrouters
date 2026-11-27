//! Public OAuth endpoints for the MCP server: discovery documents, dynamic client registration
//! and the token endpoint. The authorization endpoint is a dashboard page (`/oauth/authorize`,
//! served by the SPA) that approves through `/admin/oauth/authorize`.
//!
//! Bearer tokens are not ambient credentials, so these routes answer any origin: browser-based
//! MCP clients can discover, register and exchange tokens.

use std::time::Instant;

use axum::extract::rejection::{FormRejection, JsonRejection};
use axum::extract::{Extension, Form, State};
use axum::http::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, WWW_AUTHENTICATE};
use axum::http::{HeaderName, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{Value, json};
use tower_http::cors::{Any, CorsLayer};

use crate::rate_limit::{ClientKey, OAUTH_FAIL};
use crate::services::oauth::{
    self, OAuthError, RegisterRequest, SCOPES_SUPPORTED, TokenForm, no_store,
};
use crate::state::AppState;

pub fn cors() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::DELETE, Method::OPTIONS])
        .allow_headers([
            AUTHORIZATION,
            CONTENT_TYPE,
            ACCEPT,
            HeaderName::from_static("mcp-protocol-version"),
            HeaderName::from_static("mcp-session-id"),
        ])
        .expose_headers([WWW_AUTHENTICATE, HeaderName::from_static("mcp-session-id")])
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/.well-known/oauth-protected-resource",
            get(protected_resource),
        )
        .route(
            "/.well-known/oauth-protected-resource/mcp",
            get(protected_resource),
        )
        .route(
            "/.well-known/oauth-authorization-server",
            get(authorization_server),
        )
        .route(
            "/.well-known/openid-configuration",
            get(authorization_server),
        )
        .route("/oauth/register", post(register))
        .route("/oauth/token", post(token))
        .layer(cors())
}

async fn protected_resource(State(state): State<AppState>) -> Json<Value> {
    Json(json!({
        "resource": state.mcp_url(),
        "authorization_servers": [state.config.public_base_url],
        "scopes_supported": SCOPES_SUPPORTED,
        "bearer_methods_supported": ["header"],
        "resource_name": "PayRouters",
        "resource_documentation": format!("{}/docs", state.config.public_base_url),
    }))
}

async fn authorization_server(State(state): State<AppState>) -> Json<Value> {
    let base = &state.config.public_base_url;
    Json(json!({
        "issuer": base,
        "authorization_endpoint": format!("{base}/oauth/authorize"),
        "token_endpoint": format!("{base}/oauth/token"),
        "registration_endpoint": format!("{base}/oauth/register"),
        "response_types_supported": ["code"],
        "grant_types_supported": ["authorization_code", "refresh_token"],
        "code_challenge_methods_supported": ["S256"],
        "token_endpoint_auth_methods_supported": ["none"],
        "scopes_supported": SCOPES_SUPPORTED,
        "authorization_response_iss_parameter_supported": true,
        "service_documentation": format!("{base}/docs"),
    }))
}

async fn register(
    State(state): State<AppState>,
    payload: Result<Json<RegisterRequest>, JsonRejection>,
) -> Result<Response, OAuthError> {
    let Json(req) = payload.map_err(|e| {
        OAuthError::new(
            "invalid_client_metadata",
            format!("invalid JSON body: {}", e.body_text()),
        )
    })?;
    let client = oauth::register_client(&state, req).await?;
    let mut response = (StatusCode::CREATED, Json(client)).into_response();
    no_store(&mut response);
    Ok(response)
}

async fn token(
    State(state): State<AppState>,
    client: Option<Extension<ClientKey>>,
    payload: Result<Form<TokenForm>, FormRejection>,
) -> Result<Response, OAuthError> {
    let rate = &state.config.rate;
    let key = client.map(|c| c.0.0).unwrap_or_default();
    let now = Instant::now();
    if let Err(retry) = state
        .limiter
        .check_locked(OAUTH_FAIL, &key, rate.api_key_fails, now)
    {
        let mut response = OAuthError {
            status: StatusCode::TOO_MANY_REQUESTS,
            ..OAuthError::new(
                "slow_down",
                format!("Too many failed attempts. Try again in {retry} seconds."),
            )
        }
        .into_response();
        response.headers_mut().insert(
            axum::http::header::RETRY_AFTER,
            axum::http::HeaderValue::from(retry),
        );
        return Ok(response);
    }
    let Form(form) = payload.map_err(|e| OAuthError::invalid_request(e.body_text()))?;
    match oauth::token(&state, form).await {
        Ok(tokens) => {
            let mut response = Json(tokens).into_response();
            no_store(&mut response);
            Ok(response)
        }
        Err(e) => {
            if matches!(e.error, "invalid_grant" | "invalid_client") {
                state.limiter.record_failure(
                    OAUTH_FAIL,
                    &key,
                    rate.api_key_fails,
                    rate.fail_window,
                    now,
                );
            }
            Err(e)
        }
    }
}
