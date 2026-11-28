//! PayPal (overseas cash-in): Orders v2 with intent `CAPTURE`, approved by the payer on PayPal's
//! own page (`payer-action` link), then captured by PayRouters.
//!
//! PayPal cannot charge rupiah. PayRouters amounts stay integer IDR, and the adapter converts them
//! to USD, the only currency it charges in. The rate is the ECB reference rate (see `fx`), unless
//! the account sets a fixed `idr_rate`. The customer's total is rounded up to the next cent. The
//! order carries `pr:{payment id}:{IDR amount}:{currency}:{value}` as its
//! `custom_id`, so whatever PayPal later reports (the order, its capture, a webhook) is checked
//! against what this payment asked for and is mapped back to IDR with the rate that was in force at
//! checkout. The fee PayPal reports is converted the same way.
//!
//! The payer's approval does not move money: the order must be captured. That happens when the
//! payer returns (`/snap/api/{token}/return`), on `CHECKOUT.ORDER.APPROVED`, or when the worker
//! polls, whichever comes first; the capture call is idempotent. Webhooks are verified with
//! PayPal's verify-webhook-signature call and then only used to learn which order to re-read:
//! every state change comes from `GET /v2/checkout/orders/{id}`.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use axum::http::HeaderMap;
use chrono::{DateTime, Utc};
use payrouters_types::{Instructions, Mode, PaymentStatus, Provider};
use serde_json::{Value, json};
use uuid::Uuid;

use super::*;
use crate::util::sha256_hex;

pub struct Paypal;

fn base(mode: Mode) -> &'static str {
    match mode {
        Mode::Sandbox => "https://api-m.sandbox.paypal.com",
        Mode::Live => "https://api-m.paypal.com",
    }
}

/// The only currency charged: the rupiah is converted to USD (two decimal places).
const CURRENCY: &str = "USD";
const EXPONENT: u32 = 2;

/// PayPal keeps an order the payer has not approved open for about three hours (longer needs
/// PayPal's account team), whatever deadline the payment asked for.
const APPROVAL_WINDOW: chrono::Duration = chrono::Duration::hours(3);

/// The fixed rate, if any, from the account credentials.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FxConfig {
    /// `idr_rate`: rupiah per US dollar. Absent means the ECB reference rate.
    fixed: Option<f64>,
}

/// Where a rate came from, kept with the payment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum RateSource {
    Fixed,
    Ecb(chrono::NaiveDate),
}

/// The currency and the rupiah price of one unit of it that a payment is charged at.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Fx {
    currency: String,
    exponent: u32,
    rate: f64,
    source: RateSource,
}

pub(crate) fn fx_config(creds: &Credentials) -> Result<FxConfig, String> {
    let fixed = match creds.get("idr_rate") {
        None => None,
        Some(raw) => {
            let rate: f64 = raw
                .trim()
                .replace(',', "")
                .parse()
                .map_err(|_| format!("idr_rate `{raw}` is not a number"))?;
            if !rate.is_finite() || rate <= 0.0 || rate > 1e9 {
                return Err(format!("idr_rate `{raw}` must be a positive number"));
            }
            Some(rate)
        }
    };
    Ok(FxConfig { fixed })
}

impl FxConfig {
    fn at(&self, rate: f64, source: RateSource) -> Fx {
        Fx {
            currency: CURRENCY.to_string(),
            exponent: EXPONENT,
            rate,
            source,
        }
    }

    /// The charge rate: the account's fixed one, else the current ECB reference rate.
    async fn resolve(&self, ctx: &ProviderCtx<'_>) -> Result<Fx, ProviderError> {
        if let Some(rate) = self.fixed {
            return Ok(self.at(rate, RateSource::Fixed));
        }
        let r = fx::idr_per_unit(ctx.http, CURRENCY).await?;
        Ok(self.at(r.idr_per_unit, RateSource::Ecb(r.as_of)))
    }
}

impl RateSource {
    fn name(&self) -> &'static str {
        match self {
            RateSource::Fixed => "fixed",
            RateSource::Ecb(_) => "ecb",
        }
    }

    fn date(&self) -> Option<String> {
        match self {
            RateSource::Fixed => None,
            RateSource::Ecb(d) => Some(d.to_string()),
        }
    }

    fn label(&self) -> String {
        match self {
            RateSource::Fixed => "fixed rate set on the account".to_string(),
            RateSource::Ecb(d) => format!("ECB reference rate of {d}"),
        }
    }
}
impl Fx {
    /// What the customer is charged for `amount` rupiah, as shown before and after checkout.
    fn quote(&self, amount: i64) -> Quote {
        Quote {
            currency: self.currency.clone(),
            value: self.value(self.minor(amount)),
            idr_per_unit: self.rate,
            source: self.source.name().to_string(),
            as_of: self.source.date(),
        }
    }

    /// Minor units of the charge currency for `amount` rupiah, rounded up so the merchant never
    /// receives less than it asked for.
    fn minor(&self, amount: i64) -> i64 {
        let x = amount as f64 * 10f64.powi(self.exponent as i32) / self.rate;
        let nearest = x.round();
        // 162505 IDR at 16250.5 is exactly 10.00; float noise must not turn that into 10.01.
        if (x - nearest).abs() < 1e-6 {
            nearest as i64
        } else {
            x.ceil() as i64
        }
        .max(1)
    }

    fn value(&self, minor: i64) -> String {
        format_minor(minor, self.exponent)
    }
}

fn format_minor(minor: i64, exponent: u32) -> String {
    if exponent == 0 {
        return minor.to_string();
    }
    let unit = 10i64.pow(exponent);
    format!(
        "{}.{:0width$}",
        minor / unit,
        minor % unit,
        width = exponent as usize
    )
}

/// `"25.44"` → 2544. Refuses more decimals than the currency has and anything that is not a plain
/// decimal.
fn parse_minor(value: &str, exponent: u32) -> Option<i64> {
    let (int, frac) = value.split_once('.').unwrap_or((value, ""));
    let digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    if int.is_empty() || !digits(int) || !digits(frac) || frac.len() > exponent as usize {
        return None;
    }
    let padded = format!("{frac:0<width$}", width = exponent as usize);
    format!("{int}{padded}").parse().ok()
}

/// What the order's `custom_id` says this payment asked for.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Ours {
    payment_id: Uuid,
    amount: i64,
    currency: String,
    value: String,
}

fn custom_id(input: &ChargeInput<'_>, fx: &Fx, value: &str) -> String {
    format!(
        "pr:{}:{}:{}:{value}",
        input.payment_id, input.amount, fx.currency
    )
}

fn parse_custom_id(s: &str) -> Option<Ours> {
    let mut parts = s.strip_prefix("pr:")?.split(':');
    let ours = Ours {
        payment_id: Uuid::parse_str(parts.next()?).ok()?,
        amount: parts.next()?.parse().ok()?,
        currency: parts.next()?.to_string(),
        value: parts.next()?.to_string(),
    };
    (parts.next().is_none() && ours.currency == CURRENCY).then_some(ours)
}

pub(crate) fn order_body(input: &ChargeInput<'_>, fx: &Fx) -> Value {
    let value = fx.value(fx.minor(input.amount));
    let description: String = input
        .description
        .map(str::trim)
        .filter(|d| !d.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("Order {}", input.order_id))
        .chars()
        .take(127)
        .collect();
    let mut paypal = json!({
        "experience_context": {
            "return_url": format!("{}/return", input.snap_api_url),
            "cancel_url": format!("{}/cancel", input.snap_api_url),
            "user_action": "PAY_NOW",
            "shipping_preference": "NO_SHIPPING",
        }
    });
    if let Some(email) = input.customer.and_then(|c| c.email.as_deref()) {
        paypal["email_address"] = json!(email);
    }
    json!({
        "intent": "CAPTURE",
        "purchase_units": [{
            "custom_id": custom_id(input, fx, &value),
            "description": description,
            "amount": { "currency_code": fx.currency, "value": value },
        }],
        "payment_source": { "paypal": paypal },
    })
}

// ---------------------------------------------------------------------------------------------
// HTTP: OAuth token cache and the authenticated call

struct CachedToken {
    fingerprint: String,
    token: String,
    expires: Instant,
}

static TOKENS: LazyLock<Mutex<HashMap<Uuid, CachedToken>>> = LazyLock::new(Default::default);

fn error_detail(reply: &HttpReply) -> Option<String> {
    let json = &reply.json;
    let message = str_field(json, "message").or_else(|| str_field(json, "error_description"))?;
    let issue = json
        .get("details")
        .and_then(Value::as_array)
        .and_then(|d| d.first())
        .and_then(|d| str_field(d, "issue"));
    Some(match issue {
        Some(issue) => format!("{issue}: {message}"),
        None => message.to_string(),
    })
}

fn api_error(reply: &HttpReply) -> ProviderError {
    fail_by_status("PayPal", reply, error_detail(reply))
}

/// The machine-readable `details[0].issue` of a PayPal error.
fn issue(reply: &HttpReply) -> Option<&str> {
    reply
        .json
        .get("details")
        .and_then(Value::as_array)
        .and_then(|d| d.first())
        .and_then(|d| str_field(d, "issue"))
}

async fn access_token(ctx: &ProviderCtx<'_>, refresh: bool) -> Result<String, ProviderError> {
    let id = ctx.creds.require("client_id")?;
    let secret = ctx.creds.require("client_secret")?;
    let fingerprint = sha256_hex(format!("{}:{id}:{secret}", ctx.mode));
    if !refresh
        && let Ok(cache) = TOKENS.lock()
        && let Some(c) = cache.get(&ctx.account_id)
        && c.fingerprint == fingerprint
        && c.expires > Instant::now()
    {
        return Ok(c.token.clone());
    }
    let reply = send(
        ctx.http
            .post(format!("{}/v1/oauth2/token", base(ctx.mode)))
            .basic_auth(id, Some(secret))
            .header("Accept", "application/json")
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body("grant_type=client_credentials"),
    )
    .await?;
    if reply.status == 401 {
        return Err(ProviderError::Rejected(
            "PayPal rejected the client id and secret (sandbox credentials only work on a sandbox account)".into(),
        ));
    }
    if !(200..300).contains(&reply.status) {
        return Err(api_error(&reply));
    }
    let token = str_field(&reply.json, "access_token")
        .ok_or_else(|| ProviderError::Unavailable("PayPal sent no access token".into()))?
        .to_string();
    let ttl = reply
        .json
        .get("expires_in")
        .and_then(Value::as_u64)
        .unwrap_or(300);
    if let Ok(mut cache) = TOKENS.lock() {
        cache.insert(
            ctx.account_id,
            CachedToken {
                fingerprint,
                token: token.clone(),
                expires: Instant::now() + Duration::from_secs(ttl.saturating_sub(60).max(30)),
            },
        );
    }
    Ok(token)
}

enum Verb {
    Get,
    Post,
}

/// An authenticated call. A 401 drops the cached token and retries once with a fresh one.
async fn call(
    ctx: &ProviderCtx<'_>,
    verb: Verb,
    path: &str,
    body: Option<String>,
    request_id: Option<&str>,
) -> Result<HttpReply, ProviderError> {
    let url = format!("{}{path}", base(ctx.mode));
    let mut reply = None;
    for refresh in [false, true] {
        let token = access_token(ctx, refresh).await?;
        let mut req = match verb {
            Verb::Get => ctx.http.get(&url),
            Verb::Post => ctx.http.post(&url),
        }
        .bearer_auth(token)
        .header("Accept", "application/json");
        if let Some(id) = request_id {
            req = req.header("PayPal-Request-Id", id);
        }
        if let Some(b) = &body {
            req = req
                .header("Content-Type", "application/json")
                .body(b.clone());
        }
        let r = send(req).await?;
        let unauthorized = r.status == 401;
        reply = Some(r);
        if !unauthorized {
            break;
        }
    }
    Ok(reply.expect("loop ran at least once"))
}

// ---------------------------------------------------------------------------------------------
// Orders

/// PayPal order ids are short upper-case alphanumerics. Anything else never reaches a URL path.
fn valid_order_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

pub(crate) enum OrderState {
    /// The payer approved; nothing is collected until the order is captured.
    NeedsCapture(Ours),
    Update(StatusUpdate),
}

fn unit(order: &Value) -> Option<&Value> {
    order.get("purchase_units")?.as_array()?.first()
}

fn captures(order: &Value) -> Vec<&Value> {
    unit(order)
        .and_then(|u| u.get("payments"))
        .and_then(|p| p.get("captures"))
        .and_then(Value::as_array)
        .map(|c| c.iter().collect())
        .unwrap_or_default()
}

fn money(v: &Value, key: &str) -> Option<(String, String)> {
    let m = v.get(key)?;
    Some((
        str_field(m, "currency_code")?.to_string(),
        str_field(m, "value")?.to_string(),
    ))
}

fn parse_time(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|t| t.with_timezone(&Utc))
}

fn capture_status(capture: &Value) -> &str {
    str_field(capture, "status").unwrap_or("")
}

/// Maps an order as PayPal reports it to what PayRouters stores. `None` means the order is not one
/// of ours (no or foreign `custom_id`).
pub(crate) fn order_state(order: &Value) -> Option<OrderState> {
    let ours = parse_custom_id(str_field(unit(order)?, "custom_id")?)?;
    let order_id = str_field(order, "id");
    let status = |s: PaymentStatus| {
        let mut u = StatusUpdate::new(s);
        u.provider_ref = order_id.map(str::to_string);
        OrderState::Update(u)
    };
    match str_field(order, "status").unwrap_or("") {
        "APPROVED" => Some(OrderState::NeedsCapture(ours)),
        "VOIDED" => Some(status(PaymentStatus::Cancelled)),
        "COMPLETED" => {
            let all = captures(order);
            let collected = all.iter().find(|c| {
                matches!(
                    capture_status(c),
                    "COMPLETED" | "PARTIALLY_REFUNDED" | "REFUNDED"
                )
            });
            if let Some(capture) = collected {
                return Some(paid_state(order_id, &ours, capture));
            }
            let pending = all.iter().any(|c| capture_status(c) == "PENDING");
            let failed = all
                .iter()
                .any(|c| matches!(capture_status(c), "DECLINED" | "FAILED"));
            Some(status(if failed && !pending {
                PaymentStatus::Failed
            } else {
                PaymentStatus::Pending
            }))
        }
        // CREATED, PAYER_ACTION_REQUIRED, SAVED
        _ => Some(status(PaymentStatus::Pending)),
    }
}

fn paid_state(order_id: Option<&str>, ours: &Ours, capture: &Value) -> OrderState {
    let mut u = StatusUpdate::new(PaymentStatus::Paid);
    u.provider_ref = order_id.map(str::to_string);
    u.paid_at = str_field(capture, "update_time")
        .or_else(|| str_field(capture, "create_time"))
        .and_then(parse_time);
    let exp = EXPONENT;
    let asked = parse_minor(&ours.value, exp);
    let exact = asked.is_some()
        && money(capture, "amount").is_some_and(|(currency, value)| {
            currency == ours.currency && parse_minor(&value, exp) == asked
        });
    // What was captured is not what this payment asked for: a value no stored amount can equal,
    // so the amount check rejects the payment instead of crediting it.
    u.amount = Some(if exact { ours.amount } else { -1 });
    if exact
        && let (Some(asked), Some((currency, fee))) = (
            asked,
            capture
                .get("seller_receivable_breakdown")
                .and_then(|b| money(b, "paypal_fee")),
        )
        && currency == ours.currency
        && let Some(fee) = parse_minor(&fee, exp)
        && asked > 0
    {
        // The fee is in the charge currency; the ledger is in rupiah, at the checkout rate.
        let idr =
            (i128::from(fee) * i128::from(ours.amount) + i128::from(asked) / 2) / i128::from(asked);
        u.fee_amount = i64::try_from(idr).ok();
    }
    OrderState::Update(u)
}

async fn get_order(ctx: &ProviderCtx<'_>, id: &str) -> Result<Option<Value>, ProviderError> {
    let reply = call(
        ctx,
        Verb::Get,
        &format!("/v2/checkout/orders/{id}"),
        None,
        None,
    )
    .await?;
    if reply.status == 404 {
        return Ok(None);
    }
    if !(200..300).contains(&reply.status) {
        return Err(api_error(&reply));
    }
    Ok(Some(reply.json))
}

/// Re-read an order and, when the payer has approved it, capture it. `Ok(None)`: no such order, or
/// not one of ours.
async fn sync_order(
    ctx: &ProviderCtx<'_>,
    id: &str,
) -> Result<Option<StatusUpdate>, ProviderError> {
    if !valid_order_id(id) {
        return Ok(None);
    }
    let Some(order) = get_order(ctx, id).await? else {
        return Ok(None);
    };
    let ours = match order_state(&order) {
        None => return Ok(None),
        Some(OrderState::Update(u)) => return Ok(Some(u)),
        Some(OrderState::NeedsCapture(ours)) => ours,
    };
    let reply = call(
        ctx,
        Verb::Post,
        &format!("/v2/checkout/orders/{id}/capture"),
        Some("{}".to_string()),
        Some(&format!("capture-{}", ours.payment_id)),
    )
    .await?;
    let settled = if (200..300).contains(&reply.status) {
        Some(reply.json)
    } else if reply.status == 422 {
        match issue(&reply) {
            // Someone else captured first (the return page, a webhook): read the result.
            Some("ORDER_ALREADY_CAPTURED") => get_order(ctx, id).await?,
            // The payer's funding source was refused; the order stays open for another try.
            Some("INSTRUMENT_DECLINED" | "ORDER_NOT_APPROVED") => None,
            _ => return Err(api_error(&reply)),
        }
    } else {
        return Err(api_error(&reply));
    };
    Ok(Some(match settled.as_ref().and_then(order_state) {
        Some(OrderState::Update(u)) => u,
        _ => {
            let mut u = StatusUpdate::new(PaymentStatus::Pending);
            u.provider_ref = Some(id.to_string());
            u
        }
    }))
}

// ---------------------------------------------------------------------------------------------
// Webhooks

/// The order a verified webhook is about. Refunds, reversals and everything else are no cash-in
/// state change.
pub(crate) fn webhook_order_id(v: &Value) -> Option<String> {
    let resource = v.get("resource")?;
    match str_field(v, "event_type")? {
        "CHECKOUT.ORDER.APPROVED" | "CHECKOUT.ORDER.COMPLETED" => {
            str_field(resource, "id").map(str::to_string)
        }
        "PAYMENT.CAPTURE.COMPLETED"
        | "PAYMENT.CAPTURE.PENDING"
        | "PAYMENT.CAPTURE.DECLINED"
        | "PAYMENT.CAPTURE.DENIED" => resource
            .get("supplementary_data")?
            .get("related_ids")?
            .get("order_id")
            .and_then(Value::as_str)
            .map(str::to_string),
        _ => None,
    }
}

struct Transmission<'a> {
    auth_algo: &'a str,
    cert_url: &'a str,
    id: &'a str,
    sig: &'a str,
    time: &'a str,
}

/// The `verify-webhook-signature` body. PayPal checks the event byte for byte, so the received
/// text is spliced in as it came, never parsed and written back.
fn verify_payload(t: &Transmission<'_>, webhook_id: &str, raw_event: &str) -> String {
    let s = |v: &str| Value::String(v.to_string()).to_string();
    format!(
        r#"{{"auth_algo":{},"cert_url":{},"transmission_id":{},"transmission_sig":{},"transmission_time":{},"webhook_id":{},"webhook_event":{}}}"#,
        s(t.auth_algo),
        s(t.cert_url),
        s(t.id),
        s(t.sig),
        s(t.time),
        s(webhook_id),
        raw_event.trim()
    )
}

async fn verify_webhook(
    ctx: &ProviderCtx<'_>,
    headers: &HeaderMap,
    body: &[u8],
) -> Result<(), WebhookError> {
    let webhook_id = ctx.creds.get("webhook_id").ok_or_else(|| {
        WebhookError::Unauthorized(
            "webhook_id is not set on the PayPal account, so the event cannot be verified".into(),
        )
    })?;
    let header = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .filter(|v| !v.is_empty())
            .ok_or_else(|| WebhookError::Unauthorized(format!("missing {name} header")))
    };
    let t = Transmission {
        auth_algo: header("paypal-auth-algo")?,
        cert_url: header("paypal-cert-url")?,
        id: header("paypal-transmission-id")?,
        sig: header("paypal-transmission-sig")?,
        time: header("paypal-transmission-time")?,
    };
    let raw = std::str::from_utf8(body)
        .map_err(|_| WebhookError::Malformed("body is not UTF-8".into()))?;
    let reply = call(
        ctx,
        Verb::Post,
        "/v1/notifications/verify-webhook-signature",
        Some(verify_payload(&t, webhook_id, raw)),
        None,
    )
    .await
    .map_err(|e| WebhookError::Upstream(format!("could not verify with PayPal: {e}")))?;
    match reply.status {
        200..=299 if str_field(&reply.json, "verification_status") == Some("SUCCESS") => Ok(()),
        200..=299 => Err(WebhookError::Unauthorized(
            "PayPal says the webhook signature does not verify".into(),
        )),
        // our credentials or PayPal's availability, not the sender
        401 | 403 | 429 | 500..=599 => Err(WebhookError::Upstream(format!(
            "could not verify with PayPal: {}",
            api_error(&reply)
        ))),
        _ => Err(WebhookError::Unauthorized(format!(
            "PayPal refused to verify the webhook: {}",
            api_error(&reply)
        ))),
    }
}

#[async_trait]
impl ProviderAdapter for Paypal {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            provider: Provider::Paypal,
            supports_sandbox: true,
            supports_provider_checkout: true,
            credential_fields: vec![
                CredentialField {
                    key: "client_id",
                    label: "Client ID",
                    secret: false,
                    required: true,
                    placeholder: "PayPal Developer → Apps & Credentials",
                },
                CredentialField {
                    key: "client_secret",
                    label: "Client secret",
                    secret: true,
                    required: true,
                    placeholder: "shown next to the client ID",
                },
                CredentialField {
                    key: "idr_rate",
                    label: "Fixed rupiah per 1 USD (optional)",
                    secret: false,
                    required: false,
                    placeholder: "blank = ECB reference rate",
                },
                CredentialField {
                    key: "webhook_id",
                    label: "Webhook ID",
                    secret: false,
                    required: false,
                    placeholder: "PayPal app → Webhooks (after adding the URL)",
                },
            ],
            methods: vec![],
        }
    }

    fn validate(
        &self,
        creds: &Credentials,
        _input: &ChargeInput<'_>,
        _hosted: bool,
    ) -> Result<(), String> {
        // The rate itself is looked up when the order is created; here only the account's setup
        // is checked, before anything is stored.
        fx_config(creds).map(|_| ())
    }

    async fn create_charge(
        &self,
        _ctx: &ProviderCtx<'_>,
        _input: &ChargeInput<'_>,
    ) -> Result<ChargeOutput, ProviderError> {
        Err(ProviderError::Rejected(
            "PayPal has no standard payment methods; use its hosted checkout".into(),
        ))
    }

    async fn create_hosted(
        &self,
        ctx: &ProviderCtx<'_>,
        input: &ChargeInput<'_>,
    ) -> Result<ChargeOutput, ProviderError> {
        let cfg = fx_config(ctx.creds).map_err(ProviderError::Config)?;
        let fx = cfg.resolve(ctx).await?;
        let body = order_body(input, &fx);
        let reply = call(
            ctx,
            Verb::Post,
            "/v2/checkout/orders",
            Some(body.to_string()),
            Some(&input.payment_id.to_string()),
        )
        .await?;
        if !(200..300).contains(&reply.status) {
            return Err(api_error(&reply));
        }
        let order = &reply.json;
        let id = str_field(order, "id")
            .ok_or_else(|| ProviderError::Unavailable("PayPal order lacked an id".into()))?;
        let url = order
            .get("links")
            .and_then(Value::as_array)
            .and_then(|links| {
                links
                    .iter()
                    .find(|l| matches!(str_field(l, "rel"), Some("payer-action" | "approve")))
            })
            .and_then(|l| str_field(l, "href"))
            .ok_or_else(|| {
                ProviderError::Unavailable("PayPal order lacked an approval link".into())
            })?;
        let charged = &body["purchase_units"][0]["amount"];
        Ok(ChargeOutput {
            instructions: Instructions::Redirect {
                url: url.to_string(),
            },
            provider_ref: Some(id.to_string()),
            provider_data: json!({
                "kind": "order",
                "charge": { "currency": charged["currency_code"], "value": charged["value"] },
                "idr_rate": fx.rate,
                "rate_source": fx.source.name(),
                "rate_date": fx.source.date(),
            }),
            payable_amount: None,
            expires_at: enforced_expiry(input.expires_at, None, Some(APPROVAL_WINDOW)),
        })
    }

    async fn fetch_status(
        &self,
        ctx: &ProviderCtx<'_>,
        payment: &PaymentRef<'_>,
    ) -> Result<Option<StatusUpdate>, ProviderError> {
        match payment.provider_ref {
            Some(id) => sync_order(ctx, id).await,
            None => Ok(None),
        }
    }

    async fn parse_webhook(
        &self,
        ctx: &ProviderCtx<'_>,
        headers: &HeaderMap,
        body: &[u8],
    ) -> Result<ParsedWebhook, WebhookError> {
        let v: Value = serde_json::from_slice(body)
            .map_err(|e| WebhookError::Malformed(format!("body is not JSON: {e}")))?;
        verify_webhook(ctx, headers, body).await?;
        let ignore = ParsedWebhook {
            lookup: Lookup::Ignore,
            update: None,
        };
        let Some(order_id) = webhook_order_id(&v) else {
            return Ok(ignore);
        };
        // Authentic, but the order is read again: the event body is never the source of state.
        match sync_order(ctx, &order_id).await {
            Ok(Some(update)) => Ok(ParsedWebhook {
                lookup: Lookup::ProviderRef(order_id),
                update: Some(update),
            }),
            Ok(None) => Ok(ignore),
            Err(e) => Err(WebhookError::Upstream(format!(
                "could not read the order from PayPal: {e}"
            ))),
        }
    }

    async fn quote(
        &self,
        ctx: &ProviderCtx<'_>,
        amount: i64,
    ) -> Result<Option<Quote>, ProviderError> {
        let cfg = fx_config(ctx.creds).map_err(ProviderError::Config)?;
        Ok(Some(cfg.resolve(ctx).await?.quote(amount)))
    }

    async fn test_connection(&self, ctx: &ProviderCtx<'_>) -> Result<String, ProviderError> {
        let cfg = fx_config(ctx.creds).map_err(ProviderError::Config)?;
        access_token(ctx, true).await?;
        // A payment is refused when no rate can be found, so prove the rate source works too.
        let fx = cfg.resolve(ctx).await?;
        let webhook = if ctx.creds.get("webhook_id").is_some() {
            "webhook ID is set"
        } else {
            "no webhook ID yet: PayPal webhooks are refused until it is set, and payments settle when the payer returns or by polling"
        };
        Ok(format!(
            "Authenticated against PayPal; charging in {} at {:.2} IDR per unit ({}); {webhook}",
            fx.currency,
            fx.rate,
            fx.source.label()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use payrouters_types::Customer;
    use std::collections::BTreeMap;

    const PAYMENT: &str = "0b7e6f64-8f0e-4a63-9c43-0a1c1d2e3f40";

    fn creds(pairs: &[(&str, &str)]) -> Credentials {
        Credentials(
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect::<BTreeMap<_, _>>(),
        )
    }

    /// A charge at a fixed rate in USD.
    fn usd(rate: &str) -> Fx {
        fixed(&[("idr_rate", rate)])
    }

    fn fixed(pairs: &[(&str, &str)]) -> Fx {
        let cfg = fx_config(&creds(pairs)).unwrap();
        let rate = cfg.fixed.expect("a fixed rate");
        cfg.at(rate, RateSource::Fixed)
    }

    fn input<'a>(customer: Option<&'a Customer>, amount: i64) -> ChargeInput<'a> {
        ChargeInput {
            payment_id: Uuid::parse_str(PAYMENT).unwrap(),
            order_id: "INV-1",
            amount,
            expires_at: "2026-11-30T10:00:00Z".parse().unwrap(),
            description: Some("Consulting"),
            customer,
            items: &[],
            return_url: None,
            snap_url: "https://pay.example.com/snap/t",
            snap_api_url: "https://pay.example.com/snap/api/t",
            provider_options: &Value::Null,
            method: None,
        }
    }

    #[test]
    fn config_takes_an_optional_fixed_rate_and_always_charges_usd() {
        // no rate: the ECB reference rate is used
        let cfg = fx_config(&creds(&[])).unwrap();
        assert_eq!(cfg.fixed, None);
        assert!(fx_config(&creds(&[("idr_rate", "abc")])).is_err());
        assert!(fx_config(&creds(&[("idr_rate", "0")])).is_err());
        assert!(fx_config(&creds(&[("idr_rate", "-5")])).is_err());
        let ok = fx_config(&creds(&[("idr_rate", "16,250.5")])).unwrap();
        assert_eq!(ok.fixed, Some(16_250.5));
        // a `currency` left on an account from before USD was fixed changes nothing
        let old = fx_config(&creds(&[("idr_rate", "16000"), ("currency", "SGD")])).unwrap();
        assert_eq!(old.at(16_000.0, RateSource::Fixed).currency, "USD");
        assert_eq!(usd("16000").currency, "USD");
    }

    #[test]
    fn conversion_rounds_up_to_the_next_minor_unit() {
        let fx = usd("16000");
        assert_eq!(fx.value(fx.minor(160_000)), "10.00");
        // 100 000 IDR = 6.25 USD exactly; 100 001 needs one more cent
        assert_eq!(fx.value(fx.minor(100_000)), "6.25");
        assert_eq!(fx.value(fx.minor(100_001)), "6.26");
        // never below the smallest unit
        assert_eq!(fx.value(fx.minor(1)), "0.01");
        // float noise at an exact boundary does not add a cent
        assert_eq!(usd("16250.5").value(usd("16250.5").minor(162_505)), "10.00");
    }

    #[test]
    fn a_quote_is_what_the_order_will_charge() {
        let fx = usd("18378");
        let q = fx.quote(129_000);
        // 129 000 / 18 378 = 7.0192… USD, rounded up to the next cent
        assert_eq!(q.value, "7.02");
        assert_eq!(q.currency, "USD");
        assert_eq!(q.idr_per_unit, 18_378.0);
        assert_eq!((q.source.as_str(), q.as_of), ("fixed", None));
        let body = order_body(&input(None, 129_000), &fx);
        assert_eq!(body["purchase_units"][0]["amount"]["value"], q.value);
    }

    #[test]
    fn the_stored_charge_reads_back_as_the_quote_shown_to_the_customer() {
        let data = json!({
            "kind": "order",
            "charge": { "currency": "USD", "value": "7.02" },
            "idr_rate": 18378.0,
            "rate_source": "ecb",
            "rate_date": "2026-11-27",
        });
        let q = Quote::from_provider_data(&data).unwrap();
        assert_eq!((q.currency.as_str(), q.value.as_str()), ("USD", "7.02"));
        assert_eq!(q.idr_per_unit, 18_378.0);
        assert_eq!(
            (q.source.as_str(), q.as_of.as_deref()),
            ("ecb", Some("2026-11-27"))
        );
        // a payment on a provider that charges rupiah has nothing to show
        assert_eq!(Quote::from_provider_data(&json!({ "kind": "va" })), None);
    }

    #[test]
    fn minor_units_round_trip() {
        assert_eq!(parse_minor("25.44", 2), Some(2544));
        assert_eq!(parse_minor("25.4", 2), Some(2540));
        assert_eq!(parse_minor("25", 2), Some(2500));
        assert_eq!(parse_minor("0.01", 2), Some(1));
        assert_eq!(parse_minor("500", 0), Some(500));
        assert_eq!(parse_minor("25.444", 2), None);
        assert_eq!(parse_minor("500.5", 0), None);
        assert_eq!(parse_minor("-1.00", 2), None);
        assert_eq!(parse_minor("1e3", 2), None);
        assert_eq!(parse_minor("", 2), None);
        assert_eq!(format_minor(2544, 2), "25.44");
        assert_eq!(format_minor(5, 2), "0.05");
        assert_eq!(format_minor(100, 2), "1.00");
    }

    #[test]
    fn custom_id_round_trips_and_rejects_foreign_ids() {
        let i = input(None, 100_000);
        let fx = usd("16000");
        let id = custom_id(&i, &fx, "6.25");
        assert_eq!(
            parse_custom_id(&id),
            Some(Ours {
                payment_id: i.payment_id,
                amount: 100_000,
                currency: "USD".into(),
                value: "6.25".into()
            })
        );
        assert_eq!(parse_custom_id("custom_id_1001"), None);
        assert_eq!(parse_custom_id(&format!("pr:{PAYMENT}:1:USD")), None);
        assert_eq!(parse_custom_id(&format!("pr:{PAYMENT}:1:USD:1.00:x")), None);
        assert_eq!(parse_custom_id(&format!("pr:{PAYMENT}:1:IDR:1.00")), None);
        assert_eq!(parse_custom_id("pr:not-a-uuid:1:USD:1.00"), None);
    }

    #[test]
    fn order_request_sends_the_payer_back_through_payrouters() {
        let c = Customer {
            name: None,
            email: Some("buyer@example.com".into()),
            phone: None,
        };
        let b = order_body(&input(Some(&c), 100_000), &usd("16000"));
        assert_eq!(b["intent"], "CAPTURE");
        let unit = &b["purchase_units"][0];
        assert_eq!(unit["amount"]["currency_code"], "USD");
        assert_eq!(unit["amount"]["value"], "6.25");
        assert_eq!(unit["description"], "Consulting");
        assert_eq!(unit["custom_id"], format!("pr:{PAYMENT}:100000:USD:6.25"));
        let paypal = &b["payment_source"]["paypal"];
        assert_eq!(paypal["email_address"], "buyer@example.com");
        let ctx = &paypal["experience_context"];
        assert_eq!(
            ctx["return_url"],
            "https://pay.example.com/snap/api/t/return"
        );
        assert_eq!(
            ctx["cancel_url"],
            "https://pay.example.com/snap/api/t/cancel"
        );
        assert_eq!(ctx["user_action"], "PAY_NOW");
        assert_eq!(ctx["shipping_preference"], "NO_SHIPPING");
    }

    fn order(status: &str, captures: Value) -> Value {
        json!({
            "id": "5O190127TN364715T",
            "status": status,
            "purchase_units": [{
                "custom_id": format!("pr:{PAYMENT}:100000:USD:6.25"),
                "amount": { "currency_code": "USD", "value": "6.25" },
                "payments": { "captures": captures },
            }],
        })
    }

    fn capture(status: &str, value: &str, currency: &str) -> Value {
        json!({
            "id": "3C679366HH908993F",
            "status": status,
            "amount": { "currency_code": currency, "value": value },
            "seller_receivable_breakdown": {
                "gross_amount": { "currency_code": currency, "value": value },
                "paypal_fee": { "currency_code": currency, "value": "0.50" },
                "net_amount": { "currency_code": currency, "value": "5.75" },
            },
            "create_time": "2026-11-28T10:00:00Z",
            "update_time": "2026-11-28T10:00:05Z",
        })
    }

    fn update(state: Option<OrderState>) -> StatusUpdate {
        match state {
            Some(OrderState::Update(u)) => u,
            Some(OrderState::NeedsCapture(_)) => panic!("expected an update, got a capture"),
            None => panic!("expected an update, got none"),
        }
    }

    #[test]
    fn a_completed_capture_is_paid_in_rupiah_with_the_converted_fee() {
        let o = order("COMPLETED", json!([capture("COMPLETED", "6.25", "USD")]));
        let u = update(order_state(&o));
        assert_eq!(u.status, PaymentStatus::Paid);
        assert_eq!(u.amount, Some(100_000));
        // 0.50 of 6.25 is 8 % of 100 000 IDR
        assert_eq!(u.fee_amount, Some(8_000));
        assert_eq!(u.provider_ref.as_deref(), Some("5O190127TN364715T"));
        assert_eq!(
            u.paid_at,
            Some("2026-11-28T10:00:05Z".parse::<DateTime<Utc>>().unwrap())
        );
    }

    #[test]
    fn a_capture_for_another_amount_or_currency_fails_closed() {
        for (value, currency) in [("6.24", "USD"), ("6.25", "EUR"), ("6.250", "USD")] {
            let o = order("COMPLETED", json!([capture("COMPLETED", value, currency)]));
            let u = update(order_state(&o));
            assert_eq!(u.status, PaymentStatus::Paid);
            assert_eq!(u.amount, Some(-1), "{value} {currency}");
            assert_eq!(u.fee_amount, None);
        }
    }

    #[test]
    fn refunded_captures_were_still_collected() {
        for s in ["PARTIALLY_REFUNDED", "REFUNDED"] {
            let o = order("COMPLETED", json!([capture(s, "6.25", "USD")]));
            assert_eq!(update(order_state(&o)).status, PaymentStatus::Paid);
        }
    }

    #[test]
    fn pending_and_declined_captures() {
        let o = order("COMPLETED", json!([capture("PENDING", "6.25", "USD")]));
        assert_eq!(update(order_state(&o)).status, PaymentStatus::Pending);
        let o = order("COMPLETED", json!([capture("DECLINED", "6.25", "USD")]));
        assert_eq!(update(order_state(&o)).status, PaymentStatus::Failed);
        let o = order("COMPLETED", json!([]));
        assert_eq!(update(order_state(&o)).status, PaymentStatus::Pending);
    }

    #[test]
    fn order_statuses_before_and_after_the_payer() {
        for s in ["CREATED", "PAYER_ACTION_REQUIRED", "SAVED"] {
            assert_eq!(
                update(order_state(&order(s, json!([])))).status,
                PaymentStatus::Pending
            );
        }
        assert_eq!(
            update(order_state(&order("VOIDED", json!([])))).status,
            PaymentStatus::Cancelled
        );
        // approved means "capture me", and only for an order that is ours
        match order_state(&order("APPROVED", json!([]))) {
            Some(OrderState::NeedsCapture(ours)) => {
                assert_eq!(ours.payment_id, Uuid::parse_str(PAYMENT).unwrap())
            }
            _ => panic!("approved orders need a capture"),
        }
    }

    #[test]
    fn foreign_orders_are_not_ours_and_never_captured() {
        let mut o = order("APPROVED", json!([]));
        o["purchase_units"][0]["custom_id"] = json!("custom_id_1001");
        assert!(order_state(&o).is_none());
        o["purchase_units"][0] = json!({ "amount": { "currency_code": "USD", "value": "1.00" } });
        assert!(order_state(&o).is_none());
    }

    #[test]
    fn webhooks_name_the_order_to_read_again() {
        let approved =
            json!({ "event_type": "CHECKOUT.ORDER.APPROVED", "resource": { "id": "ORD1" } });
        assert_eq!(webhook_order_id(&approved).as_deref(), Some("ORD1"));
        let captured = json!({
            "event_type": "PAYMENT.CAPTURE.COMPLETED",
            "resource": { "id": "CAP1", "supplementary_data": { "related_ids": { "order_id": "ORD2" } } }
        });
        assert_eq!(webhook_order_id(&captured).as_deref(), Some("ORD2"));
        let refunded = json!({
            "event_type": "PAYMENT.CAPTURE.REFUNDED",
            "resource": { "supplementary_data": { "related_ids": { "order_id": "ORD3" } } }
        });
        assert_eq!(webhook_order_id(&refunded), None);
        assert_eq!(
            webhook_order_id(&json!({ "event_type": "X", "resource": {} })),
            None
        );
    }

    #[test]
    fn order_ids_cannot_escape_the_url_path() {
        assert!(valid_order_id("5O190127TN364715T"));
        assert!(!valid_order_id(""));
        assert!(!valid_order_id("../v1/oauth2/token"));
        assert!(!valid_order_id("A?B"));
        assert!(!valid_order_id(&"A".repeat(65)));
    }

    #[test]
    fn verification_posts_the_event_exactly_as_received() {
        let raw =
            "{\"id\":\"WH-1\",  \"resource\":{\"amount\":{\"value\":\"500.00\"}},\"a\":1.0}\n";
        let t = Transmission {
            auth_algo: "SHA256withRSA",
            cert_url: "https://api.sandbox.paypal.com/v1/notifications/certs/CERT-1",
            id: "tx-1",
            sig: "c2ln+/=",
            time: "2026-11-28T10:00:00Z",
        };
        let payload = verify_payload(&t, "WH-ID", raw);
        // the event sits in the payload byte for byte (only the trailing newline is trimmed)
        assert!(payload.contains(&format!("\"webhook_event\":{}", raw.trim())));
        let parsed: Value = serde_json::from_str(&payload).expect("payload is valid JSON");
        assert_eq!(parsed["webhook_id"], "WH-ID");
        assert_eq!(parsed["transmission_sig"], "c2ln+/=");
        assert_eq!(parsed["webhook_event"]["id"], "WH-1");
    }

    #[test]
    fn error_detail_names_the_issue() {
        let reply = HttpReply {
            status: 422,
            json: json!({
                "name": "UNPROCESSABLE_ENTITY",
                "message": "The requested action could not be performed.",
                "details": [{ "issue": "ORDER_ALREADY_CAPTURED" }]
            }),
            text: String::new(),
        };
        assert_eq!(issue(&reply), Some("ORDER_ALREADY_CAPTURED"));
        assert_eq!(
            error_detail(&reply).as_deref(),
            Some("ORDER_ALREADY_CAPTURED: The requested action could not be performed.")
        );
    }

    #[test]
    fn validate_rejects_a_bad_account_config() {
        let i = input(None, 100_000);
        assert!(Paypal.validate(&creds(&[]), &i, true).is_ok());
        assert!(
            Paypal
                .validate(&creds(&[("idr_rate", "16000")]), &i, true)
                .is_ok()
        );
        // PayPal always charges USD: a `currency` left on an older account is ignored, not refused
        assert!(
            Paypal
                .validate(&creds(&[("currency", "IDR")]), &i, true)
                .is_ok()
        );
        assert!(
            Paypal
                .validate(&creds(&[("idr_rate", "abc")]), &i, true)
                .is_err()
        );
    }
}
