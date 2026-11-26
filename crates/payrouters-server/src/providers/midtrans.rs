//! Midtrans: Core API (`/v2/charge`) for the PayRouters Snap flow, Midtrans Snap
//! (`/snap/v1/transactions`) for provider-hosted checkout.

use async_trait::async_trait;
use axum::http::HeaderMap;
use chrono::{DateTime, NaiveDateTime, Utc};
use payrouters_types::{Instructions, Method, Mode, PaymentStatus, Provider};
use serde_json::{Value, json};
use uuid::Uuid;

use super::*;
use crate::util::{secure_eq, sha512_hex};

pub struct Midtrans;

const METHODS: [Method; 6] = [
    Method::VaBca,
    Method::VaBni,
    Method::VaBri,
    Method::VaMandiri,
    Method::VaPermata,
    Method::Qris,
];

fn api_base(mode: Mode) -> &'static str {
    match mode {
        Mode::Sandbox => "https://api.sandbox.midtrans.com",
        Mode::Live => "https://api.midtrans.com",
    }
}

fn app_base(mode: Mode) -> &'static str {
    match mode {
        Mode::Sandbox => "https://app.sandbox.midtrans.com",
        Mode::Live => "https://app.midtrans.com",
    }
}

fn truncate_chars(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

fn customer_details(input: &ChargeInput<'_>) -> Option<Value> {
    let c = input.customer?;
    let mut v = serde_json::Map::new();
    if let Some(n) = &c.name {
        v.insert("first_name".into(), json!(truncate_chars(n, 255)));
    }
    if let Some(e) = &c.email {
        v.insert("email".into(), json!(e));
    }
    if let Some(p) = &c.phone {
        v.insert("phone".into(), json!(p));
    }
    (!v.is_empty()).then_some(Value::Object(v))
}

fn item_details(input: &ChargeInput<'_>) -> Option<Value> {
    if input.items.is_empty() {
        return None;
    }
    Some(Value::Array(
        input
            .items
            .iter()
            .enumerate()
            .map(|(i, it)| {
                json!({
                    "id": truncate_chars(it.id.as_deref().unwrap_or(&format!("item-{}", i + 1)), 50),
                    "price": it.price,
                    "quantity": it.quantity,
                    "name": truncate_chars(&it.name, 50),
                })
            })
            .collect(),
    ))
}

fn common_body(input: &ChargeInput<'_>) -> serde_json::Map<String, Value> {
    let mut body = serde_json::Map::new();
    body.insert(
        "transaction_details".into(),
        json!({ "order_id": input.payment_id.to_string(), "gross_amount": input.amount }),
    );
    if let Some(c) = customer_details(input) {
        body.insert("customer_details".into(), c);
    }
    if let Some(i) = item_details(input) {
        body.insert("item_details".into(), i);
    }
    body
}

pub(crate) fn core_charge_body(input: &ChargeInput<'_>, method: Method) -> Value {
    let mut body = common_body(input);
    body.insert(
        "custom_expiry".into(),
        json!({ "expiry_duration": minutes_until(input.expires_at), "unit": "minute" }),
    );
    match method {
        Method::Qris => {
            body.insert("payment_type".into(), json!("qris"));
            body.insert("qris".into(), json!({ "acquirer": "gopay" }));
        }
        Method::VaMandiri => {
            body.insert("payment_type".into(), json!("echannel"));
            body.insert(
                "echannel".into(),
                json!({ "bill_info1": "Payment", "bill_info2": truncate_chars(input.order_id, 30) }),
            );
        }
        m => {
            body.insert("payment_type".into(), json!("bank_transfer"));
            body.insert(
                "bank_transfer".into(),
                json!({ "bank": m.bank().unwrap_or("bca") }),
            );
        }
    }
    Value::Object(body)
}

pub(crate) fn snap_body(input: &ChargeInput<'_>) -> Value {
    let mut body = common_body(input);
    body.insert(
        "expiry".into(),
        json!({ "unit": "minutes", "duration": minutes_until(input.expires_at) }),
    );
    if let Some(url) = input.return_url {
        body.insert("callbacks".into(), json!({ "finish": url }));
    }
    Value::Object(body)
}

pub(crate) fn parse_instructions(method: Method, v: &Value) -> Option<Instructions> {
    match method {
        Method::Qris => {
            let qr_string = str_field(v, "qr_string")?.to_string();
            let qr_image_url = v
                .get("actions")
                .and_then(Value::as_array)
                .and_then(|a| {
                    a.iter()
                        .find(|x| str_field(x, "name") == Some("generate-qr-code"))
                })
                .and_then(|x| str_field(x, "url"))
                .map(str::to_string);
            Some(Instructions::Qris {
                qr_string,
                qr_image_url,
            })
        }
        Method::VaMandiri => Some(Instructions::VirtualAccount {
            bank: "mandiri".into(),
            account_number: None,
            account_name: None,
            biller_code: str_field(v, "biller_code").map(str::to_string),
            bill_key: str_field(v, "bill_key").map(str::to_string),
        })
        .filter(|i| {
            matches!(
                i,
                Instructions::VirtualAccount {
                    bill_key: Some(_),
                    ..
                }
            )
        }),
        Method::VaPermata => Some(Instructions::VirtualAccount {
            bank: "permata".into(),
            account_number: Some(str_field(v, "permata_va_number")?.to_string()),
            account_name: None,
            biller_code: None,
            bill_key: None,
        }),
        m => {
            let va = v.get("va_numbers")?.as_array()?.first()?;
            Some(Instructions::VirtualAccount {
                bank: str_field(va, "bank")
                    .map(str::to_string)
                    .unwrap_or_else(|| m.bank().unwrap_or_default().to_string()),
                account_number: Some(str_field(va, "va_number")?.to_string()),
                account_name: None,
                biller_code: None,
                bill_key: None,
            })
        }
    }
}

pub(crate) fn map_status(
    transaction_status: &str,
    fraud_status: Option<&str>,
) -> Option<PaymentStatus> {
    match transaction_status {
        "capture" => match fraud_status {
            Some("challenge") => Some(PaymentStatus::Pending),
            Some("deny") => Some(PaymentStatus::Failed),
            _ => Some(PaymentStatus::Paid),
        },
        "settlement" => Some(PaymentStatus::Paid),
        "pending" => Some(PaymentStatus::Pending),
        "deny" | "failure" => Some(PaymentStatus::Failed),
        "cancel" => Some(PaymentStatus::Cancelled),
        "expire" => Some(PaymentStatus::Expired),
        // refunds, chargebacks, authorize: not a cash-in state change
        _ => None,
    }
}

pub(crate) fn method_from(v: &Value) -> Option<Method> {
    match str_field(v, "payment_type")? {
        "bank_transfer" => {
            if let Some(bank) = v
                .get("va_numbers")
                .and_then(Value::as_array)
                .and_then(|a| a.first())
                .and_then(|x| str_field(x, "bank"))
            {
                Method::from_bank(bank)
            } else if v.get("permata_va_number").is_some() {
                Some(Method::VaPermata)
            } else {
                None
            }
        }
        "echannel" => Some(Method::VaMandiri),
        "qris" => Some(Method::Qris),
        _ => None,
    }
}

/// Midtrans times are WIB (UTC+7) without an offset: `2026-11-26 10:00:00`.
pub(crate) fn parse_wib(s: &str) -> Option<DateTime<Utc>> {
    let naive = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").ok()?;
    Some(naive.and_utc() - chrono::Duration::hours(7))
}

fn gross_amount(v: &Value) -> Option<i64> {
    match v.get("gross_amount")? {
        Value::String(s) => s.parse::<f64>().ok().map(|f| f.round() as i64),
        Value::Number(n) => n.as_f64().map(|f| f.round() as i64),
        _ => None,
    }
}

/// Shared by webhook notifications and `GET /v2/{id}/status`, which have the same shape.
pub(crate) fn status_from_payload(v: &Value) -> Option<StatusUpdate> {
    let status = map_status(
        str_field(v, "transaction_status")?,
        str_field(v, "fraud_status"),
    )?;
    let mut u = StatusUpdate::new(status);
    u.method = method_from(v);
    u.provider_ref = str_field(v, "transaction_id").map(str::to_string);
    u.amount = gross_amount(v);
    if status == PaymentStatus::Paid {
        u.paid_at = str_field(v, "settlement_time")
            .or_else(|| str_field(v, "transaction_time"))
            .and_then(parse_wib);
    }
    Some(u)
}

pub(crate) fn expected_signature(v: &Value, server_key: &str) -> Option<String> {
    Some(sha512_hex(format!(
        "{}{}{}{}",
        str_field(v, "order_id")?,
        str_field(v, "status_code")?,
        str_field(v, "gross_amount")?,
        server_key
    )))
}

fn body_status_code(v: &Value) -> Option<u16> {
    match v.get("status_code")? {
        Value::String(s) => s.parse().ok(),
        Value::Number(n) => n.as_u64().map(|n| n as u16),
        _ => None,
    }
}

fn check_reply(reply: &HttpReply) -> Result<(), ProviderError> {
    let code = body_status_code(&reply.json).unwrap_or(reply.status);
    if (200..300).contains(&code) {
        return Ok(());
    }
    let mut detail = str_field(&reply.json, "status_message")
        .unwrap_or_default()
        .to_string();
    if let Some(msgs) = reply
        .json
        .get("validation_messages")
        .and_then(Value::as_array)
    {
        let joined: Vec<&str> = msgs.iter().filter_map(Value::as_str).collect();
        if !joined.is_empty() {
            detail = format!("{detail}: {}", joined.join("; "));
        }
    }
    let mut r = HttpReply {
        status: code,
        json: reply.json.clone(),
        text: reply.text.clone(),
    };
    if code == 0 {
        r.status = 502;
    }
    Err(fail_by_status("Midtrans", &r, Some(detail)))
}

#[async_trait]
impl ProviderAdapter for Midtrans {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            provider: Provider::Midtrans,
            supports_sandbox: true,
            supports_provider_checkout: true,
            credential_fields: vec![CredentialField {
                key: "server_key",
                label: "Server key",
                secret: true,
                required: true,
                placeholder: "SB-Mid-server-…",
            }],
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
        let key = ctx.creds.require("server_key")?;
        let reply = send(
            ctx.http
                .post(format!("{}/v2/charge", api_base(ctx.mode)))
                .basic_auth(key, Some(""))
                .header("Accept", "application/json")
                .json(&core_charge_body(input, method)),
        )
        .await?;
        check_reply(&reply)?;
        let instructions = parse_instructions(method, &reply.json).ok_or_else(|| {
            ProviderError::Unavailable("Midtrans response lacked payment instructions".into())
        })?;
        Ok(ChargeOutput {
            instructions,
            provider_ref: str_field(&reply.json, "transaction_id").map(str::to_string),
            provider_data: json!({ "kind": "core" }),
            payable_amount: None,
        })
    }

    async fn create_hosted(
        &self,
        ctx: &ProviderCtx<'_>,
        input: &ChargeInput<'_>,
    ) -> Result<ChargeOutput, ProviderError> {
        let key = ctx.creds.require("server_key")?;
        let reply = send(
            ctx.http
                .post(format!("{}/snap/v1/transactions", app_base(ctx.mode)))
                .basic_auth(key, Some(""))
                .header("Accept", "application/json")
                .json(&snap_body(input)),
        )
        .await?;
        if !(200..300).contains(&reply.status) {
            let detail = reply
                .json
                .get("error_messages")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join("; ")
                });
            return Err(fail_by_status("Midtrans Snap", &reply, detail));
        }
        let url = str_field(&reply.json, "redirect_url").ok_or_else(|| {
            ProviderError::Unavailable("Midtrans Snap response lacked redirect_url".into())
        })?;
        Ok(ChargeOutput {
            instructions: Instructions::Redirect {
                url: url.to_string(),
            },
            provider_ref: str_field(&reply.json, "token").map(str::to_string),
            provider_data: json!({ "kind": "snap" }),
            payable_amount: None,
        })
    }

    async fn fetch_status(
        &self,
        ctx: &ProviderCtx<'_>,
        payment: &PaymentRef<'_>,
    ) -> Result<Option<StatusUpdate>, ProviderError> {
        let key = ctx.creds.require("server_key")?;
        let reply = send(
            ctx.http
                .get(format!("{}/v2/{}/status", api_base(ctx.mode), payment.id))
                .basic_auth(key, Some(""))
                .header("Accept", "application/json"),
        )
        .await?;
        match body_status_code(&reply.json).unwrap_or(reply.status) {
            404 => Ok(None),
            _ => {
                check_reply(&reply)?;
                Ok(status_from_payload(&reply.json))
            }
        }
    }

    async fn parse_webhook(
        &self,
        ctx: &ProviderCtx<'_>,
        _headers: &HeaderMap,
        body: &[u8],
    ) -> Result<ParsedWebhook, WebhookError> {
        let v: Value = serde_json::from_slice(body)
            .map_err(|e| WebhookError::Malformed(format!("body is not JSON: {e}")))?;
        let key = ctx
            .creds
            .get("server_key")
            .ok_or_else(|| WebhookError::Unauthorized("server_key not configured".into()))?;
        let expected = expected_signature(&v, key)
            .ok_or_else(|| WebhookError::Unauthorized("missing signature fields".into()))?;
        let given = str_field(&v, "signature_key").unwrap_or_default();
        if !secure_eq(&expected, given) {
            return Err(WebhookError::Unauthorized("signature_key mismatch".into()));
        }
        let order_id = str_field(&v, "order_id").unwrap_or_default();
        let lookup = match Uuid::parse_str(order_id) {
            Ok(id) => Lookup::PaymentId(id),
            Err(_) => Lookup::ProviderRef(order_id.to_string()),
        };
        Ok(ParsedWebhook {
            lookup,
            update: status_from_payload(&v),
        })
    }

    async fn test_connection(&self, ctx: &ProviderCtx<'_>) -> Result<String, ProviderError> {
        let key = ctx.creds.require("server_key")?;
        let reply = send(
            ctx.http
                .get(format!(
                    "{}/v2/payrouters-connection-test/status",
                    api_base(ctx.mode)
                ))
                .basic_auth(key, Some(""))
                .header("Accept", "application/json"),
        )
        .await?;
        let code = body_status_code(&reply.json).unwrap_or(reply.status);
        match code {
            401 | 403 => Err(ProviderError::Rejected(
                "Midtrans rejected the server key".into(),
            )),
            404 | 200 => Ok(format!("Authenticated against Midtrans {}", ctx.mode)),
            other => Err(ProviderError::Unavailable(format!(
                "unexpected Midtrans status {other}"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use payrouters_types::Customer;

    fn input<'a>(
        customer: Option<&'a Customer>,
        items: &'a [payrouters_types::Item],
        opts: &'a Value,
    ) -> ChargeInput<'a> {
        ChargeInput {
            payment_id: Uuid::nil(),
            order_id: "INV-1",
            amount: 150_000,
            expires_at: Utc::now() + chrono::Duration::hours(1),
            description: None,
            customer,
            items,
            return_url: Some("https://app.example.com/thanks"),
            snap_url: "https://pay.example.com/snap/t",
            provider_options: opts,
            method: None,
        }
    }

    #[test]
    fn charge_bodies_by_method() {
        let opts = json!({});
        let i = input(None, &[], &opts);
        let b = core_charge_body(&i, Method::VaBni);
        assert_eq!(b["payment_type"], "bank_transfer");
        assert_eq!(b["bank_transfer"]["bank"], "bni");
        assert_eq!(b["transaction_details"]["gross_amount"], 150_000);
        assert_eq!(b["custom_expiry"]["unit"], "minute");
        assert_eq!(
            core_charge_body(&i, Method::VaMandiri)["payment_type"],
            "echannel"
        );
        assert_eq!(
            core_charge_body(&i, Method::Qris)["qris"]["acquirer"],
            "gopay"
        );
        assert_eq!(
            snap_body(&i)["callbacks"]["finish"],
            "https://app.example.com/thanks"
        );
    }

    #[test]
    fn parses_instructions() {
        let va = json!({ "va_numbers": [{ "bank": "bca", "va_number": "812345" }] });
        match parse_instructions(Method::VaBca, &va).unwrap() {
            Instructions::VirtualAccount {
                bank,
                account_number,
                ..
            } => {
                assert_eq!(bank, "bca");
                assert_eq!(account_number.as_deref(), Some("812345"));
            }
            other => panic!("{other:?}"),
        }
        let echannel = json!({ "bill_key": "123", "biller_code": "70012" });
        assert!(matches!(
            parse_instructions(Method::VaMandiri, &echannel),
            Some(Instructions::VirtualAccount {
                bill_key: Some(_),
                account_number: None,
                ..
            })
        ));
        let qris = json!({ "qr_string": "0002", "actions": [{ "name": "generate-qr-code", "url": "https://q/png" }] });
        assert_eq!(
            parse_instructions(Method::Qris, &qris),
            Some(Instructions::Qris {
                qr_string: "0002".into(),
                qr_image_url: Some("https://q/png".into())
            })
        );
        assert!(parse_instructions(Method::VaBri, &json!({})).is_none());
    }

    #[test]
    fn status_mapping() {
        assert_eq!(map_status("settlement", None), Some(PaymentStatus::Paid));
        assert_eq!(
            map_status("capture", Some("accept")),
            Some(PaymentStatus::Paid)
        );
        assert_eq!(
            map_status("capture", Some("challenge")),
            Some(PaymentStatus::Pending)
        );
        assert_eq!(map_status("expire", None), Some(PaymentStatus::Expired));
        assert_eq!(map_status("cancel", None), Some(PaymentStatus::Cancelled));
        assert_eq!(map_status("deny", None), Some(PaymentStatus::Failed));
        assert_eq!(map_status("refund", None), None);
    }

    #[test]
    fn notification_signature_and_payload() {
        let payload = json!({
            "order_id": "0192f8e0-0000-7000-8000-000000000001",
            "status_code": "200",
            "gross_amount": "150000.00",
            "transaction_status": "settlement",
            "transaction_id": "tx-1",
            "payment_type": "bank_transfer",
            "va_numbers": [{ "bank": "bca", "va_number": "1" }],
            "settlement_time": "2026-11-26 17:00:00",
        });
        let sig = expected_signature(&payload, "SB-key").unwrap();
        assert_eq!(
            sig,
            sha512_hex("0192f8e0-0000-7000-8000-000000000001200150000.00SB-key")
        );
        let u = status_from_payload(&payload).unwrap();
        assert_eq!(u.status, PaymentStatus::Paid);
        assert_eq!(u.amount, Some(150_000));
        assert_eq!(u.method, Some(Method::VaBca));
        assert_eq!(u.paid_at.unwrap().to_rfc3339(), "2026-11-26T10:00:00+00:00");
        assert_eq!(u.provider_ref.as_deref(), Some("tx-1"));
    }
}
