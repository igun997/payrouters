//! Xendit: Payments API v3 (`/v3/payment_requests`) for the PayRouters Snap flow,
//! Payment Session (`/sessions`, mode PAYMENT_LINK) for provider-hosted checkout.

use async_trait::async_trait;
use axum::http::HeaderMap;
use chrono::{DateTime, Utc};
use payrouters_types::{Instructions, Method, PaymentStatus, Provider};
use serde_json::{Value, json};
use uuid::Uuid;

use super::*;
use crate::util::secure_eq;

pub struct Xendit;

const BASE: &str = "https://api.xendit.co";
const API_VERSION: &str = "2024-11-11";

const METHODS: [Method; 6] = [
    Method::VaBca,
    Method::VaBni,
    Method::VaBri,
    Method::VaMandiri,
    Method::VaPermata,
    Method::Qris,
];

pub(crate) fn channel_code(m: Method) -> &'static str {
    match m {
        Method::VaBca => "BCA_VIRTUAL_ACCOUNT",
        Method::VaBni => "BNI_VIRTUAL_ACCOUNT",
        Method::VaBri => "BRI_VIRTUAL_ACCOUNT",
        Method::VaMandiri => "MANDIRI_VIRTUAL_ACCOUNT",
        Method::VaPermata => "PERMATA_VIRTUAL_ACCOUNT",
        Method::Qris => "QRIS",
    }
}

pub(crate) fn method_from_channel(code: &str) -> Option<Method> {
    METHODS.into_iter().find(|m| channel_code(*m) == code)
}

fn iso(t: DateTime<Utc>) -> String {
    t.format("%Y-%m-%dT%H:%M:%S.000Z").to_string()
}

pub(crate) fn payment_request_body(input: &ChargeInput<'_>, method: Method) -> Value {
    let mut props = serde_json::Map::new();
    props.insert("expires_at".into(), json!(iso(input.expires_at)));
    if method != Method::Qris {
        let name = input
            .customer
            .and_then(|c| c.name.as_deref())
            .unwrap_or("PayRouters");
        props.insert(
            "display_name".into(),
            json!(name.chars().take(50).collect::<String>()),
        );
    }
    let mut body = json!({
        "reference_id": input.payment_id.to_string(),
        "type": "PAY",
        "country": "ID",
        "currency": "IDR",
        "request_amount": input.amount,
        "capture_method": "AUTOMATIC",
        "channel_code": channel_code(method),
        "channel_properties": Value::Object(props),
        "metadata": { "order_id": input.order_id },
    });
    if let Some(d) = input.description {
        body["description"] = json!(d);
    }
    body
}

/// Xendit rejects a session that expires in less than 10 minutes (`INVALID_EXPIRY_DATE`).
const SESSION_MIN_LIFETIME: chrono::Duration = chrono::Duration::minutes(10);

/// Xendit wants names alphanumeric: letters, digits and spaces only.
fn plain_name(raw: &str) -> String {
    raw.chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ')
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// `+` and 8 to 15 digits, the E.164 shape Xendit requires for `mobile_number`.
fn e164(raw: &str) -> Option<&str> {
    let digits = raw.strip_prefix('+')?;
    ((8..=15).contains(&digits.len()) && digits.bytes().all(|b| b.is_ascii_digit())).then_some(raw)
}

/// A whole-number amount from a JSON number (Xendit may send `10000` or `10000.0`). Anything that
/// is not a whole number becomes a value no stored amount can equal, so the amount check fails
/// closed instead of being skipped.
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

pub(crate) fn session_body(input: &ChargeInput<'_>) -> Value {
    let expires_at = input
        .expires_at
        .max(Utc::now() + SESSION_MIN_LIFETIME + chrono::Duration::seconds(30));
    let mut body = json!({
        "reference_id": input.payment_id.to_string(),
        "session_type": "PAY",
        "mode": "PAYMENT_LINK",
        "amount": input.amount,
        "currency": "IDR",
        "country": "ID",
        "capture_method": "AUTOMATIC",
        "expires_at": iso(expires_at),
        "metadata": { "order_id": input.order_id },
    });
    if let Some(d) = input.description {
        body["description"] = json!(d);
    }
    // Xendit only accepts HTTPS return URLs; the Snap page still redirects to the real one.
    if let Some(url) = input.return_url.filter(|u| u.starts_with("https://")) {
        body["success_return_url"] = json!(url);
        body["cancel_return_url"] = json!(url);
    }
    if let Some(c) = input.customer
        && let Some(name) = c.name.as_deref().map(plain_name).filter(|n| !n.is_empty())
    {
        let (given, surname) = match name.rsplit_once(' ') {
            Some((given, surname)) => (given.to_string(), Some(surname.to_string())),
            None => (name, None),
        };
        let mut detail = json!({ "given_names": given });
        if let Some(s) = surname {
            detail["surname"] = json!(s);
        }
        let mut cust = json!({
            // alphanumeric only: no prefix with a dash
            "reference_id": input.payment_id.simple().to_string(),
            "type": "INDIVIDUAL",
            "individual_detail": detail,
        });
        if let Some(e) = c.email.as_deref().filter(|e| e.len() <= 50) {
            cust["email"] = json!(e);
        }
        if let Some(p) = c.phone.as_deref().and_then(e164) {
            cust["mobile_number"] = json!(p);
        }
        body["customer"] = cust;
    }
    body
}

pub(crate) fn parse_actions(method: Method, resp: &Value) -> Option<Instructions> {
    let actions = resp.get("actions")?.as_array()?;
    let find = |descriptor: &str| {
        actions
            .iter()
            .find(|a| str_field(a, "descriptor") == Some(descriptor))
            .and_then(|a| str_field(a, "value"))
            .map(str::to_string)
    };
    match method {
        Method::Qris => Some(Instructions::Qris {
            qr_string: find("QR_STRING")?,
            qr_image_url: None,
        }),
        m => Some(Instructions::VirtualAccount {
            bank: m.bank().unwrap_or_default().to_string(),
            account_number: Some(find("VIRTUAL_ACCOUNT_NUMBER")?),
            account_name: resp
                .get("channel_properties")
                .and_then(|p| str_field(p, "display_name"))
                .map(str::to_string),
            biller_code: None,
            bill_key: None,
        }),
    }
}

pub(crate) fn map_payment_status(s: &str) -> PaymentStatus {
    match s {
        "SUCCEEDED" => PaymentStatus::Paid,
        "EXPIRED" => PaymentStatus::Expired,
        "CANCELED" | "CANCELLED" => PaymentStatus::Cancelled,
        "FAILED" => PaymentStatus::Failed,
        // ACCEPTING_PAYMENTS, REQUIRES_ACTION, AUTHORIZED, PENDING
        _ => PaymentStatus::Pending,
    }
}

pub(crate) fn map_session_status(s: &str) -> PaymentStatus {
    match s {
        "COMPLETED" => PaymentStatus::Paid,
        "EXPIRED" => PaymentStatus::Expired,
        "CANCELED" | "CANCELLED" => PaymentStatus::Cancelled,
        _ => PaymentStatus::Pending,
    }
}

fn parse_time(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|t| t.with_timezone(&Utc))
}

/// Map a webhook body to a lookup and status update.
pub(crate) fn parse_event(v: &Value) -> Result<ParsedWebhook, String> {
    let event = str_field(v, "event").ok_or("missing `event`")?;
    let data = v.get("data").ok_or("missing `data`")?;
    let reference = str_field(data, "reference_id").unwrap_or_default();
    let lookup = match Uuid::parse_str(reference) {
        Ok(id) => Lookup::PaymentId(id),
        Err(_) => Lookup::ProviderRef(reference.to_string()),
    };
    let update = match event {
        "payment.capture" => {
            let mut u =
                StatusUpdate::new(map_payment_status(str_field(data, "status").unwrap_or("")));
            u.method = str_field(data, "channel_code").and_then(method_from_channel);
            u.amount = whole_amount(data.get("request_amount"));
            u.paid_at = data
                .get("captures")
                .and_then(Value::as_array)
                .and_then(|c| c.first())
                .and_then(|c| str_field(c, "capture_timestamp"))
                .or_else(|| str_field(data, "updated"))
                .and_then(parse_time);
            Some(u)
        }
        "payment.failure" => Some(StatusUpdate::new(PaymentStatus::Failed)),
        e if e.starts_with("payment_session.") => {
            let status = map_session_status(str_field(data, "status").unwrap_or(""));
            let mut u = StatusUpdate::new(status);
            u.amount = whole_amount(data.get("amount"));
            u.paid_at = str_field(data, "updated").and_then(parse_time);
            Some(u)
        }
        // payment.authorization and anything newer: acknowledged, no state change
        _ => None,
    };
    Ok(ParsedWebhook { lookup, update })
}

fn error_detail(reply: &HttpReply) -> Option<String> {
    str_field(&reply.json, "message").map(|m| match str_field(&reply.json, "error_code") {
        Some(code) => format!("{code}: {m}"),
        None => m.to_string(),
    })
}

/// A refused charge. A channel Xendit has not activated for the account is a fact about the account.
fn charge_error(reply: &HttpReply) -> ProviderError {
    let detail = error_detail(reply);
    if str_field(&reply.json, "error_code") == Some("CHANNEL_NOT_ACTIVATED") {
        return ProviderError::ChannelInactive(format!(
            "Xendit responded {}: {}",
            reply.status,
            detail.unwrap_or_default()
        ));
    }
    fail_by_status("Xendit", reply, detail)
}

#[async_trait]
impl ProviderAdapter for Xendit {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            provider: Provider::Xendit,
            supports_sandbox: true,
            supports_provider_checkout: true,
            credential_fields: vec![
                CredentialField {
                    key: "secret_key",
                    label: "Secret API key",
                    secret: true,
                    required: true,
                    placeholder: "xnd_development_…",
                },
                CredentialField {
                    key: "callback_token",
                    label: "Webhook verification token",
                    secret: true,
                    required: true,
                    placeholder: "Dashboard → Webhooks → Verification token",
                },
            ],
            methods: METHODS.to_vec(),
        }
    }

    async fn create_charge(
        &self,
        ctx: &ProviderCtx<'_>,
        input: &ChargeInput<'_>,
    ) -> Result<ChargeOutput, ProviderError> {
        let method = input
            .method
            .ok_or_else(|| ProviderError::Rejected("method required".into()))?;
        let key = ctx.creds.require("secret_key")?;
        let reply = send(
            ctx.http
                .post(format!("{BASE}/v3/payment_requests"))
                .basic_auth(key, Some(""))
                .header("api-version", API_VERSION)
                .json(&payment_request_body(input, method)),
        )
        .await?;
        if !(200..300).contains(&reply.status) {
            return Err(charge_error(&reply));
        }
        let instructions = parse_actions(method, &reply.json).ok_or_else(|| {
            ProviderError::Unavailable("Xendit response lacked payment instructions".into())
        })?;
        Ok(ChargeOutput {
            instructions,
            provider_ref: str_field(&reply.json, "payment_request_id").map(str::to_string),
            provider_data: json!({ "kind": "payment_request" }),
            payable_amount: None,
            expires_at: None,
        })
    }

    async fn create_hosted(
        &self,
        ctx: &ProviderCtx<'_>,
        input: &ChargeInput<'_>,
    ) -> Result<ChargeOutput, ProviderError> {
        let key = ctx.creds.require("secret_key")?;
        let reply = send(
            ctx.http
                .post(format!("{BASE}/sessions"))
                .basic_auth(key, Some(""))
                .json(&session_body(input)),
        )
        .await?;
        if !(200..300).contains(&reply.status) {
            return Err(fail_by_status("Xendit", &reply, error_detail(&reply)));
        }
        let url = str_field(&reply.json, "payment_link_url").ok_or_else(|| {
            ProviderError::Unavailable("Xendit session lacked payment_link_url".into())
        })?;
        Ok(ChargeOutput {
            instructions: Instructions::Redirect {
                url: url.to_string(),
            },
            provider_ref: str_field(&reply.json, "payment_session_id").map(str::to_string),
            provider_data: json!({ "kind": "session" }),
            payable_amount: None,
            expires_at: enforced_expiry(
                input.expires_at,
                Some(SESSION_MIN_LIFETIME + chrono::Duration::seconds(30)),
                None,
            ),
        })
    }

    async fn fetch_status(
        &self,
        ctx: &ProviderCtx<'_>,
        payment: &PaymentRef<'_>,
    ) -> Result<Option<StatusUpdate>, ProviderError> {
        let Some(id) = payment.provider_ref else {
            return Ok(None);
        };
        let key = ctx.creds.require("secret_key")?;
        let session = str_field(payment.provider_data, "kind") == Some("session");
        let req = if session {
            ctx.http.get(format!("{BASE}/sessions/{id}"))
        } else {
            ctx.http
                .get(format!("{BASE}/v3/payment_requests/{id}"))
                .header("api-version", API_VERSION)
        };
        let reply = send(req.basic_auth(key, Some(""))).await?;
        if reply.status == 404 {
            return Ok(None);
        }
        if !(200..300).contains(&reply.status) {
            return Err(fail_by_status("Xendit", &reply, error_detail(&reply)));
        }
        let raw = str_field(&reply.json, "status").unwrap_or("");
        let status = if session {
            map_session_status(raw)
        } else {
            map_payment_status(raw)
        };
        let mut u = StatusUpdate::new(status);
        u.provider_ref = Some(id.to_string());
        u.amount = whole_amount(
            reply
                .json
                .get("request_amount")
                .or_else(|| reply.json.get("amount")),
        );
        if status == PaymentStatus::Paid {
            u.paid_at = str_field(&reply.json, "updated").and_then(parse_time);
        }
        Ok(Some(u))
    }

    async fn parse_webhook(
        &self,
        ctx: &ProviderCtx<'_>,
        headers: &HeaderMap,
        body: &[u8],
    ) -> Result<ParsedWebhook, WebhookError> {
        let expected = ctx
            .creds
            .get("callback_token")
            .ok_or_else(|| WebhookError::Unauthorized("callback_token not configured".into()))?;
        let given = headers
            .get("x-callback-token")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        if !secure_eq(expected, given) {
            return Err(WebhookError::Unauthorized(
                "x-callback-token mismatch".into(),
            ));
        }
        let v: Value = serde_json::from_slice(body)
            .map_err(|e| WebhookError::Malformed(format!("body is not JSON: {e}")))?;
        match parse_event(&v) {
            Ok(p) => Ok(p),
            // Authentic but a shape we do not model (e.g. legacy invoice callbacks): acknowledge.
            Err(_) => Ok(ParsedWebhook {
                lookup: Lookup::Ignore,
                update: None,
            }),
        }
    }

    async fn test_connection(&self, ctx: &ProviderCtx<'_>) -> Result<String, ProviderError> {
        let key = ctx.creds.require("secret_key")?;
        let reply = send(
            ctx.http
                .get(format!("{BASE}/balance"))
                .basic_auth(key, Some("")),
        )
        .await?;
        match reply.status {
            200..=299 => Ok("Authenticated against Xendit".to_string()),
            401 => Err(ProviderError::Rejected(
                "Xendit rejected the secret key".into(),
            )),
            403 if str_field(&reply.json, "error_code") == Some("UNAUTHORIZED_SENDER_IP") => {
                Err(ProviderError::Rejected(
                    "Xendit blocked this server's IP address: add it to the IP allowlist in Xendit settings".into(),
                ))
            }
            // Key is valid but lacks the Balance read permission: still proves authentication.
            // (A key needs Money-in Write to create charges; this call cannot prove that.)
            403 => Ok(
                "Authenticated against Xendit (key has no Balance permission, which is fine)"
                    .to_string(),
            ),
            _ => Err(fail_by_status("Xendit", &reply, error_detail(&reply))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use payrouters_types::Customer;

    fn input<'a>(customer: Option<&'a Customer>, opts: &'a Value) -> ChargeInput<'a> {
        ChargeInput {
            payment_id: Uuid::nil(),
            order_id: "INV-1",
            amount: 150_000,
            expires_at: DateTime::parse_from_rfc3339("2026-11-27T10:00:00Z")
                .unwrap()
                .into(),
            description: Some("Order"),
            customer,
            items: &[],
            return_url: Some("https://app.example.com/thanks"),
            snap_url: "https://pay.example.com/snap/t",
            snap_api_url: "https://pay.example.com/snap/api/t",
            provider_options: opts,
            method: None,
        }
    }

    #[test]
    fn request_bodies() {
        let opts = json!({});
        let c = Customer {
            name: Some("Budi".into()),
            email: Some("b@x.id".into()),
            phone: None,
        };
        let i = input(Some(&c), &opts);
        let b = payment_request_body(&i, Method::VaBri);
        assert_eq!(b["channel_code"], "BRI_VIRTUAL_ACCOUNT");
        assert_eq!(b["channel_properties"]["display_name"], "Budi");
        assert_eq!(
            b["channel_properties"]["expires_at"],
            "2026-11-27T10:00:00.000Z"
        );
        assert_eq!(b["request_amount"], 150_000);
        let q = payment_request_body(&i, Method::Qris);
        assert!(q["channel_properties"].get("display_name").is_none());
        let s = session_body(&i);
        assert_eq!(s["mode"], "PAYMENT_LINK");
        assert_eq!(s["customer"]["individual_detail"]["given_names"], "Budi");
        assert_eq!(s["success_return_url"], "https://app.example.com/thanks");
        // no name → no customer object (Xendit requires given_names)
        let no_name = Customer {
            name: None,
            email: Some("b@x.id".into()),
            phone: None,
        };
        assert!(
            session_body(&input(Some(&no_name), &opts))
                .get("customer")
                .is_none()
        );
    }

    #[test]
    fn parses_actions() {
        let resp = json!({
            "channel_properties": { "display_name": "Budi" },
            "actions": [{ "type": "PRESENT_TO_CUSTOMER", "descriptor": "VIRTUAL_ACCOUNT_NUMBER", "value": "8881" }]
        });
        assert!(matches!(
            parse_actions(Method::VaBca, &resp),
            Some(Instructions::VirtualAccount { account_number: Some(n), account_name: Some(_), .. }) if n == "8881"
        ));
        let qr = json!({ "actions": [{ "descriptor": "QR_STRING", "value": "0002" }] });
        assert!(matches!(
            parse_actions(Method::Qris, &qr),
            Some(Instructions::Qris { .. })
        ));
        assert!(parse_actions(Method::Qris, &resp).is_none());
    }

    #[test]
    fn webhook_events() {
        let id = Uuid::now_v7();
        let capture = json!({
            "event": "payment.capture",
            "data": {
                "reference_id": id.to_string(), "status": "SUCCEEDED", "channel_code": "BCA_VIRTUAL_ACCOUNT",
                "request_amount": 150000,
                "captures": [{ "capture_timestamp": "2026-11-26T07:28:37.2Z" }]
            }
        });
        let p = parse_event(&capture).unwrap();
        assert!(matches!(p.lookup, Lookup::PaymentId(x) if x == id));
        let u = p.update.unwrap();
        assert_eq!(u.status, PaymentStatus::Paid);
        assert_eq!(u.method, Some(Method::VaBca));
        assert_eq!(u.amount, Some(150_000));
        assert!(u.paid_at.is_some());

        let expired = json!({ "event": "payment_session.expired", "data": { "reference_id": id.to_string(), "status": "EXPIRED", "amount": 1 } });
        assert_eq!(
            parse_event(&expired).unwrap().update.unwrap().status,
            PaymentStatus::Expired
        );

        let auth =
            json!({ "event": "payment.authorization", "data": { "reference_id": id.to_string() } });
        assert!(parse_event(&auth).unwrap().update.is_none());
        assert!(parse_event(&json!({ "external_id": "x" })).is_err());
    }

    #[test]
    fn status_maps() {
        assert_eq!(map_payment_status("SUCCEEDED"), PaymentStatus::Paid);
        assert_eq!(
            map_payment_status("REQUIRES_ACTION"),
            PaymentStatus::Pending
        );
        assert_eq!(map_session_status("COMPLETED"), PaymentStatus::Paid);
        assert_eq!(map_session_status("ACTIVE"), PaymentStatus::Pending);
    }

    #[test]
    fn session_customer_and_urls_follow_xendit_rules() {
        let opts = json!({});
        let c = Customer {
            name: Some("Siti N. O'Brien-Aisyah".into()),
            email: Some(format!("{}@example.com", "a".repeat(45))),
            phone: Some("0812345678".into()),
        };
        let mut i = input(Some(&c), &opts);
        i.return_url = Some("http://app.example.com/thanks");
        let s = session_body(&i);
        // alphanumeric names, split into given names and surname
        assert_eq!(s["customer"]["individual_detail"]["given_names"], "Siti N");
        assert_eq!(
            s["customer"]["individual_detail"]["surname"],
            "OBrienAisyah"
        );
        // alphanumeric reference, over-long email and a non-E.164 number are left out
        assert_eq!(
            s["customer"]["reference_id"],
            "00000000000000000000000000000000"
        );
        assert!(s["customer"].get("email").is_none());
        assert!(s["customer"].get("mobile_number").is_none());
        // https only
        assert!(s.get("success_return_url").is_none());
        assert!(s.get("cancel_return_url").is_none());

        let ok = Customer {
            name: Some("Budi".into()),
            email: Some("b@x.id".into()),
            phone: Some("+628123456789".into()),
        };
        let s = session_body(&input(Some(&ok), &opts));
        assert_eq!(s["customer"]["email"], "b@x.id");
        assert_eq!(s["customer"]["mobile_number"], "+628123456789");
        assert!(s["customer"]["individual_detail"].get("surname").is_none());
    }

    #[test]
    fn a_session_never_expires_sooner_than_xendit_allows() {
        let opts = json!({});
        let mut i = input(None, &opts);
        i.expires_at = Utc::now() + chrono::Duration::minutes(5);
        let s = session_body(&i);
        let at = DateTime::parse_from_rfc3339(s["expires_at"].as_str().unwrap()).unwrap();
        assert!(at.with_timezone(&Utc) > Utc::now() + chrono::Duration::minutes(10));
        // a far expiry is passed through untouched
        i.expires_at = DateTime::parse_from_rfc3339("2099-01-01T00:00:00Z")
            .unwrap()
            .into();
        assert_eq!(session_body(&i)["expires_at"], "2099-01-01T00:00:00.000Z");
    }

    #[test]
    fn webhook_amounts_may_be_written_as_floats_and_odd_ones_never_match() {
        assert_eq!(whole_amount(Some(&json!(10000))), Some(10_000));
        assert_eq!(whole_amount(Some(&json!(10000.0))), Some(10_000));
        assert_eq!(whole_amount(Some(&json!(10000.5))), Some(-1));
        assert_eq!(whole_amount(Some(&json!("10000"))), None);
        assert_eq!(whole_amount(None), None);
        let body = json!({
            "event": "payment.capture",
            "data": { "reference_id": "r-1", "status": "SUCCEEDED", "request_amount": 150000.0 }
        });
        let parsed = parse_event(&body).unwrap();
        assert_eq!(parsed.update.unwrap().amount, Some(150_000));
    }

    #[test]
    fn a_channel_xendit_has_not_activated_is_flagged() {
        let inactive = HttpReply {
            status: 403,
            json: json!({ "error_code": "CHANNEL_NOT_ACTIVATED", "message": "channel not activated" }),
            text: String::new(),
        };
        assert!(matches!(
            charge_error(&inactive),
            ProviderError::ChannelInactive(_)
        ));
        let invalid = HttpReply {
            status: 400,
            json: json!({ "error_code": "API_VALIDATION_ERROR", "message": "bad" }),
            text: String::new(),
        };
        assert!(matches!(charge_error(&invalid), ProviderError::Rejected(_)));
    }
}
