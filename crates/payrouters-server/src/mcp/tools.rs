//! The MCP tools. Each one wraps the same code the dashboard uses (the admin handlers and the
//! payment services), so an assistant and a person see identical behaviour and validation.
//!
//! Permissions: `read` tools are always available; `write` tools need the `write` scope; anything
//! that touches live mode needs the `live` scope. Without `live` every tool is confined to
//! sandbox data: lists are filtered, live keys are hidden and a `mode: "live"` argument is
//! refused with an explanation.

use std::time::Duration;

use axum::Json;
use axum::extract::{Path, State};
use payrouters_types::{Method, Mode, Outcome};
use serde::de::DeserializeOwned;
use serde_json::{Map, Value, json};
use uuid::Uuid;

use super::Ctx;
use super::content;
use crate::api::admin::payments::{self, AdminCreate, DeliveriesQuery, EventsQuery, ListQuery};
use crate::api::admin::products::{self, CreateKey, CreateProduct, UpdateProduct};
use crate::api::admin::{providers_admin, stats};
use crate::error::{ApiJson, ApiQuery, AppError};
use crate::models::DeliveryRow;
use crate::providers;
use crate::services::{accounts, outbound};
use crate::util::random_hex;

/// A failed tool call: shown to the assistant as the tool result with `isError: true`.
pub struct ToolError(pub String);

impl From<AppError> for ToolError {
    fn from(e: AppError) -> Self {
        ToolError(match e {
            AppError::Internal(_) => "PayRouters hit an internal error. Try again.".into(),
            AppError::Conflict { code, message } | AppError::Unprocessable { code, message } => {
                format!("{code}: {message}")
            }
            other => other.to_string(),
        })
    }
}

impl From<sqlx::Error> for ToolError {
    fn from(e: sqlx::Error) -> Self {
        AppError::from(e).into()
    }
}

pub struct ToolOutput {
    pub text: String,
    pub data: Value,
}

type TResult = Result<ToolOutput, ToolError>;

fn fail<T>(message: impl Into<String>) -> Result<T, ToolError> {
    Err(ToolError(message.into()))
}

fn pretty(v: &Value) -> String {
    serde_json::to_string_pretty(v).unwrap_or_default()
}

fn out(summary: impl Into<String>, data: Value) -> TResult {
    Ok(ToolOutput {
        text: format!("{}\n\n{}", summary.into(), pretty(&data)),
        data,
    })
}

fn to_value<T: serde::Serialize>(v: &T) -> Result<Value, ToolError> {
    serde_json::to_value(v).map_err(|e| ToolError(format!("could not encode the result: {e}")))
}

// --- Definitions --------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Need {
    Read,
    Write,
}

struct Def {
    name: &'static str,
    title: &'static str,
    description: &'static str,
    schema: fn() -> Value,
    need: Need,
    idempotent: bool,
}

fn mode_prop() -> Value {
    json!({
        "type": "string",
        "enum": ["sandbox", "live"],
        "description": "Default sandbox. live needs the live permission granted when this connection was approved."
    })
}

fn id_prop(what: &str) -> Value {
    json!({ "type": "string", "format": "uuid", "description": what })
}

fn limit_prop() -> Value {
    json!({ "type": "integer", "minimum": 1, "maximum": 50, "description": "Rows to return, default 20." })
}

fn object(properties: Value, required: &[&str]) -> Value {
    json!({ "type": "object", "properties": properties, "required": required })
}

const DEFS: &[Def] = &[
    Def {
        name: "get_integration_status",
        title: "Integration status and next steps",
        description: "Start here. Shows the base URL, which provider accounts are enabled per mode, every product with its webhook URL and active API keys, and an ordered list of next_steps to finish an integration. Read-only.",
        schema: || object(json!({}), &[]),
        need: Need::Read,
        idempotent: true,
    },
    Def {
        name: "get_env_template",
        title: "Environment variables for a product's backend",
        description: "The .env block a backend needs (PAYROUTERS_BASE_URL, PAYROUTERS_API_KEY, PAYROUTERS_WEBHOOK_SECRET) with the real base URL filled in and where each secret comes from. With product_id it also lists the product's existing key prefixes and webhook URL.",
        schema: || {
            object(
                json!({ "product_id": id_prop("Optional product to describe."), "mode": mode_prop() }),
                &[],
            )
        },
        need: Need::Read,
        idempotent: true,
    },
    Def {
        name: "list_products",
        title: "List products",
        description: "Products (apps) with webhook URL, default provider and API keys (prefix, mode, status; the full key is never shown again). Without the live permission live keys are hidden.",
        schema: || object(json!({}), &[]),
        need: Need::Read,
        idempotent: true,
    },
    Def {
        name: "list_provider_accounts",
        title: "List provider accounts",
        description: "Midtrans, Xendit, IDRX and mock accounts per mode with enabled state, fee schedule and the inbound webhook URL to paste into the provider dashboard. Secrets are masked. Credentials can only be added in the dashboard.",
        schema: || object(json!({}), &[]),
        need: Need::Read,
        idempotent: true,
    },
    Def {
        name: "list_payment_methods",
        title: "Payment methods available in a mode",
        description: "Per enabled provider account of the mode: the standard method codes it supports (va_bca, va_bni, va_bri, va_mandiri, va_permata, qris) and whether it offers a provider-hosted checkout. Use it to pick valid provider and method values.",
        schema: || object(json!({ "mode": mode_prop() }), &[]),
        need: Need::Read,
        idempotent: true,
    },
    Def {
        name: "list_payments",
        title: "List payments",
        description: "Newest first, compact rows. Filter by status, product, provider or a search term (order id, payment id, provider reference). Use get_payment for one payment in full.",
        schema: || {
            object(
                json!({
                    "mode": mode_prop(),
                    "status": { "type": "string", "enum": ["pending", "paid", "expired", "failed", "cancelled"] },
                    "product_id": id_prop("Only this product."),
                    "provider": { "type": "string", "enum": ["midtrans", "xendit", "idrx", "mock"] },
                    "q": { "type": "string", "description": "Matches order_id, payment id and provider reference." },
                    "limit": limit_prop(),
                    "cursor": { "type": "string", "description": "next_cursor from the previous page." }
                }),
                &[],
            )
        },
        need: Need::Read,
        idempotent: true,
    },
    Def {
        name: "get_payment",
        title: "Get one payment",
        description: "A payment in full with its provider events, webhook deliveries to the product and the cash-in ledger row. Give payment_id, or order_id for the newest payment with that order id.",
        schema: || {
            object(
                json!({
                    "payment_id": id_prop("The payment id."),
                    "order_id": { "type": "string", "description": "Alternative to payment_id." }
                }),
                &[],
            )
        },
        need: Need::Read,
        idempotent: true,
    },
    Def {
        name: "list_webhook_deliveries",
        title: "List webhook deliveries to products",
        description: "PayRouters to product webhook deliveries: status (pending, delivered, failed), attempts, next attempt, last HTTP status and last error. The first place to look when a product says it never got a payment result.",
        schema: || {
            object(
                json!({
                    "product_id": id_prop("Only this product."),
                    "payment_id": id_prop("Only this payment."),
                    "status": { "type": "string", "enum": ["pending", "delivered", "failed"] },
                    "limit": limit_prop()
                }),
                &[],
            )
        },
        need: Need::Read,
        idempotent: true,
    },
    Def {
        name: "list_inbound_webhook_events",
        title: "List inbound provider webhook events",
        description: "What Midtrans, Xendit and IDRX sent to PayRouters, with outcome (applied, noop, unmatched, rejected, error), whether the signature verified and the error. Set include_body to see headers and body.",
        schema: || {
            object(
                json!({
                    "provider_account_id": id_prop("Only this provider account."),
                    "outcome": { "type": "string", "enum": ["applied", "noop", "unmatched", "rejected", "error"] },
                    "include_body": { "type": "boolean", "description": "Include headers and body. Default false." },
                    "limit": limit_prop()
                }),
                &[],
            )
        },
        need: Need::Read,
        idempotent: true,
    },
    Def {
        name: "get_stats_summary",
        title: "Payment and cash-in totals",
        description: "Payments by status, conversion rate and cash-in gross, fee and net for a date range (YYYY-MM-DD in the server's timezone, default last 30 days) compared with the previous range.",
        schema: || {
            object(
                json!({
                    "mode": mode_prop(),
                    "from": { "type": "string", "description": "YYYY-MM-DD, inclusive." },
                    "to": { "type": "string", "description": "YYYY-MM-DD, inclusive." },
                    "product_id": id_prop("Only this product."),
                    "provider": { "type": "string", "enum": ["midtrans", "xendit", "idrx", "mock"] }
                }),
                &[],
            )
        },
        need: Need::Read,
        idempotent: true,
    },
    Def {
        name: "get_docs",
        title: "PayRouters documentation",
        description: "The integration documentation as markdown with this deployment's base URL filled in. Topics: overview, authentication, create-payment, checkout-and-methods, payment-status, webhooks, errors-and-limits, sandbox, providers. Omit topic to list them. The same text is available as payrouters://docs/<topic> resources.",
        schema: || {
            object(
                json!({ "topic": { "type": "string", "enum": content::topic_ids() } }),
                &[],
            )
        },
        need: Need::Read,
        idempotent: true,
    },
    Def {
        name: "generate_code",
        title: "Ready-to-paste integration code",
        description: "Working code with this deployment's base URL for one task in one language: create_payment, verify_webhook (HMAC signature on the raw body, timestamp tolerance, constant-time compare) or get_payment. Languages: curl, typescript (fetch, no dependencies), python, php. Adapt it to the project's conventions.",
        schema: || {
            object(
                json!({
                    "task": { "type": "string", "enum": content::TASKS },
                    "language": { "type": "string", "enum": content::LANGUAGES }
                }),
                &["task", "language"],
            )
        },
        need: Need::Read,
        idempotent: true,
    },
    Def {
        name: "create_product",
        title: "Create a product",
        description: "Creates a product (an app that takes payments). Returns the product and its webhook secret (whsec_...), which is shown ONCE: tell the user to store it as PAYROUTERS_WEBHOOK_SECRET in their backend's environment. webhook_url is the public URL of the backend endpoint that should receive payment results (it must be reachable from this server, so not localhost).",
        schema: || {
            object(
                json!({
                    "name": { "type": "string", "description": "1-80 characters, e.g. the app's name." },
                    "webhook_url": { "type": "string", "description": "http(s) URL that receives payment.* events." },
                    "default_provider": { "type": "string", "enum": ["midtrans", "xendit", "idrx", "mock"], "description": "Used when a payment names no provider." }
                }),
                &["name"],
            )
        },
        need: Need::Write,
        idempotent: false,
    },
    Def {
        name: "update_product",
        title: "Update a product",
        description: "Changes a product's name, webhook_url or default_provider; send null to clear webhook_url or default_provider. A product that already has live API keys can only be changed with the live permission.",
        schema: || {
            object(
                json!({
                    "product_id": id_prop("The product."),
                    "name": { "type": "string" },
                    "webhook_url": { "type": ["string", "null"] },
                    "default_provider": { "type": ["string", "null"], "enum": ["midtrans", "xendit", "idrx", "mock", null] }
                }),
                &["product_id"],
            )
        },
        need: Need::Write,
        idempotent: true,
    },
    Def {
        name: "rotate_webhook_secret",
        title: "Rotate a product's webhook secret",
        description: "Issues a new whsec_... secret (shown once) and invalidates the old one at once, for sandbox and live traffic alike: the product's backend must be updated before the next delivery. A product that already has live API keys needs the live permission.",
        schema: || {
            object(
                json!({ "product_id": id_prop("The product.") }),
                &["product_id"],
            )
        },
        need: Need::Write,
        idempotent: false,
    },
    Def {
        name: "create_api_key",
        title: "Create an API key",
        description: "Creates a product API key (prk_test_... for sandbox, prk_live_... for live). The full key is returned ONCE: tell the user to store it as PAYROUTERS_API_KEY in their backend's environment. Live keys need the live permission.",
        schema: || {
            object(
                json!({
                    "product_id": id_prop("The product."),
                    "name": { "type": "string", "description": "A label such as backend or staging." },
                    "mode": mode_prop()
                }),
                &["product_id", "name"],
            )
        },
        need: Need::Write,
        idempotent: false,
    },
    Def {
        name: "revoke_api_key",
        title: "Revoke an API key",
        description: "Revokes one API key at once. Get key ids from list_products. Revoking a live key needs the live permission.",
        schema: || {
            object(
                json!({ "product_id": id_prop("The product."), "key_id": id_prop("The key.") }),
                &["product_id", "key_id"],
            )
        },
        need: Need::Write,
        idempotent: true,
    },
    Def {
        name: "create_test_payment",
        title: "Create a payment",
        description: "Creates a payment for a product, like the dashboard Playground (no API key needed). In sandbox use provider mock with a method (for example va_bca) to get instructions you can mark paid with simulate_payment. order_id is generated when omitted. Returns the payment and its snap_url. Live mode creates a real payment and needs the live permission.",
        schema: || {
            object(
                json!({
                    "product_id": id_prop("The product."),
                    "amount": { "type": "integer", "minimum": 1, "description": "Rupiah. IDRX needs at least 20000." },
                    "mode": mode_prop(),
                    "order_id": { "type": "string", "description": "Up to 64 characters, unique per product and mode. Generated when omitted." },
                    "provider": { "type": "string", "enum": ["midtrans", "xendit", "idrx", "mock"] },
                    "checkout": { "type": "string", "enum": ["snap", "provider"] },
                    "method": { "type": "string", "enum": Method::ALL.iter().map(|m| m.as_str()).collect::<Vec<_>>() },
                    "description": { "type": "string" },
                    "customer": { "type": "object", "properties": { "name": { "type": "string" }, "email": { "type": "string" }, "phone": { "type": "string" } } },
                    "return_url": { "type": "string" },
                    "expires_in": { "type": "integer", "minimum": 300, "description": "Seconds, default 86400." }
                }),
                &["product_id", "amount"],
            )
        },
        need: Need::Write,
        idempotent: false,
    },
    Def {
        name: "simulate_payment",
        title: "Simulate a payment outcome",
        description: "Moves a pending mock-provider payment to paid, expired or failed through the same pipeline as a real provider (ledger, webhook to the product). Paid needs the payment to have a method.",
        schema: || {
            object(
                json!({
                    "payment_id": id_prop("A pending mock payment."),
                    "outcome": { "type": "string", "enum": ["paid", "expired", "failed"] }
                }),
                &["payment_id", "outcome"],
            )
        },
        need: Need::Write,
        idempotent: false,
    },
    Def {
        name: "sync_payment",
        title: "Re-read a payment from its provider",
        description: "Asks the provider for the current status and applies any change. Use it when a webhook was missed.",
        schema: || {
            object(
                json!({ "payment_id": id_prop("The payment.") }),
                &["payment_id"],
            )
        },
        need: Need::Write,
        idempotent: true,
    },
    Def {
        name: "retry_webhook_delivery",
        title: "Retry a webhook delivery",
        description: "Queues a pending or failed delivery for another attempt now. Get delivery ids from list_webhook_deliveries or get_payment.",
        schema: || {
            object(
                json!({ "delivery_id": id_prop("The delivery.") }),
                &["delivery_id"],
            )
        },
        need: Need::Write,
        idempotent: false,
    },
    Def {
        name: "test_webhook_integration",
        title: "Test a product's webhook endpoint end to end",
        description: "Sandbox only. Creates a mock payment for the product, marks it paid and reports what the product's webhook endpoint answered (HTTP status, error, attempts) with advice. Needs the product to have a webhook_url. A 2xx answer means the signed payment.paid event arrived; it does not prove the endpoint verified the signature, so check that in the endpoint's code.",
        schema: || {
            object(
                json!({ "product_id": id_prop("The product to test.") }),
                &["product_id"],
            )
        },
        need: Need::Write,
        idempotent: false,
    },
];

pub fn list() -> Vec<Value> {
    DEFS.iter()
        .map(|d| {
            json!({
                "name": d.name,
                "title": d.title,
                "description": d.description,
                "inputSchema": (d.schema)(),
                "annotations": {
                    "title": d.title,
                    "readOnlyHint": d.need == Need::Read,
                    "destructiveHint": false,
                    "idempotentHint": d.idempotent,
                    "openWorldHint": false
                }
            })
        })
        .collect()
}

/// `tools/call`: the MCP result object. Failures are results with `isError`, not protocol errors.
pub async fn call(ctx: &Ctx, name: &str, arguments: Value) -> Result<Value, String> {
    let Some(def) = DEFS.iter().find(|d| d.name == name) else {
        return Err(format!("Unknown tool: {name}"));
    };
    let result = if def.need == Need::Write && !ctx.grant.scopes.write {
        fail(
            "This connection can only read. Ask the user to disconnect PayRouters in their MCP client and connect again with \"Make changes\" allowed on the consent screen.",
        )
    } else {
        let args = if arguments.is_null() {
            json!({})
        } else {
            arguments
        };
        if args.is_object() {
            dispatch(ctx, name, args).await
        } else {
            fail("arguments must be a JSON object")
        }
    };
    Ok(match result {
        Ok(o) => json!({
            "content": [{ "type": "text", "text": o.text }],
            "structuredContent": o.data,
            "isError": false
        }),
        Err(ToolError(message)) => json!({
            "content": [{ "type": "text", "text": message }],
            "isError": true
        }),
    })
}

async fn dispatch(ctx: &Ctx, name: &str, args: Value) -> TResult {
    match name {
        "get_integration_status" => integration_status(ctx).await,
        "get_env_template" => env_template(ctx, &args).await,
        "list_products" => list_products(ctx).await,
        "list_provider_accounts" => list_provider_accounts(ctx).await,
        "list_payment_methods" => list_payment_methods(ctx, &args).await,
        "list_payments" => list_payments(ctx, args).await,
        "get_payment" => get_payment(ctx, &args).await,
        "list_webhook_deliveries" => list_deliveries(ctx, args).await,
        "list_inbound_webhook_events" => list_events(ctx, args).await,
        "get_stats_summary" => stats_summary(ctx, args).await,
        "get_docs" => get_docs(ctx, &args),
        "generate_code" => generate_code(ctx, &args),
        "create_product" => create_product(ctx, args).await,
        "update_product" => update_product(ctx, args).await,
        "rotate_webhook_secret" => rotate_secret(ctx, &args).await,
        "create_api_key" => create_api_key(ctx, &args).await,
        "revoke_api_key" => revoke_api_key(ctx, &args).await,
        "create_test_payment" => create_test_payment(ctx, args).await,
        "simulate_payment" => simulate_payment(ctx, &args).await,
        "sync_payment" => sync_payment(ctx, &args).await,
        "retry_webhook_delivery" => retry_delivery(ctx, &args).await,
        "test_webhook_integration" => test_webhook(ctx, &args).await,
        other => fail(format!("Unknown tool: {other}")),
    }
}

// --- Argument helpers ---------------------------------------------------------------------------

fn parse<T: DeserializeOwned>(v: Value) -> Result<T, ToolError> {
    serde_json::from_value(v).map_err(|e| ToolError(format!("Invalid arguments: {e}")))
}

fn str_arg<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

fn uuid_arg(args: &Value, key: &str) -> Result<Uuid, ToolError> {
    let raw = str_arg(args, key).ok_or_else(|| ToolError(format!("{key} is required")))?;
    raw.parse()
        .map_err(|_| ToolError(format!("{key} must be a UUID, got \"{raw}\"")))
}

fn opt_uuid(args: &Value, key: &str) -> Result<Option<Uuid>, ToolError> {
    match str_arg(args, key) {
        None => Ok(None),
        Some(_) => uuid_arg(args, key).map(Some),
    }
}

const LIVE_HINT: &str = "Live mode needs the live permission. Ask the user to disconnect PayRouters in their MCP client and connect again with \"Live mode\" ticked on the consent screen.";

/// The mode a tool works in: sandbox unless the call names live and the connection may use it.
fn mode_of(ctx: &Ctx, args: &Value) -> Result<Mode, ToolError> {
    let mode = match str_arg(args, "mode") {
        None | Some("sandbox") => Mode::Sandbox,
        Some("live") => Mode::Live,
        Some(other) => {
            return fail(format!(
                "mode must be \"sandbox\" or \"live\", got \"{other}\""
            ));
        }
    };
    need_mode(ctx, mode)?;
    Ok(mode)
}

fn need_mode(ctx: &Ctx, mode: Mode) -> Result<(), ToolError> {
    if mode == Mode::Live && !ctx.grant.scopes.live {
        return fail(LIVE_HINT);
    }
    Ok(())
}

fn limit_of(args: &Value) -> i64 {
    args.get("limit")
        .and_then(Value::as_i64)
        .unwrap_or(20)
        .clamp(1, 50)
}

fn pick(v: &Value, keys: &[&str]) -> Value {
    let mut out = Map::new();
    for k in keys {
        if let Some(x) = v.get(*k) {
            out.insert((*k).to_string(), x.clone());
        }
    }
    Value::Object(out)
}

fn array(v: &Value, key: &str) -> Vec<Value> {
    v.get(key)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

/// Drops live keys from a product unless the connection has the live permission.
fn scrub_product(ctx: &Ctx, product: &mut Value) {
    if ctx.grant.scopes.live {
        return;
    }
    if let Some(keys) = product.get_mut("api_keys").and_then(Value::as_array_mut) {
        keys.retain(|k| k.get("mode").and_then(Value::as_str) != Some("live"));
    }
}

/// Products with live traffic (an active live key) are read-only without the live permission:
/// their webhook URL and secret serve live payments too.
async fn ensure_product_writable(ctx: &Ctx, product_id: Uuid) -> Result<(), ToolError> {
    if ctx.grant.scopes.live {
        return Ok(());
    }
    let (live_keys,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM api_keys WHERE product_id = $1 AND mode = 'live' AND revoked_at IS NULL",
    )
    .bind(product_id)
    .fetch_one(&ctx.state.pool)
    .await?;
    if live_keys > 0 {
        return fail(format!(
            "This product already serves live traffic, so changing it needs the live permission. {LIVE_HINT}"
        ));
    }
    Ok(())
}

// --- Read tools ---------------------------------------------------------------------------------

async fn integration_status(ctx: &Ctx) -> TResult {
    let st = &ctx.state;
    let live = ctx.grant.scopes.live;

    let Json(accounts_json) = providers_admin::list(State(st.clone())).await?;
    let accounts: Vec<Value> = array(&accounts_json, "data")
        .into_iter()
        .filter(|a| live || a.get("mode").and_then(Value::as_str) == Some("sandbox"))
        .collect();
    let account_rows: Vec<Value> = accounts
        .iter()
        .map(|a| {
            pick(
                a,
                &["id", "provider", "mode", "label", "enabled", "webhook_url"],
            )
        })
        .collect();

    let Json(products_json) = products::list(State(st.clone())).await?;
    let mut product_rows = Vec::new();
    let mut steps: Vec<String> = Vec::new();
    for mut p in array(&products_json, "data") {
        scrub_product(ctx, &mut p);
        let keys = array(&p, "api_keys");
        let active = |mode: &str| {
            keys.iter()
                .filter(|k| {
                    k.get("mode").and_then(Value::as_str) == Some(mode)
                        && k.get("revoked_at").is_none_or(Value::is_null)
                })
                .count()
        };
        let (sandbox_keys, live_keys) = (active("sandbox"), active("live"));
        let name = p
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("?")
            .to_string();
        let webhook = p.get("webhook_url").filter(|w| !w.is_null()).cloned();
        if webhook.is_none() {
            steps.push(format!("Product \"{name}\" has no webhook_url: set one with update_product so payment results reach your backend."));
        }
        if sandbox_keys == 0 {
            steps.push(format!("Product \"{name}\" has no active sandbox API key: call create_api_key (mode sandbox)."));
        }
        let mut row = pick(
            &p,
            &["id", "name", "slug", "webhook_url", "default_provider"],
        );
        row["active_keys"] = if live {
            json!({ "sandbox": sandbox_keys, "live": live_keys })
        } else {
            json!({ "sandbox": sandbox_keys })
        };
        product_rows.push(row);
    }

    let real_sandbox = accounts.iter().any(|a| {
        a.get("mode").and_then(Value::as_str) == Some("sandbox")
            && a.get("enabled").and_then(Value::as_bool) == Some(true)
            && a.get("provider").and_then(Value::as_str) != Some("mock")
    });
    if product_rows.is_empty() {
        steps.insert(0, "No product yet: call create_product (a name, and webhook_url = the public URL of the backend endpoint that should receive payment results).".into());
    }
    if !real_sandbox {
        steps.push("Only the built-in mock simulator is available in sandbox. To test a real provider, add a Midtrans or Xendit sandbox account in the dashboard (Providers); credentials are never entered through MCP.".into());
    }
    if steps.iter().all(|s| s.contains("mock simulator")) && !product_rows.is_empty() {
        steps.insert(0, "Prove the integration: create_test_payment (provider mock, a method), simulate_payment, then test_webhook_integration.".into());
    }

    let data = json!({
        "base_url": st.config.public_base_url,
        "mcp_url": st.mcp_url(),
        "permissions": ctx.grant.scopes.list(),
        "live_access": live,
        "provider_accounts": account_rows,
        "products": product_rows,
        "next_steps": steps,
    });
    let summary = format!(
        "{} product(s), {} provider account(s). Next steps:\n{}",
        data["products"].as_array().map_or(0, Vec::len),
        data["provider_accounts"].as_array().map_or(0, Vec::len),
        steps
            .iter()
            .enumerate()
            .map(|(i, s)| format!("{}. {s}", i + 1))
            .collect::<Vec<_>>()
            .join("\n")
    );
    out(summary, data)
}

async fn env_template(ctx: &Ctx, args: &Value) -> TResult {
    let mode = mode_of(ctx, args)?;
    let tag = if mode == Mode::Live { "live" } else { "test" };
    let base = &ctx.state.config.public_base_url;
    let mut text = format!(
        "# Backend environment ({mode})\nPAYROUTERS_BASE_URL={base}\nPAYROUTERS_API_KEY=prk_{tag}_...        # create_api_key returns it once; PayRouters cannot show it again\nPAYROUTERS_WEBHOOK_SECRET=whsec_...    # create_product or rotate_webhook_secret returns it once\n",
        mode = mode.as_str()
    );
    let mut data = json!({
        "mode": mode.as_str(),
        "variables": {
            "PAYROUTERS_BASE_URL": base,
            "PAYROUTERS_API_KEY": format!("prk_{tag}_... (from create_api_key)"),
            "PAYROUTERS_WEBHOOK_SECRET": "whsec_... (from create_product or rotate_webhook_secret)"
        }
    });
    if let Some(id) = opt_uuid(args, "product_id")? {
        let row: Option<(String, Option<String>)> = sqlx::query_as(
            "SELECT name, webhook_url FROM products WHERE id = $1 AND archived_at IS NULL",
        )
        .bind(id)
        .fetch_optional(&ctx.state.pool)
        .await?;
        let Some((name, webhook_url)) = row else {
            return fail("Product not found.");
        };
        let keys: Vec<(String, String)> = sqlx::query_as(
            "SELECT prefix, name FROM api_keys
             WHERE product_id = $1 AND mode = $2 AND revoked_at IS NULL ORDER BY created_at",
        )
        .bind(id)
        .bind(mode.as_str())
        .fetch_all(&ctx.state.pool)
        .await?;
        text.push_str(&format!("\n# Product: {name}\n"));
        match &webhook_url {
            Some(url) => text.push_str(&format!("# Webhook endpoint registered: {url}\n")),
            None => text.push_str(
                "# No webhook endpoint registered yet: update_product sets webhook_url\n",
            ),
        }
        if keys.is_empty() {
            text.push_str(&format!(
                "# No active {mode} key yet: create_api_key makes one\n",
                mode = mode.as_str()
            ));
        }
        for (prefix, key_name) in &keys {
            text.push_str(&format!(
                "# Existing {} key \"{key_name}\": {prefix}... (full value not recoverable)\n",
                mode.as_str()
            ));
        }
        data["product"] = json!({ "id": id, "name": name, "webhook_url": webhook_url, "active_key_prefixes": keys.iter().map(|(p, _)| p).collect::<Vec<_>>() });
    }
    text.push_str("\nKeep these in the backend's environment or secret manager, never in source control or a browser app.");
    Ok(ToolOutput { text, data })
}

async fn list_products(ctx: &Ctx) -> TResult {
    let Json(v) = products::list(State(ctx.state.clone())).await?;
    let mut data = array(&v, "data");
    for p in &mut data {
        scrub_product(ctx, p);
    }
    out(
        format!("{} product(s).", data.len()),
        json!({ "data": data }),
    )
}

async fn list_provider_accounts(ctx: &Ctx) -> TResult {
    let Json(v) = providers_admin::list(State(ctx.state.clone())).await?;
    let data: Vec<Value> = array(&v, "data")
        .into_iter()
        .filter(|a| {
            ctx.grant.scopes.live || a.get("mode").and_then(Value::as_str) == Some("sandbox")
        })
        .collect();
    out(
        format!("{} provider account(s).", data.len()),
        json!({ "data": data }),
    )
}

async fn list_payment_methods(ctx: &Ctx, args: &Value) -> TResult {
    let mode = mode_of(ctx, args)?;
    let enabled = accounts::enabled_providers(&ctx.state, mode).await?;
    let providers: Vec<Value> = enabled
        .into_iter()
        .map(|p| {
            let info = providers::adapter(p).info();
            json!({
                "provider": p.as_str(),
                "label": p.label(),
                "checkout_provider": info.supports_provider_checkout,
                "methods": info.methods.iter().map(|m| m.info()).collect::<Vec<_>>(),
            })
        })
        .collect();
    let summary = if providers.is_empty() {
        format!(
            "No enabled provider account in {} mode. Add one in the dashboard (Providers).",
            mode.as_str()
        )
    } else {
        format!(
            "{} enabled provider(s) in {} mode.",
            providers.len(),
            mode.as_str()
        )
    };
    out(
        summary,
        json!({ "mode": mode.as_str(), "providers": providers }),
    )
}

const PAYMENT_ROW: &[&str] = &[
    "id",
    "order_id",
    "status",
    "mode",
    "amount",
    "fee_amount",
    "net_amount",
    "provider",
    "method",
    "checkout",
    "created_at",
    "paid_at",
    "expires_at",
];

async fn list_payments(ctx: &Ctx, mut args: Value) -> TResult {
    let mode = mode_of(ctx, &args)?;
    args["mode"] = json!(mode.as_str());
    args["limit"] = json!(limit_of(&args));
    let query: ListQuery = parse(args)?;
    let Json(page) = payments::list(State(ctx.state.clone()), ApiQuery(query)).await?;
    let page = to_value(&page)?;
    let rows: Vec<Value> = array(&page, "data")
        .iter()
        .map(|p| {
            let mut row = pick(p, PAYMENT_ROW);
            row["product"] = p
                .get("product")
                .and_then(|x| x.get("name"))
                .cloned()
                .unwrap_or(Value::Null);
            row
        })
        .collect();
    out(
        format!(
            "{} payment(s) in {} mode, newest first.",
            rows.len(),
            mode.as_str()
        ),
        json!({ "data": rows, "next_cursor": page.get("next_cursor") }),
    )
}

async fn get_payment(ctx: &Ctx, args: &Value) -> TResult {
    let id = match opt_uuid(args, "payment_id")? {
        Some(id) => id,
        None => {
            let Some(order_id) = str_arg(args, "order_id") else {
                return fail("Give payment_id or order_id.");
            };
            let row: Option<(Uuid,)> = sqlx::query_as(
                "SELECT id FROM payments WHERE order_id = $1 AND ($2 OR mode = 'sandbox')
                 ORDER BY created_at DESC LIMIT 1",
            )
            .bind(order_id)
            .bind(ctx.grant.scopes.live)
            .fetch_optional(&ctx.state.pool)
            .await?;
            match row {
                Some((id,)) => id,
                None => return fail(format!("No payment with order_id {order_id}.")),
            }
        }
    };
    let Json(detail) = payments::detail(State(ctx.state.clone()), Path(id)).await?;
    let mode = detail.pointer("/payment/mode").and_then(Value::as_str);
    if mode == Some("live") && !ctx.grant.scopes.live {
        // do not reveal that a live payment exists
        return fail("Payment not found.");
    }
    let status = detail
        .pointer("/payment/status")
        .and_then(Value::as_str)
        .unwrap_or("?");
    out(format!("Payment {id} is {status}."), detail)
}

fn compact_delivery(d: &Value) -> Value {
    pick(
        d,
        &[
            "id",
            "payment_id",
            "product_id",
            "event_type",
            "status",
            "attempts",
            "next_attempt_at",
            "last_status_code",
            "last_error",
            "created_at",
            "delivered_at",
        ],
    )
}

async fn list_deliveries(ctx: &Ctx, mut args: Value) -> TResult {
    args["limit"] = json!(limit_of(&args));
    let query: DeliveriesQuery = parse(args)?;
    let Json(v) = payments::deliveries(State(ctx.state.clone()), ApiQuery(query)).await?;
    let rows: Vec<Value> = array(&v, "data")
        .iter()
        .filter(|d| {
            ctx.grant.scopes.live
                || d.pointer("/payload/livemode").and_then(Value::as_bool) != Some(true)
        })
        .map(compact_delivery)
        .collect();
    out(
        format!("{} delivery row(s), newest first.", rows.len()),
        json!({ "data": rows, "next_cursor": v.get("next_cursor") }),
    )
}

async fn list_events(ctx: &Ctx, mut args: Value) -> TResult {
    let include_body = args
        .get("include_body")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    args["limit"] = json!(limit_of(&args));
    let query: EventsQuery = parse(args)?;
    let Json(v) = payments::events(State(ctx.state.clone()), ApiQuery(query)).await?;
    let Json(accounts_json) = providers_admin::list(State(ctx.state.clone())).await?;
    let sandbox_accounts: Vec<String> = array(&accounts_json, "data")
        .iter()
        .filter(|a| a.get("mode").and_then(Value::as_str) == Some("sandbox"))
        .filter_map(|a| a.get("id").and_then(Value::as_str).map(str::to_string))
        .collect();
    let keys: &[&str] = if include_body {
        &[
            "id",
            "provider",
            "provider_account_id",
            "payment_id",
            "verified",
            "outcome",
            "error",
            "received_at",
            "headers",
            "body",
        ]
    } else {
        &[
            "id",
            "provider",
            "provider_account_id",
            "payment_id",
            "verified",
            "outcome",
            "error",
            "received_at",
        ]
    };
    let rows: Vec<Value> = array(&v, "data")
        .iter()
        .filter(|e| {
            ctx.grant.scopes.live
                || e.get("provider_account_id")
                    .and_then(Value::as_str)
                    .is_some_and(|id| sandbox_accounts.iter().any(|a| a == id))
        })
        .map(|e| pick(e, keys))
        .collect();
    out(
        format!("{} inbound event(s), newest first.", rows.len()),
        json!({ "data": rows, "next_cursor": v.get("next_cursor") }),
    )
}

async fn stats_summary(ctx: &Ctx, mut args: Value) -> TResult {
    let mode = mode_of(ctx, &args)?;
    args["mode"] = json!(mode.as_str());
    let query: stats::StatsQuery = parse(args)?;
    let Json(v) = stats::summary(State(ctx.state.clone()), ApiQuery(query)).await?;
    out(format!("Summary for {} mode.", mode.as_str()), v)
}

fn get_docs(ctx: &Ctx, args: &Value) -> TResult {
    match str_arg(args, "topic") {
        None => {
            let list: Vec<Value> = content::TOPICS
                .iter()
                .map(|t| json!({ "topic": t.id, "title": t.title, "summary": t.summary, "uri": content::uri(t.id) }))
                .collect();
            let text = content::TOPICS
                .iter()
                .map(|t| format!("- {}: {}", t.id, t.summary))
                .collect::<Vec<_>>()
                .join("\n");
            Ok(ToolOutput {
                text: format!("Topics (call get_docs with topic):\n{text}"),
                data: json!({ "topics": list }),
            })
        }
        Some(id) => match content::topic(id) {
            Some(topic) => {
                let body = topic.render(&ctx.state);
                Ok(ToolOutput {
                    data: json!({ "topic": topic.id, "title": topic.title, "markdown": body }),
                    text: body,
                })
            }
            None => fail(format!(
                "Unknown topic \"{id}\". Topics: {}.",
                content::topic_ids().join(", ")
            )),
        },
    }
}

fn generate_code(ctx: &Ctx, args: &Value) -> TResult {
    let task = str_arg(args, "task").ok_or_else(|| ToolError("task is required".into()))?;
    let language =
        str_arg(args, "language").ok_or_else(|| ToolError("language is required".into()))?;
    let Some(code) = content::snippet(task, language, &ctx.state.config.public_base_url) else {
        return fail(format!(
            "No snippet for task \"{task}\" in \"{language}\". Tasks: {}. Languages: {}.",
            content::TASKS.join(", "),
            content::LANGUAGES.join(", ")
        ));
    };
    let note = match task {
        "create_payment" => {
            "Needs PAYROUTERS_API_KEY (a prk_test_ key for sandbox) in the environment."
        }
        "verify_webhook" => {
            "Needs PAYROUTERS_WEBHOOK_SECRET (the product's whsec_ secret). Verify the raw body, not parsed JSON."
        }
        _ => "Needs PAYROUTERS_API_KEY in the environment.",
    };
    Ok(ToolOutput {
        text: format!("{note}\n\n```{language}\n{code}\n```"),
        data: json!({ "task": task, "language": language, "code": code, "note": note }),
    })
}

// --- Write tools --------------------------------------------------------------------------------

const SHOWN_ONCE: &str =
    "Shown once: tell the user to store it now in their backend's environment.";

async fn create_product(ctx: &Ctx, args: Value) -> TResult {
    let body: CreateProduct = parse(args)?;
    let (_, Json(v)) = products::create(State(ctx.state.clone()), ApiJson(body)).await?;
    let name = v
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("product")
        .to_string();
    out(
        format!(
            "Created product \"{name}\". Its webhook secret (webhook_secret) is in the result. {SHOWN_ONCE} Next: create_api_key for a sandbox key."
        ),
        v,
    )
}

async fn update_product(ctx: &Ctx, args: Value) -> TResult {
    let id = uuid_arg(&args, "product_id")?;
    ensure_product_writable(ctx, id).await?;
    let mut body = args;
    if let Some(o) = body.as_object_mut() {
        o.remove("product_id");
    }
    let body: UpdateProduct = parse(body)?;
    let Json(v) = products::update(State(ctx.state.clone()), Path(id), ApiJson(body)).await?;
    let mut v = to_value(&v)?;
    scrub_product(ctx, &mut v);
    out("Product updated.", v)
}

async fn rotate_secret(ctx: &Ctx, args: &Value) -> TResult {
    let id = uuid_arg(args, "product_id")?;
    ensure_product_writable(ctx, id).await?;
    let Json(v) = products::rotate_secret(State(ctx.state.clone()), Path(id)).await?;
    out(
        format!(
            "The old webhook secret no longer verifies. The new one is webhook_secret. {SHOWN_ONCE} Update the backend before the next delivery."
        ),
        v,
    )
}

async fn create_api_key(ctx: &Ctx, args: &Value) -> TResult {
    let id = uuid_arg(args, "product_id")?;
    let mode = mode_of(ctx, args)?;
    let name = str_arg(args, "name").ok_or_else(|| ToolError("name is required".into()))?;
    let body: CreateKey = parse(json!({ "mode": mode.as_str(), "name": name }))?;
    let (_, Json(v)) =
        products::create_key(State(ctx.state.clone()), Path(id), ApiJson(body)).await?;
    out(
        format!(
            "Created a {} API key. The full key is in the result (key). {SHOWN_ONCE} Variable name: PAYROUTERS_API_KEY.",
            mode.as_str()
        ),
        v,
    )
}

async fn revoke_api_key(ctx: &Ctx, args: &Value) -> TResult {
    let product_id = uuid_arg(args, "product_id")?;
    let key_id = uuid_arg(args, "key_id")?;
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT mode FROM api_keys WHERE id = $1 AND product_id = $2 AND revoked_at IS NULL",
    )
    .bind(key_id)
    .bind(product_id)
    .fetch_optional(&ctx.state.pool)
    .await?;
    let Some((mode,)) = row else {
        return fail("API key not found, or already revoked.");
    };
    if mode == "live" && !ctx.grant.scopes.live {
        return fail(LIVE_HINT);
    }
    products::revoke_key(State(ctx.state.clone()), Path((product_id, key_id))).await?;
    out(
        "API key revoked. Requests that use it now get 401.",
        json!({ "product_id": product_id, "key_id": key_id, "revoked": true }),
    )
}

async fn create_test_payment(ctx: &Ctx, args: Value) -> TResult {
    let mode = mode_of(ctx, &args)?;
    let mut body = args;
    body["mode"] = json!(mode.as_str());
    if str_arg(&body, "order_id").is_none() {
        body["order_id"] = json!(format!("MCP-{}", random_hex::<4>().to_uppercase()));
    }
    let create: AdminCreate = parse(body)?;
    let (_, Json(payment)) = payments::create(State(ctx.state.clone()), ApiJson(create)).await?;
    let payment = to_value(&payment)?;
    let hint = if payment.get("provider").and_then(Value::as_str) == Some("mock") {
        "Provider mock: open snap_url to see the customer page, or call simulate_payment to settle it."
    } else {
        "Open snap_url to see the customer page; the provider settles it."
    };
    out(
        format!("Created payment in {} mode. {hint}", mode.as_str()),
        payment,
    )
}

async fn payment_mode(ctx: &Ctx, id: Uuid) -> Result<Mode, ToolError> {
    let row: Option<(String,)> = sqlx::query_as("SELECT mode FROM payments WHERE id = $1")
        .bind(id)
        .fetch_optional(&ctx.state.pool)
        .await?;
    let Some((mode,)) = row else {
        return fail("Payment not found.");
    };
    if mode == "live" && !ctx.grant.scopes.live {
        // do not reveal that a live payment exists
        return fail("Payment not found.");
    }
    Ok(if mode == "live" {
        Mode::Live
    } else {
        Mode::Sandbox
    })
}

async fn simulate_payment(ctx: &Ctx, args: &Value) -> TResult {
    let id = uuid_arg(args, "payment_id")?;
    payment_mode(ctx, id).await?;
    let outcome: Outcome = match str_arg(args, "outcome") {
        Some(raw) => raw
            .parse()
            .map_err(|_| ToolError("outcome must be paid, expired or failed".into()))?,
        None => return fail("outcome is required (paid, expired or failed)."),
    };
    let Json(v) = payments::simulate(
        State(ctx.state.clone()),
        Path(id),
        ApiJson(parse(json!({ "outcome": outcome.as_str() }))?),
    )
    .await?;
    let v = to_value(&v)?;
    out(
        format!(
            "Payment is now {}.",
            v.get("status").and_then(Value::as_str).unwrap_or("?")
        ),
        v,
    )
}

async fn sync_payment(ctx: &Ctx, args: &Value) -> TResult {
    let id = uuid_arg(args, "payment_id")?;
    payment_mode(ctx, id).await?;
    let Json(v) = payments::sync(State(ctx.state.clone()), Path(id)).await?;
    let v = to_value(&v)?;
    out(
        format!(
            "Payment is {} after re-reading the provider.",
            v.get("status").and_then(Value::as_str).unwrap_or("?")
        ),
        v,
    )
}

async fn retry_delivery(ctx: &Ctx, args: &Value) -> TResult {
    let id = uuid_arg(args, "delivery_id")?;
    if !ctx.grant.scopes.live {
        let row: Option<(Value,)> =
            sqlx::query_as("SELECT payload FROM webhook_deliveries WHERE id = $1")
                .bind(id)
                .fetch_optional(&ctx.state.pool)
                .await?;
        if row.is_some_and(|(p,)| p.get("livemode").and_then(Value::as_bool) == Some(true)) {
            return fail("Delivery not found, or already delivered.");
        }
    }
    let Json(row) = payments::retry_delivery(State(ctx.state.clone()), Path(id)).await?;
    out(
        "Queued for another attempt now. Check it with list_webhook_deliveries in a few seconds.",
        compact_delivery(&to_value(&row)?),
    )
}

async fn test_webhook(ctx: &Ctx, args: &Value) -> TResult {
    let product_id = uuid_arg(args, "product_id")?;
    let st = &ctx.state;
    let row: Option<(String, Option<String>)> = sqlx::query_as(
        "SELECT name, webhook_url FROM products WHERE id = $1 AND archived_at IS NULL",
    )
    .bind(product_id)
    .fetch_optional(&st.pool)
    .await?;
    let Some((name, webhook_url)) = row else {
        return fail("Product not found.");
    };
    let Some(webhook_url) = webhook_url else {
        return fail(format!(
            "Product \"{name}\" has no webhook_url. Set one with update_product first."
        ));
    };

    let order_id = format!("MCP-TEST-{}", random_hex::<3>().to_uppercase());
    let create: AdminCreate = parse(json!({
        "product_id": product_id,
        "mode": "sandbox",
        "order_id": order_id,
        "amount": 10000,
        "provider": "mock",
        "method": "va_bca",
        "description": "PayRouters MCP webhook test"
    }))?;
    let (_, Json(created)) = payments::create(State(st.clone()), ApiJson(create)).await?;
    let payment_id = created.payment.id;
    let _ = payments::simulate(
        State(st.clone()),
        Path(payment_id),
        ApiJson(parse(json!({ "outcome": "paid" }))?),
    )
    .await?;
    // Deliver now instead of waiting for the background loop, then give a concurrent attempt a moment.
    let _ = outbound::run_once(st).await;
    let mut delivery: Option<DeliveryRow> = None;
    for _ in 0..24 {
        delivery = sqlx::query_as(
            "SELECT * FROM webhook_deliveries WHERE payment_id = $1 ORDER BY created_at DESC LIMIT 1",
        )
        .bind(payment_id)
        .fetch_optional(&st.pool)
        .await?;
        if delivery
            .as_ref()
            .is_some_and(|d| d.attempts > 0 || d.status != "pending")
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    let Some(d) = delivery else {
        return fail(
            "The payment was marked paid but no webhook delivery was queued. Check that the product has a webhook_url.",
        );
    };

    let verdict = match (
        d.status.as_str(),
        d.last_status_code,
        d.last_error.as_deref(),
    ) {
        ("delivered", Some(code), _) => format!(
            "Delivered: your endpoint answered {code}. The signed payment.paid event arrived. A 2xx does not prove the signature was checked: make sure the endpoint verifies PayRouters-Signature on the raw body (generate_code task verify_webhook) and ignores a repeated PayRouters-Event-Id."
        ),
        (_, Some(code), _) => format!(
            "Your endpoint answered HTTP {code}, which is not 2xx, so PayRouters will retry. Fix the endpoint (a 401 or 400 often means the signature check rejected the request or the body was parsed before verifying), then call retry_webhook_delivery."
        ),
        (_, None, Some(error)) => format!(
            "PayRouters could not reach {webhook_url}: {error}. The URL must be public and answer within 10 seconds; localhost is not reachable from this server (use a tunnel such as cloudflared or ngrok while developing). Redirects are not followed."
        ),
        _ => "The delivery is still queued; call list_webhook_deliveries in a few seconds."
            .to_string(),
    };
    out(
        verdict.clone(),
        json!({
            "product": name,
            "webhook_url": webhook_url,
            "payment_id": payment_id,
            "order_id": order_id,
            "delivery": compact_delivery(&to_value(&d)?),
            "verdict": verdict,
        }),
    )
}
