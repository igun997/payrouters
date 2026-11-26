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

pub(crate) fn session_body(input: &ChargeInput<'_>) -> Value {
    let mut body = json!({
        "reference_id": input.payment_id.to_string(),
        "session_type": "PAY",
        "mode": "PAYMENT_LINK",
        "amount": input.amount,
        "currency": "IDR",
        "country": "ID",
        "capture_method": "AUTOMATIC",
        "expires_at": iso(input.expires_at),
        "metadata": { "order_id": input.order_id },
    });
    if let Some(d) = input.description {
        body["description"] = json!(d);
    }
    if let Some(url) = input.return_url {
        body["success_return_url"] = json!(url);
        body["cancel_return_url"] = json!(url);
    }
    if let Some(c) = input.customer
        && let Some(name) = &c.name
    {
        let mut cust = json!({
            "reference_id": format!("cust-{}", input.payment_id.simple()),
            "type": "INDIVIDUAL",
            "individual_detail": { "given_names": name },
        });
        if let Some(e) = &c.email {
            cust["email"] = json!(e);
        }
        if let Some(p) = &c.phone {
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
            u.amount = data.get("request_amount").and_then(Value::as_i64);
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
            u.amount = data.get("amount").and_then(Value::as_i64);
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
            return Err(fail_by_status("Xendit", &reply, error_detail(&reply)));
        }
        let instructions = parse_actions(method, &reply.json).ok_or_else(|| {
            ProviderError::Unavailable("Xendit response lacked payment instructions".into())
        })?;
        Ok(ChargeOutput {
            instructions,
            provider_ref: str_field(&reply.json, "payment_request_id").map(str::to_string),
            provider_data: json!({ "kind": "payment_request" }),
            payable_amount: None,
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
        u.amount = reply
            .json
            .get("request_amount")
            .or_else(|| reply.json.get("amount"))
            .and_then(Value::as_i64);
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
            // Key is valid but lacks the Balance read permission: still proves authentication.
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
}
