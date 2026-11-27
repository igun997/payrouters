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

/// Midtrans: BCA virtual accounts accept at most 30 characters in a customer name.
const BCA_NAME_MAX: usize = 30;
/// Dynamic QRIS and Snap tokens live at most 7 days; a longer expiry is rejected or ignored.
const MAX_EXPIRY_MINUTES: i64 = 7 * 24 * 60;

fn customer_details(input: &ChargeInput<'_>, name_max: usize) -> Option<Value> {
    let c = input.customer?;
    let mut v = serde_json::Map::new();
    if let Some(n) = &c.name {
        v.insert("first_name".into(), json!(truncate_chars(n, name_max)));
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

fn common_body(input: &ChargeInput<'_>, name_max: usize) -> serde_json::Map<String, Value> {
    let mut body = serde_json::Map::new();
    body.insert(
        "transaction_details".into(),
        json!({ "order_id": input.payment_id.to_string(), "gross_amount": input.amount }),
    );
    if let Some(c) = customer_details(input, name_max) {
        body.insert("customer_details".into(), c);
    }
    if let Some(i) = item_details(input) {
        body.insert("item_details".into(), i);
    }
    body
}

pub(crate) fn core_charge_body(input: &ChargeInput<'_>, method: Method) -> Value {
    let name_max = if method == Method::VaBca {
        BCA_NAME_MAX
    } else {
        255
    };
    let mut body = common_body(input, name_max);
    let minutes = match method {
        Method::Qris => minutes_until(input.expires_at).min(MAX_EXPIRY_MINUTES),
        _ => minutes_until(input.expires_at),
    };
    body.insert(
        "custom_expiry".into(),
        json!({ "expiry_duration": minutes, "unit": "minute" }),
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
    // The hosted page may end up on a BCA virtual account, so use its stricter name limit.
    let mut body = common_body(input, BCA_NAME_MAX);
    body.insert(
        "expiry".into(),
        json!({ "unit": "minutes", "duration": minutes_until(input.expires_at).min(MAX_EXPIRY_MINUTES) }),
    );
    if let Some(url) = input.return_url {
        body.insert("callbacks".into(), json!({ "finish": url }));
    }
    Value::Object(body)
}

pub(crate) fn parse_instructions(method: Method, v: &Value) -> Option<Instructions> {
    match method {
        Method::Qris => {
            // The QR text is not part of every QRIS response; the image URL in `actions` always is.
            // Without the text the Snap page shows Midtrans' own image.
            let action_url = |name: &str| {
                v.get("actions")
                    .and_then(Value::as_array)
                    .and_then(|a| a.iter().find(|x| str_field(x, "name") == Some(name)))
                    .and_then(|x| str_field(x, "url"))
                    .map(str::to_string)
            };
            let qr_image_url =
                action_url("generate-qr-code").or_else(|| action_url("generate-qr-code-v2"));
            let qr_string = str_field(v, "qr_string").map(str::to_string);
            if qr_string.is_none() && qr_image_url.is_none() {
                return None;
            }
            Some(Instructions::Qris {
                qr_string: qr_string.unwrap_or_default(),
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
        // Midtrans: when fraud_status is present it must be accept before an order is fulfilled.
        "settlement" => match fraud_status {
            Some("deny") => Some(PaymentStatus::Failed),
            _ => Some(PaymentStatus::Paid),
        },
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

/// A signed field as Midtrans wrote it: strings are used verbatim, a bare number as printed.
fn signed_field(v: &Value, key: &str) -> Option<String> {
    match v.get(key)? {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

pub(crate) fn expected_signature(v: &Value, server_key: &str) -> Option<String> {
    Some(sha512_hex(format!(
        "{}{}{}{}",
        signed_field(v, "order_id")?,
        signed_field(v, "status_code")?,
        signed_field(v, "gross_amount")?,
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
    if code == 403 {
        detail = format!(
            "{detail} (the server key is missing a permission: charges need Core API Write, Snap needs Snap Checkout Write, status checks need a Read permission; see Settings > Access Keys)"
        );
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
            credential_fields: vec![
                CredentialField {
                    key: "server_key",
                    label: "Server key",
                    secret: true,
                    required: true,
                    placeholder: "SB-Mid-server-…",
                },
                // Since 28 Oct 2026 Midtrans signs notifications with this key, not the server key.
                // Merchants who integrated earlier have the same value in both until they rotate.
                CredentialField {
                    key: "signature_secret_key",
                    label: "Signature secret key",
                    secret: true,
                    required: false,
                    placeholder: "Access Keys > Signature Secret Key",
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
        // The Signature Secret Key signs notifications. The server key is accepted too: it was the
        // signing key before 28 Oct 2026 and still is for merchants who never rotated either key.
        let keys: Vec<&str> = ["signature_secret_key", "server_key"]
            .into_iter()
            .filter_map(|k| ctx.creds.get(k))
            .collect();
        if keys.is_empty() {
            return Err(WebhookError::Unauthorized(
                "no signature key configured for this account".into(),
            ));
        }
        let given = str_field(&v, "signature_key").unwrap_or_default();
        let authentic = keys.iter().any(|key| {
            expected_signature(&v, key).is_some_and(|expected| secure_eq(&expected, given))
        });
        if expected_signature(&v, keys[0]).is_none() {
            return Err(WebhookError::Unauthorized(
                "missing signature fields".into(),
            ));
        }
        if !authentic {
            return Err(WebhookError::Unauthorized(
                "signature_key mismatch: Midtrans signs notifications with the Signature Secret Key (Settings > Access Keys); add it to this provider account".into(),
            ));
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
                "Midtrans rejected the server key (wrong key, revoked or expired, or without a Read permission)".into(),
            )),
            404 | 200 => Ok(format!(
                "Authenticated against Midtrans {}. This checks that the key is valid, not that it has Write permission for charges.",
                ctx.mode
            )),
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

    #[test]
    fn bca_names_and_long_expiries_are_clamped_to_what_midtrans_accepts() {
        let opts = json!({});
        let customer = Customer {
            name: Some("N".repeat(60)),
            email: None,
            phone: None,
        };
        let mut i = input(Some(&customer), &[], &opts);
        i.expires_at = Utc::now() + chrono::Duration::days(30);

        let bca = core_charge_body(&i, Method::VaBca);
        assert_eq!(
            bca["customer_details"]["first_name"]
                .as_str()
                .unwrap()
                .len(),
            30
        );
        let bni = core_charge_body(&i, Method::VaBni);
        assert_eq!(
            bni["customer_details"]["first_name"]
                .as_str()
                .unwrap()
                .len(),
            60
        );
        // bank transfers may live long, dynamic QRIS and Snap tokens at most 7 days
        assert!(bni["custom_expiry"]["expiry_duration"].as_i64().unwrap() > 7 * 24 * 60);
        let qris = core_charge_body(&i, Method::Qris);
        assert_eq!(qris["custom_expiry"]["expiry_duration"], 7 * 24 * 60);
        assert_eq!(snap_body(&i)["expiry"]["duration"], 7 * 24 * 60);
        assert_eq!(
            snap_body(&i)["customer_details"]["first_name"]
                .as_str()
                .unwrap()
                .len(),
            30
        );
    }

    #[test]
    fn qris_instructions_survive_a_response_without_the_qr_text() {
        let image_only = json!({ "actions": [
            { "name": "generate-qr-code-v2", "url": "https://q/v2.png" },
            { "name": "deeplink-redirect", "url": "https://q/app" }
        ] });
        assert_eq!(
            parse_instructions(Method::Qris, &image_only),
            Some(Instructions::Qris {
                qr_string: String::new(),
                qr_image_url: Some("https://q/v2.png".into())
            })
        );
        // the original image wins over the ASPI one when both are listed
        let both = json!({ "qr_string": "0002", "actions": [
            { "name": "generate-qr-code-v2", "url": "https://q/v2.png" },
            { "name": "generate-qr-code", "url": "https://q/v1.png" }
        ] });
        assert_eq!(
            parse_instructions(Method::Qris, &both),
            Some(Instructions::Qris {
                qr_string: "0002".into(),
                qr_image_url: Some("https://q/v1.png".into())
            })
        );
        assert_eq!(parse_instructions(Method::Qris, &json!({})), None);
    }

    #[test]
    fn denied_fraud_status_is_never_a_paid_settlement() {
        assert_eq!(
            map_status("settlement", Some("accept")),
            Some(PaymentStatus::Paid)
        );
        assert_eq!(map_status("settlement", None), Some(PaymentStatus::Paid));
        assert_eq!(
            map_status("settlement", Some("deny")),
            Some(PaymentStatus::Failed)
        );
        assert_eq!(
            map_status("capture", Some("challenge")),
            Some(PaymentStatus::Pending)
        );
    }

    #[test]
    fn signature_fields_may_arrive_as_numbers() {
        let text = json!({ "order_id": "o-1", "status_code": "200", "gross_amount": "100.00" });
        let numeric = json!({ "order_id": "o-1", "status_code": 200, "gross_amount": "100.00" });
        assert_eq!(
            expected_signature(&text, "k"),
            expected_signature(&numeric, "k")
        );
        assert!(expected_signature(&json!({ "order_id": "o-1" }), "k").is_none());
    }
}
