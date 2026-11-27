//! MCP server (Model Context Protocol, Streamable HTTP, JSON responses) at `POST /mcp`.
//!
//! Authentication is an OAuth access token issued by `services::oauth` after the signed-in
//! dashboard session approved the connection. A missing or invalid token answers `401` with a
//! `WWW-Authenticate` header that points at the protected-resource metadata, which is how a
//! client discovers the OAuth flow by itself. There is no server-initiated stream and no
//! session id: `GET` and `DELETE` answer `405`.

pub mod content;
mod tools;

use std::time::Instant;

use axum::Json;
use axum::Router;
use axum::body::Bytes;
use axum::extract::{Extension, State};
use axum::http::header::{ALLOW, AUTHORIZATION, WWW_AUTHENTICATE};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use serde_json::{Value, json};

use crate::error::AppError;
use crate::rate_limit::{ClientKey, OAUTH_FAIL};
use crate::services::oauth::{self, McpGrant};
use crate::state::AppState;

pub struct Ctx {
    pub state: AppState,
    pub grant: McpGrant,
}

const LATEST_PROTOCOL: &str = "2025-06-18";
const PROTOCOLS: [&str; 4] = ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/mcp",
            post(handle)
                .get(method_not_allowed)
                .delete(method_not_allowed),
        )
        .layer(crate::api::oauth::cors())
}

async fn method_not_allowed() -> Response {
    let mut response = StatusCode::METHOD_NOT_ALLOWED.into_response();
    response
        .headers_mut()
        .insert(ALLOW, HeaderValue::from_static("POST"));
    response
}

fn bearer(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .map(str::trim)
        .filter(|t| !t.is_empty())
}

fn unauthorized(state: &AppState, presented: bool) -> Response {
    let metadata = format!(
        "{}/.well-known/oauth-protected-resource/mcp",
        state.config.public_base_url
    );
    let challenge = if presented {
        format!(
            "Bearer error=\"invalid_token\", error_description=\"The access token is invalid, expired or revoked\", resource_metadata=\"{metadata}\", scope=\"read write\""
        )
    } else {
        format!("Bearer resource_metadata=\"{metadata}\", scope=\"read write\"")
    };
    let mut response = (
        StatusCode::UNAUTHORIZED,
        Json(json!({
            "error": "invalid_token",
            "error_description": "Connect through OAuth: the WWW-Authenticate header points at the metadata."
        })),
    )
        .into_response();
    if let Ok(value) = HeaderValue::from_str(&challenge) {
        response.headers_mut().insert(WWW_AUTHENTICATE, value);
    }
    response
}

async fn handle(
    State(state): State<AppState>,
    client: Option<Extension<ClientKey>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let rate = &state.config.rate;
    let key = client.map(|c| c.0.0).unwrap_or_default();
    let now = Instant::now();
    if let Err(retry) = state
        .limiter
        .check_locked(OAUTH_FAIL, &key, rate.api_key_fails, now)
    {
        return AppError::RateLimited(retry).into_response();
    }

    let token = bearer(&headers);
    let grant = match token {
        Some(t) => match oauth::authenticate(&state, t).await {
            Ok(grant) => grant,
            Err(e) => return e.into_response(),
        },
        None => None,
    };
    let Some(grant) = grant else {
        if token.is_some() {
            state.limiter.record_failure(
                OAUTH_FAIL,
                &key,
                rate.api_key_fails,
                rate.fail_window,
                now,
            );
        }
        return unauthorized(&state, token.is_some());
    };

    let message: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(rpc_error(
                    Value::Null,
                    -32700,
                    "Parse error: the body is not valid JSON",
                )),
            )
                .into_response();
        }
    };
    let ctx = Ctx { state, grant };
    match message {
        Value::Array(items) => {
            if items.is_empty() {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(rpc_error(
                        Value::Null,
                        -32600,
                        "Invalid Request: empty batch",
                    )),
                )
                    .into_response();
            }
            let mut replies = Vec::new();
            for item in items {
                if let Some(reply) = process(&ctx, item).await {
                    replies.push(reply);
                }
            }
            if replies.is_empty() {
                StatusCode::ACCEPTED.into_response()
            } else {
                Json(Value::Array(replies)).into_response()
            }
        }
        single => match process(&ctx, single).await {
            Some(reply) => Json(reply).into_response(),
            None => StatusCode::ACCEPTED.into_response(),
        },
    }
}

fn rpc_result(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn rpc_error(id: Value, code: i32, message: impl Into<String>) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message.into() } })
}

/// One JSON-RPC message. `None` for notifications and for responses a client sent us.
async fn process(ctx: &Ctx, msg: Value) -> Option<Value> {
    let Some(obj) = msg.as_object() else {
        return Some(rpc_error(
            Value::Null,
            -32600,
            "Invalid Request: expected an object",
        ));
    };
    let id = obj.get("id").cloned();
    let Some(method) = obj.get("method").and_then(Value::as_str) else {
        // a response (or garbage) from the client: nothing to answer
        return id.map(|id| rpc_error(id, -32600, "Invalid Request: no method"));
    };
    let params = obj.get("params").cloned().unwrap_or(Value::Null);
    let Some(id) = id else {
        // notification (notifications/initialized, notifications/cancelled, ...)
        return None;
    };
    Some(match dispatch(ctx, method, params).await {
        Ok(result) => rpc_result(id, result),
        Err((code, message)) => rpc_error(id, code, message),
    })
}

type RpcResult = Result<Value, (i32, String)>;

async fn dispatch(ctx: &Ctx, method: &str, params: Value) -> RpcResult {
    match method {
        "initialize" => Ok(initialize(&params)),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tools::list() })),
        "tools/call" => {
            let name = params
                .get("name")
                .and_then(Value::as_str)
                .ok_or((-32602, "params.name is required".to_string()))?;
            let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
            tools::call(ctx, name, arguments)
                .await
                .map_err(|m| (-32602, m))
        }
        "resources/list" => Ok(json!({ "resources": resources() })),
        "resources/templates/list" => Ok(json!({ "resourceTemplates": [] })),
        "resources/read" => read_resource(ctx, &params),
        "prompts/list" => Ok(json!({ "prompts": prompts() })),
        "prompts/get" => get_prompt(&params),
        other => Err((-32601, format!("Method not found: {other}"))),
    }
}

fn initialize(params: &Value) -> Value {
    let asked = params.get("protocolVersion").and_then(Value::as_str);
    let version = match asked {
        Some(v) if PROTOCOLS.contains(&v) => v,
        _ => LATEST_PROTOCOL,
    };
    json!({
        "protocolVersion": version,
        "capabilities": {
            "tools": { "listChanged": false },
            "resources": { "listChanged": false, "subscribe": false },
            "prompts": { "listChanged": false }
        },
        "serverInfo": {
            "name": "payrouters",
            "title": "PayRouters",
            "version": env!("CARGO_PKG_VERSION")
        },
        "instructions": content::SERVER_INSTRUCTIONS
    })
}

fn resources() -> Vec<Value> {
    content::TOPICS
        .iter()
        .map(|t| {
            json!({
                "uri": content::uri(t.id),
                "name": t.id,
                "title": t.title,
                "description": t.summary,
                "mimeType": "text/markdown"
            })
        })
        .collect()
}

fn read_resource(ctx: &Ctx, params: &Value) -> RpcResult {
    let uri = params
        .get("uri")
        .and_then(Value::as_str)
        .ok_or((-32602, "params.uri is required".to_string()))?;
    let topic = content::topic_by_uri(uri).ok_or((-32002, format!("Resource not found: {uri}")))?;
    Ok(json!({
        "contents": [{
            "uri": uri,
            "mimeType": "text/markdown",
            "text": topic.render(&ctx.state)
        }]
    }))
}

fn prompts() -> Vec<Value> {
    content::PROMPTS
        .iter()
        .map(|(name, description)| {
            let arguments = match *name {
                "integrate_payrouters" => json!([
                    { "name": "language", "description": "Programming language of the application, e.g. TypeScript, Python, PHP.", "required": false },
                    { "name": "framework", "description": "Framework, e.g. Next.js, Laravel, Django, Express.", "required": false }
                ]),
                "debug_payment" => json!([
                    { "name": "payment_id", "description": "The PayRouters payment id.", "required": false },
                    { "name": "order_id", "description": "The order id, if the payment id is not known.", "required": false }
                ]),
                _ => json!([]),
            };
            json!({ "name": name, "title": name.replace('_', " "), "description": description, "arguments": arguments })
        })
        .collect()
}

fn get_prompt(params: &Value) -> RpcResult {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or((-32602, "params.name is required".to_string()))?;
    let arg = |key: &str| {
        params
            .get("arguments")
            .and_then(|a| a.get(key))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
    };
    let (description, text) = match name {
        "integrate_payrouters" => (
            "Integrate PayRouters payments step by step",
            content::integrate_prompt(arg("language"), arg("framework")),
        ),
        "debug_payment" => (
            "Find out why a payment is not paid or its webhook did not arrive",
            content::debug_prompt(arg("payment_id"), arg("order_id")),
        ),
        "go_live_checklist" => (
            "Check what is left before taking real payments",
            content::GO_LIVE_PROMPT.to_string(),
        ),
        other => return Err((-32602, format!("Unknown prompt: {other}"))),
    };
    Ok(json!({
        "description": description,
        "messages": [{ "role": "user", "content": { "type": "text", "text": text } }]
    }))
}
