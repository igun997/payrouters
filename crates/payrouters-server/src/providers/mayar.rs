//! Mayar (mayar.id): Headless API v2. A payment is a Single Payment Request
//! (`POST /hl/v2/payments/create`), which Mayar hosts at a `link` the customer is sent to.
//!
//! Mayar's API returns no virtual account numbers or QR codes for a payment request, so the
//! adapter is hosted-only (no standard methods). Mayar's `payment.received` webhook is unsigned:
//! the only fact taken from it is which payment request it names, and the status comes from
//! `GET /hl/v2/payments/{id}`, which the account's own API key authenticates. Pending payments
//! are also polled by the worker.

use async_trait::async_trait;
use axum::http::HeaderMap;
use chrono::{DateTime, Utc};
use payrouters_types::{Instructions, Mode, PaymentStatus, Provider};
use serde_json::{Value, json};
use uuid::Uuid;

use super::*;
use crate::util::secure_eq;

pub struct Mayar;

fn base(mode: Mode) -> &'static str {
    match mode {
        Mode::Sandbox => "https://api.mayar.io/hl/v2",
        Mode::Live => "https://api.mayar.id/hl/v2",
    }
}

/// The title Mayar shows on the payment request page.
const NAME_MAX: usize = 100;

fn iso(t: DateTime<Utc>) -> String {
    t.format("%Y-%m-%dT%H:%M:%S.000Z").to_string()
}

pub(crate) fn payment_request_body(input: &ChargeInput<'_>, with_redirect: bool) -> Value {
    let name = input
        .description
        .map(str::trim)
        .filter(|d| !d.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("Order {}", input.order_id));
    let mut body = json!({
        "name": truncate(&name, NAME_MAX),
        "amount": input.amount,
        "expiredAt": iso(input.expires_at),
    });
    if let Some(d) = input.description.map(str::trim).filter(|d| !d.is_empty()) {
        body["description"] = json!(d);
    }
    if let Some(c) = input.customer {
        if let Some(e) = &c.email {
            body["email"] = json!(e);
        }
        if let Some(p) = &c.phone {
            body["mobile"] = json!(p);
        }
    }
    if with_redirect && let Some(url) = input.return_url {
        body["redirectUrl"] = json!(url);
    }
    body
}

/// `statusCode`/`messages` envelope; some write endpoints say `message`.
fn api_error(reply: &HttpReply) -> ProviderError {
    let detail = str_field(&reply.json, "messages")
        .or_else(|| str_field(&reply.json, "message"))
        .map(str::to_string);
    fail_by_status("Mayar", reply, detail)
}

/// Payment requests may refuse a `redirectUrl` ("is not supported on payment requests and was not
/// saved"). That is the only 400 worth retrying without it.
fn redirect_refused(reply: &HttpReply) -> bool {
    reply.status == 400
        && str_field(&reply.json, "messages")
            .or_else(|| str_field(&reply.json, "message"))
            .is_some_and(|m| m.contains("redirectUrl"))
}

pub(crate) fn map_status(s: &str) -> PaymentStatus {
    match s.to_ascii_lowercase().as_str() {
        "paid" => PaymentStatus::Paid,
        // closed by the merchant (Change Single Payment Request Status)
        "closed" => PaymentStatus::Cancelled,
        _ => PaymentStatus::Pending,
    }
}

/// A whole-number amount; anything else becomes a value no stored amount can equal, so the
/// amount check fails closed instead of being skipped.
fn whole_amount(v: Option<&Value>) -> Option<i64> {
    let n = v?;
    if let Some(i) = n.as_i64() {
        return Some(i);
    }
    let f = n.as_f64()?;
    if f.is_finite() && f.fract() == 0.0 && f.abs() < 9e15 {
        Some(f as i64)
    } else {
        Some(-1)
    }
}

/// Status of a payment request from its `GET /payments/{id}` detail (`data` of the envelope).
pub(crate) fn status_from_detail(data: &Value, id: &str) -> StatusUpdate {
    let mut u = StatusUpdate::new(map_status(str_field(data, "status").unwrap_or("")));
    u.provider_ref = Some(id.to_string());
    u.amount = whole_amount(data.get("amount"));
    u
}

/// The payment request a `payment.received` webhook names: `data.productId` is the payment link
/// that `payments/create` returned as `id`. Other events and other products are not ours.
pub(crate) fn webhook_target(v: &Value) -> Option<Uuid> {
    if str_field(v, "event")? != "payment.received" {
        return None;
    }
    Uuid::parse_str(str_field(v.get("data")?, "productId")?).ok()
}

/// Whether any request header carries the webhook token: as its whole value, or as a bearer token.
/// Mayar's documentation does not name the header, so none is assumed.
pub(crate) fn carries_token(headers: &HeaderMap, token: &str) -> bool {
    headers.values().filter_map(|v| v.to_str().ok()).any(|v| {
        let v = v.trim();
        let bearer = v
            .get(..7)
            .filter(|p| p.eq_ignore_ascii_case("bearer "))
            .map(|_| v[7..].trim());
        secure_eq(v, token) || bearer.is_some_and(|b| secure_eq(b, token))
    })
}

/// Header names only, for the error that tells the owner where the token was not found.
fn header_names(headers: &HeaderMap) -> String {
    let mut names: Vec<&str> = headers.keys().map(|k| k.as_str()).collect();
    names.sort_unstable();
    names.join(", ")
}

async fn fetch_detail(
    ctx: &ProviderCtx<'_>,
    id: Uuid,
) -> Result<Option<StatusUpdate>, ProviderError> {
    let key = ctx.creds.require("api_key")?;
    let reply = send(
        ctx.http
            .get(format!("{}/payments/{id}", base(ctx.mode)))
            .bearer_auth(key),
    )
    .await?;
    // 400/404: not a payment request of this account (e.g. a regular Mayar product).
    if matches!(reply.status, 400 | 404) {
        return Ok(None);
    }
    if !(200..300).contains(&reply.status) {
        return Err(api_error(&reply));
    }
    let data = reply.json.get("data").unwrap_or(&Value::Null);
    Ok(Some(status_from_detail(data, &id.to_string())))
}

#[async_trait]
impl ProviderAdapter for Mayar {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            provider: Provider::Mayar,
            supports_sandbox: true,
            supports_provider_checkout: true,
            credential_fields: vec![
                CredentialField {
                    key: "api_key",
                    label: "API key",
                    secret: true,
                    required: true,
                    placeholder: "Mayar dashboard → API Keys (Read & Write)",
                },
                CredentialField {
                    key: "webhook_token",
                    label: "Webhook token",
                    secret: true,
                    required: false,
                    placeholder: "Mayar dashboard → Integration → Webhook Token",
                },
            ],
            methods: vec![],
        }
    }

    async fn create_charge(
        &self,
        _ctx: &ProviderCtx<'_>,
        _input: &ChargeInput<'_>,
    ) -> Result<ChargeOutput, ProviderError> {
        Err(ProviderError::Rejected(
            "Mayar has no standard payment methods; use its hosted checkout".into(),
        ))
    }

    async fn create_hosted(
        &self,
        ctx: &ProviderCtx<'_>,
        input: &ChargeInput<'_>,
    ) -> Result<ChargeOutput, ProviderError> {
        let key = ctx.creds.require("api_key")?;
        let url = format!("{}/payments/create", base(ctx.mode));
        let mut reply = send(
            ctx.http
                .post(&url)
                .bearer_auth(key)
                .json(&payment_request_body(input, true)),
        )
        .await?;
        if redirect_refused(&reply) {
            reply = send(
                ctx.http
                    .post(&url)
                    .bearer_auth(key)
                    .json(&payment_request_body(input, false)),
            )
            .await?;
        }
        if !(200..300).contains(&reply.status) {
            return Err(api_error(&reply));
        }
        let data = reply.json.get("data").unwrap_or(&Value::Null);
        let link = str_field(data, "link")
            .ok_or_else(|| ProviderError::Unavailable("Mayar response lacked a link".into()))?;
        let id = str_field(data, "id")
            .ok_or_else(|| ProviderError::Unavailable("Mayar response lacked an id".into()))?;
        Ok(ChargeOutput {
            instructions: Instructions::Redirect {
                url: link.to_string(),
            },
            provider_ref: Some(id.to_string()),
            provider_data: json!({
                "kind": "payment_request",
                "transaction_id": str_field(data, "transactionId"),
            }),
            payable_amount: None,
        })
    }

    async fn fetch_status(
        &self,
        ctx: &ProviderCtx<'_>,
        payment: &PaymentRef<'_>,
    ) -> Result<Option<StatusUpdate>, ProviderError> {
        let Some(id) = payment.provider_ref.and_then(|r| Uuid::parse_str(r).ok()) else {
            return Ok(None);
        };
        fetch_detail(ctx, id).await
    }

    async fn parse_webhook(
        &self,
        ctx: &ProviderCtx<'_>,
        headers: &HeaderMap,
        body: &[u8],
    ) -> Result<ParsedWebhook, WebhookError> {
        // Mayar's dashboard issues a webhook token "to verify that a webhook comes from Mayar".
        // Its public documentation does not say where the token travels, so when one is set it
        // is accepted wherever a header carries it, and a request without it is refused.
        if let Some(token) = ctx.creds.get("webhook_token")
            && !carries_token(headers, token)
        {
            return Err(WebhookError::Unauthorized(format!(
                "the webhook token is not in this request (headers received: {})",
                header_names(headers)
            )));
        }
        let v: Value = serde_json::from_slice(body)
            .map_err(|e| WebhookError::Malformed(format!("body is not JSON: {e}")))?;
        let ignore = ParsedWebhook {
            lookup: Lookup::Ignore,
            update: None,
        };
        let Some(id) = webhook_target(&v) else {
            return Ok(ignore);
        };
        // The webhook is unsigned: trust only what Mayar's authenticated API says.
        match fetch_detail(ctx, id).await {
            Ok(Some(update)) => Ok(ParsedWebhook {
                lookup: Lookup::ProviderRef(id.to_string()),
                update: Some(update),
            }),
            Ok(None) => Ok(ignore),
            Err(e) => Err(WebhookError::Upstream(format!(
                "could not verify with Mayar: {e}"
            ))),
        }
    }

    async fn test_connection(&self, ctx: &ProviderCtx<'_>) -> Result<String, ProviderError> {
        let key = ctx.creds.require("api_key")?;
        let reply = send(
            ctx.http
                .get(format!("{}/balances", base(ctx.mode)))
                .bearer_auth(key),
        )
        .await?;
        match reply.status {
            200..=299 => Ok(
                "Authenticated against Mayar (this cannot prove the key is Read & Write, which creating payments needs)"
                    .to_string(),
            ),
            401 | 403 => Err(ProviderError::Rejected(
                "Mayar rejected the API key (a sandbox key only works on a sandbox account and the other way round)".into(),
            )),
            _ => Err(api_error(&reply)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use payrouters_types::Customer;

    fn input<'a>(
        customer: Option<&'a Customer>,
        description: Option<&'a str>,
        return_url: Option<&'a str>,
    ) -> ChargeInput<'a> {
        ChargeInput {
            payment_id: Uuid::nil(),
            order_id: "INV-1",
            amount: 150_000,
            expires_at: "2026-11-30T10:00:00Z".parse().unwrap(),
            description,
            customer,
            items: &[],
            return_url,
            snap_url: "https://pay.example.com/snap/t",
            snap_api_url: "https://pay.example.com/snap/api/t",
            provider_options: &Value::Null,
            method: None,
        }
    }

    #[test]
    fn body_has_the_required_fields_and_customer_when_given() {
        let c = Customer {
            name: Some("Budi".into()),
            email: Some("budi@example.com".into()),
            phone: Some("081234567890".into()),
        };
        let b = payment_request_body(&input(Some(&c), Some("Jasa konsultasi"), None), true);
        assert_eq!(b["name"], "Jasa konsultasi");
        assert_eq!(b["amount"], 150_000);
        assert_eq!(b["expiredAt"], "2026-11-30T10:00:00.000Z");
        assert_eq!(b["description"], "Jasa konsultasi");
        assert_eq!(b["email"], "budi@example.com");
        assert_eq!(b["mobile"], "081234567890");
        assert!(b.get("redirectUrl").is_none());
    }

    #[test]
    fn name_falls_back_to_the_order_and_is_capped() {
        let b = payment_request_body(&input(None, None, None), true);
        assert_eq!(b["name"], "Order INV-1");
        assert!(b.get("description").is_none());
        let long = "x".repeat(300);
        let b = payment_request_body(&input(None, Some(&long), None), true);
        assert_eq!(b["name"].as_str().unwrap().chars().count(), NAME_MAX + 1);
    }

    #[test]
    fn redirect_url_is_sent_only_when_asked() {
        let i = input(None, None, Some("https://shop.example.com/done"));
        assert_eq!(
            payment_request_body(&i, true)["redirectUrl"],
            "https://shop.example.com/done"
        );
        assert!(payment_request_body(&i, false).get("redirectUrl").is_none());
    }

    #[test]
    fn only_the_redirect_url_400_is_retried() {
        let refused = HttpReply {
            status: 400,
            json: json!({ "statusCode": 400, "messages": "redirectUrl is not supported on payment requests and was not saved" }),
            text: String::new(),
        };
        assert!(redirect_refused(&refused));
        let other = HttpReply {
            status: 400,
            json: json!({ "statusCode": 400, "messages": "Validation Error" }),
            text: String::new(),
        };
        assert!(!redirect_refused(&other));
    }

    #[test]
    fn statuses_map_to_cash_in_states() {
        assert_eq!(map_status("paid"), PaymentStatus::Paid);
        assert_eq!(map_status("PAID"), PaymentStatus::Paid);
        assert_eq!(map_status("unpaid"), PaymentStatus::Pending);
        assert_eq!(map_status("closed"), PaymentStatus::Cancelled);
        assert_eq!(map_status("something-new"), PaymentStatus::Pending);
    }

    #[test]
    fn detail_carries_the_amount_for_the_mismatch_check() {
        let u = status_from_detail(&json!({ "status": "paid", "amount": 150000 }), "abc");
        assert_eq!(u.status, PaymentStatus::Paid);
        assert_eq!(u.amount, Some(150_000));
        assert_eq!(u.provider_ref.as_deref(), Some("abc"));
        // an edited, fractional or missing amount can never equal a stored one
        let u = status_from_detail(&json!({ "status": "paid", "amount": 1500.5 }), "abc");
        assert_eq!(u.amount, Some(-1));
        let u = status_from_detail(&json!({ "status": "paid" }), "abc");
        assert_eq!(u.amount, None);
    }

    #[test]
    fn only_payment_received_for_a_payment_link_is_ours() {
        let id = "7c9d2e1f-3a4b-4c5d-8e6f-1a2b3c4d5e6f";
        let received =
            json!({ "event": "payment.received", "data": { "productId": id, "id": "x" } });
        assert_eq!(webhook_target(&received), Uuid::parse_str(id).ok());
        let reminder = json!({ "event": "payment.reminder", "data": { "productId": id } });
        assert_eq!(webhook_target(&reminder), None);
        let member = json!({ "event": "membership.memberExpired", "data": {} });
        assert_eq!(webhook_target(&member), None);
        // an id that is not a uuid never reaches a URL path
        let hostile =
            json!({ "event": "payment.received", "data": { "productId": "../balances" } });
        assert_eq!(webhook_target(&hostile), None);
    }

    #[test]
    fn the_webhook_token_may_travel_in_any_header() {
        let token = "wh-token-0123456789";
        let with = |name: &'static str, value: &str| {
            let mut h = HeaderMap::new();
            h.insert("content-type", "application/json".parse().unwrap());
            h.insert(name, value.parse().unwrap());
            h
        };
        assert!(carries_token(&with("x-mayar-token", token), token));
        assert!(carries_token(&with("token", &format!(" {token} ")), token));
        assert!(carries_token(
            &with("authorization", &format!("Bearer {token}")),
            token
        ));
        assert!(carries_token(
            &with("authorization", &format!("bearer {token}")),
            token
        ));
        // near misses and absence never count
        assert!(!carries_token(
            &with("x-mayar-token", "wh-token-012345678"),
            token
        ));
        assert!(!carries_token(
            &with("x-mayar-token", &format!("{token}x")),
            token
        ));
        assert!(!carries_token(&with("authorization", "Basic abc"), token));
        let mut none = HeaderMap::new();
        none.insert("content-type", "application/json".parse().unwrap());
        assert!(!carries_token(&none, token));
        assert!(!carries_token(&HeaderMap::new(), token));
        assert_eq!(header_names(&none), "content-type");
    }
}
