//! IDRX (IDR stablecoin) mint requests as cash-in. A "payment" mints `amount` IDRX to a
//! destination wallet; the customer pays the IDR side through IDRX's checkout or a direct VA.
//!
//! IDRX publishes two signature descriptions that disagree (a legacy reference snippet and a
//! newer `METHOD:PATH:SHA256(body):ts` formula) and has no sandbox, so the adapter tries the
//! legacy scheme first, falls back to the newer one on HTTP 401 and remembers whichever the
//! account's key accepted. IDRX callbacks are unsigned and not retried: every callback is
//! re-verified against Transaction History, and pending payments are polled by the worker.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use async_trait::async_trait;
use axum::http::HeaderMap;
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use payrouters_types::{Instructions, Method, PaymentStatus, Provider};
use serde_json::{Value, json};
use uuid::Uuid;

use super::*;
use crate::util::{hmac_sha256, sha256_hex};

pub struct Idrx;

const BASE: &str = "https://api.idrx.co";
const MIN_AMOUNT: i64 = 20_000;
const MAX_AMOUNT: i64 = 1_000_000_000;
const METHODS: [Method; 2] = [Method::VaMandiri, Method::VaBri];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Scheme {
    /// `base64url(HMAC(key, ts + METHOD + url + body))` as in IDRX's reference snippet.
    Legacy,
    /// `base64url(HMAC(key, METHOD:PATH:sha256hex(body):ts))` as in the newer API pages.
    PathHash,
}

impl Scheme {
    fn other(self) -> Scheme {
        match self {
            Scheme::Legacy => Scheme::PathHash,
            Scheme::PathHash => Scheme::Legacy,
        }
    }
}

static SCHEMES: LazyLock<Mutex<HashMap<Uuid, Scheme>>> = LazyLock::new(Default::default);

/// IDRX's snippet does `crypto.createHmac('sha256', atob(secret))`, i.e. the decoded bytes are
/// treated as a Latin-1 string that Node then encodes as UTF-8. Reproduce that byte-for-byte.
fn hmac_key(secret_b64: &str) -> Vec<u8> {
    let trimmed = secret_b64.trim();
    let decoded = STANDARD
        .decode(trimmed)
        .or_else(|_| STANDARD_NO_PAD.decode(trimmed.trim_end_matches('=')))
        .unwrap_or_else(|_| trimmed.as_bytes().to_vec());
    decoded
        .iter()
        .map(|b| *b as char)
        .collect::<String>()
        .into_bytes()
}

fn sign(
    scheme: Scheme,
    secret: &str,
    ts: &str,
    method: &str,
    path_and_query: &str,
    body: &str,
) -> String {
    let key = hmac_key(secret);
    let mac = match scheme {
        Scheme::Legacy => {
            let url = format!("{BASE}{path_and_query}");
            hmac_sha256(
                &key,
                &[
                    ts.as_bytes(),
                    method.as_bytes(),
                    url.as_bytes(),
                    body.as_bytes(),
                ],
            )
        }
        Scheme::PathHash => {
            let path = path_and_query.split('?').next().unwrap_or(path_and_query);
            let msg = format!("{method}:{path}:{}:{ts}", sha256_hex(body));
            hmac_sha256(&key, &[msg.as_bytes()])
        }
    };
    URL_SAFE_NO_PAD.encode(mac)
}

async fn signed(
    ctx: &ProviderCtx<'_>,
    method: &str,
    path_and_query: &str,
    body: Option<&Value>,
) -> Result<HttpReply, ProviderError> {
    let api_key = ctx.creds.require("api_key")?;
    let secret = ctx.creds.require("api_secret")?;
    let body_text = body.map(|b| b.to_string()).unwrap_or_default();
    let first = SCHEMES
        .lock()
        .ok()
        .and_then(|m| m.get(&ctx.account_id).copied())
        .unwrap_or(Scheme::Legacy);

    let mut last = None;
    for scheme in [first, first.other()] {
        let ts = Utc::now().timestamp_millis().to_string();
        let sig = sign(scheme, secret, &ts, method, path_and_query, &body_text);
        let url = format!("{BASE}{path_and_query}");
        let mut req = match method {
            "POST" => ctx.http.post(url),
            _ => ctx.http.get(url),
        }
        .header("idrx-api-key", api_key)
        .header("idrx-api-sig", sig)
        .header("idrx-api-ts", &ts);
        if body.is_some() {
            req = req
                .header("Content-Type", "application/json")
                .body(body_text.clone());
        }
        let reply = send(req).await?;
        match auth_failure(&reply) {
            AuthFailure::None => {
                if let Ok(mut m) = SCHEMES.lock() {
                    m.insert(ctx.account_id, scheme);
                }
                return Ok(reply);
            }
            AuthFailure::UnknownKey => {
                return Err(ProviderError::Rejected(
                    "IDRX does not recognise this API key (check the key on the IDRX dashboard)"
                        .into(),
                ));
            }
            AuthFailure::NotVerified => {
                return Err(ProviderError::Rejected(
                    "IDRX says this account is not verified: finish the onboarding and verification on the IDRX dashboard".into(),
                ));
            }
            // Possibly the other signature scheme: try it once.
            AuthFailure::BadSignature => last = Some(reply),
        }
    }
    let reply = last.expect("loop ran twice");
    Err(ProviderError::Rejected(format!(
        "IDRX rejected the request signature (HTTP {}): {}",
        reply.status,
        truncate(&reply.text, 200)
    )))
}

enum AuthFailure {
    None,
    UnknownKey,
    /// `401 User is not verified`: documented on mint, and not a signature problem.
    NotVerified,
    BadSignature,
}

/// IDRX cancels an unpaid order after 24 hours whatever `expiryPeriod` says.
const MAX_EXPIRY_MINUTES: i64 = 24 * 60;

/// IDRX answers an unknown key with HTTP 400 `API Key not found`; signature problems are 401
/// (documented) or a 4xx whose message mentions the signature/timestamp.
fn auth_failure(reply: &HttpReply) -> AuthFailure {
    let msg = str_field(&reply.json, "message")
        .unwrap_or_default()
        .to_ascii_lowercase();
    if msg.contains("api key not found") {
        return AuthFailure::UnknownKey;
    }
    if msg.contains("not verified") {
        return AuthFailure::NotVerified;
    }
    let looks_like_auth = ["signature", "unauthor", "timestamp", "invalid api"]
        .iter()
        .any(|needle| msg.contains(needle));
    if reply.status == 401 || (matches!(reply.status, 400 | 403) && looks_like_auth) {
        AuthFailure::BadSignature
    } else {
        AuthFailure::None
    }
}

fn api_error(reply: &HttpReply) -> ProviderError {
    let detail = str_field(&reply.json, "message").map(str::to_string);
    fail_by_status("IDRX", reply, detail)
}

fn channel_id(m: Method) -> Option<&'static str> {
    match m {
        Method::VaMandiri => Some("MANDIRI"),
        Method::VaBri => Some("BRI"),
        _ => None,
    }
}

/// Destination wallet and chain: per-payment `provider_options` over account defaults.
pub(crate) fn resolve_target(
    creds: &Credentials,
    opts: &Value,
) -> Result<(String, String), String> {
    let wallet = opts
        .get("destination_wallet_address")
        .and_then(Value::as_str)
        .or_else(|| creds.get("default_wallet_address"))
        .ok_or("IDRX needs provider_options.destination_wallet_address (or a default wallet on the provider account)")?
        .trim()
        .to_string();
    let chain = match opts.get("chain_id") {
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::String(s)) if !s.trim().is_empty() => s.trim().to_string(),
        _ => creds.get("default_chain_id").map(str::to_string).ok_or(
            "IDRX needs provider_options.chain_id (or a default chain id on the provider account)",
        )?,
    };
    if chain.parse::<u64>().is_err() {
        return Err(format!("invalid chain_id `{chain}`"));
    }
    Ok((wallet, chain))
}

/// IDRX picks the `merchantOrderId` itself, so a mint whose answer was lost cannot be looked up by
/// an id we know. A short tag derived from the payment id travels in `productDetails`, which IDRX
/// echoes in the transaction history, and lets `recover_direct` find the order again.
fn order_tag(payment_id: Uuid) -> String {
    format!("ref:{}", &payment_id.simple().to_string()[..16])
}

fn product_details(input: &ChargeInput<'_>) -> String {
    let tag = order_tag(input.payment_id);
    let head: String = input
        .description
        .map(|d| d.trim().chars().take(255 - tag.len() - 3).collect())
        .unwrap_or_default();
    if head.is_empty() {
        tag
    } else {
        format!("{head} ({tag})")
    }
}

/// The history record created for `tag` with the amount we asked for, if any.
fn find_tagged<'a>(records: &'a [Value], tag: &str, amount: i64) -> Option<&'a Value> {
    records.iter().find(|r| {
        str_field(r, "productDetails").is_some_and(|d| d.contains(tag))
            && r.get("toBeMinted").and_then(num) == Some(amount)
    })
}

pub(crate) fn mint_body(
    creds: &Credentials,
    input: &ChargeInput<'_>,
    method: Option<Method>,
) -> Result<Value, String> {
    let (wallet, chain) = resolve_target(creds, input.provider_options)?;
    let mut body = json!({
        "toBeMinted": input.amount.to_string(),
        "destinationWalletAddress": wallet,
        "networkChainId": chain,
        "requestType": "idrx",
        "expiryPeriod": minutes_until(input.expires_at).min(MAX_EXPIRY_MINUTES),
        "returnUrl": input.return_url.unwrap_or(input.snap_url),
    });
    body["productDetails"] = json!(product_details(input));
    if let Some(m) = method {
        body["paymentMethod"] = json!("va");
        body["channelId"] = json!(channel_id(m).ok_or("unsupported IDRX channel")?);
    }
    Ok(body)
}

fn num(v: &Value) -> Option<i64> {
    match v {
        Value::Number(n) => n.as_f64().map(|f| f.round() as i64),
        Value::String(s) => s.parse::<f64>().ok().map(|f| f.round() as i64),
        _ => None,
    }
}

pub(crate) fn map_status(payment_status: &str, user_mint_status: &str) -> PaymentStatus {
    match (payment_status, user_mint_status) {
        (_, "MINTED") => PaymentStatus::Paid,
        (_, "REFUND") | (_, "REJECTED") => PaymentStatus::Cancelled,
        ("EXPIRED", _) => PaymentStatus::Expired,
        _ => PaymentStatus::Pending,
    }
}

fn method_from_code(code: &str) -> Option<Method> {
    match code {
        "BC" => Some(Method::VaBca),
        "M2" => Some(Method::VaMandiri),
        "BT" => Some(Method::VaPermata),
        "BR" => Some(Method::VaBri),
        "IQ" => Some(Method::VaBni),
        _ => None,
    }
}

pub(crate) fn status_from_record(rec: &Value) -> StatusUpdate {
    let status = map_status(
        str_field(rec, "paymentStatus").unwrap_or(""),
        str_field(rec, "userMintStatus").unwrap_or(""),
    );
    let mut u = StatusUpdate::new(status);
    u.provider_ref = rec.get("merchantOrderId").and_then(|v| {
        v.as_str()
            .map(str::to_string)
            .or_else(|| num(v).map(|n| n.to_string()))
    });
    u.method = str_field(rec, "paymentCode").and_then(method_from_code);
    if let (Some(paid), Some(minted)) = (
        rec.get("paymentAmount").and_then(num),
        rec.get("toBeMinted").and_then(num),
    ) && paid > minted
    {
        u.fee_amount = Some(paid - minted);
    }
    if status == PaymentStatus::Paid {
        u.paid_at = str_field(rec, "updatedAt")
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|t| t.with_timezone(&Utc));
    }
    u
}

async fn history_record(
    ctx: &ProviderCtx<'_>,
    merchant_order_id: &str,
) -> Result<Option<Value>, ProviderError> {
    let order: String = url_encode(merchant_order_id);
    let path = format!(
        "/transaction/user-transaction-history?transactionType=MINT&page=1&take=1&merchantOrderId={order}"
    );
    let reply = signed(ctx, "GET", &path, None).await?;
    if !(200..300).contains(&reply.status) {
        return Err(api_error(&reply));
    }
    Ok(reply
        .json
        .get("records")
        .and_then(Value::as_array)
        .and_then(|r| r.first())
        .cloned())
}

/// One page of the account's mint history, newest or oldest first (IDRX does not document which).
async fn history_page(
    ctx: &ProviderCtx<'_>,
    page: u64,
    take: u64,
) -> Result<(Vec<Value>, u64), ProviderError> {
    let path = format!(
        "/transaction/user-transaction-history?transactionType=MINT&page={page}&take={take}"
    );
    let reply = signed(ctx, "GET", &path, None).await?;
    if !(200..300).contains(&reply.status) {
        return Err(api_error(&reply));
    }
    let records = reply
        .json
        .get("records")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let total = reply
        .json
        .get("metadata")
        .and_then(|m| m.get("totalCount"))
        .and_then(num)
        .unwrap_or(0)
        .max(0) as u64;
    Ok((records, total))
}

const HISTORY_TAKE: u64 = 50;

/// Look for the order a mint request created before its answer got lost. Checks the first page and,
/// when the history is longer, the last one, so either sort order finds a fresh order.
async fn find_order_by_tag(
    ctx: &ProviderCtx<'_>,
    input: &ChargeInput<'_>,
) -> Result<Option<Value>, ProviderError> {
    let tag = order_tag(input.payment_id);
    let (first, total) = history_page(ctx, 1, HISTORY_TAKE).await?;
    if let Some(r) = find_tagged(&first, &tag, input.amount) {
        return Ok(Some(r.clone()));
    }
    let last_page = total.div_ceil(HISTORY_TAKE);
    if last_page > 1 {
        let (last, _) = history_page(ctx, last_page, HISTORY_TAKE).await?;
        return Ok(find_tagged(&last, &tag, input.amount).cloned());
    }
    Ok(None)
}

/// The Virtual Account of an existing direct order, from IDRX's stored payload (safe to re-request).
fn direct_output_from_payload(
    data: &Value,
    method: Method,
    amount: i64,
    merchant_order_id: &str,
) -> Option<ChargeOutput> {
    let account = str_field(data, "virtualAccountNo")?;
    let total = data
        .get("paymentAmount")
        .or_else(|| data.get("amount"))
        .and_then(num)
        .filter(|t| *t > amount);
    Some(ChargeOutput {
        instructions: Instructions::VirtualAccount {
            bank: method.bank().unwrap_or_default().to_string(),
            account_number: Some(account.to_string()),
            account_name: str_field(data, "virtualAccountName").map(str::to_string),
            biller_code: None,
            bill_key: None,
        },
        provider_ref: Some(merchant_order_id.to_string()),
        provider_data: json!({ "kind": "direct_va" }),
        payable_amount: total,
    })
}

/// IDRX has no idempotency key and says not to create a new order after a 5xx: recover the
/// payload of the order that may exist. `Ok(None)` means IDRX holds no such order.
async fn recover_direct(
    ctx: &ProviderCtx<'_>,
    input: &ChargeInput<'_>,
    method: Method,
) -> Result<Option<ChargeOutput>, ProviderError> {
    let Some(record) = find_order_by_tag(ctx, input).await? else {
        return Ok(None);
    };
    let Some(order) = record.get("merchantOrderId").and_then(|v| {
        v.as_str()
            .map(str::to_string)
            .or_else(|| num(v).map(|n| n.to_string()))
    }) else {
        return Ok(None);
    };
    let path = format!(
        "/duitku-snap/checkout/payment-payload?merchantOrderId={}",
        url_encode(&order)
    );
    let reply = signed(ctx, "GET", &path, None).await?;
    if !(200..300).contains(&reply.status) {
        return Err(api_error(&reply));
    }
    let data = reply.json.get("data").unwrap_or(&reply.json);
    Ok(direct_output_from_payload(
        data,
        method,
        input.amount,
        &order,
    ))
}

fn url_encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[async_trait]
impl ProviderAdapter for Idrx {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            provider: Provider::Idrx,
            supports_sandbox: false,
            supports_provider_checkout: true,
            credential_fields: vec![
                CredentialField {
                    key: "api_key",
                    label: "API key",
                    secret: true,
                    required: true,
                    placeholder: "IDRX dashboard → API Key",
                },
                CredentialField {
                    key: "api_secret",
                    label: "API secret (base64)",
                    secret: true,
                    required: true,
                    placeholder: "shown once when the key is generated",
                },
                CredentialField {
                    key: "default_wallet_address",
                    label: "Default destination wallet",
                    secret: false,
                    required: false,
                    placeholder: "0x…",
                },
                CredentialField {
                    key: "default_chain_id",
                    label: "Default chain id",
                    secret: false,
                    required: false,
                    placeholder: "8453",
                },
            ],
            methods: METHODS.to_vec(),
        }
    }

    fn validate(
        &self,
        creds: &Credentials,
        input: &ChargeInput<'_>,
        _hosted: bool,
    ) -> Result<(), String> {
        if !(MIN_AMOUNT..=MAX_AMOUNT).contains(&input.amount) {
            return Err(format!(
                "IDRX amount must be between {MIN_AMOUNT} and {MAX_AMOUNT} IDR"
            ));
        }
        resolve_target(creds, input.provider_options).map(|_| ())
    }

    async fn create_charge(
        &self,
        ctx: &ProviderCtx<'_>,
        input: &ChargeInput<'_>,
    ) -> Result<ChargeOutput, ProviderError> {
        let method = input
            .method
            .ok_or_else(|| ProviderError::Rejected("method required".into()))?;
        let body = mint_body(ctx.creds, input, Some(method)).map_err(ProviderError::Rejected)?;
        let reply = match signed(ctx, "POST", "/transaction/mint-request", Some(&body)).await {
            Ok(r) if r.status < 500 => r,
            // Timeout, dropped connection or 5xx: the order may exist at IDRX. Recover it
            // instead of letting the caller create a second one.
            outcome => {
                let err = match outcome {
                    Ok(r) => api_error(&r),
                    Err(e) => e,
                };
                if matches!(err, ProviderError::Unavailable(_))
                    && let Ok(Some(out)) = recover_direct(ctx, input, method).await
                {
                    return Ok(out);
                }
                return Err(err);
            }
        };
        if !(200..300).contains(&reply.status) {
            return Err(api_error(&reply));
        }
        let data = reply.json.get("data").unwrap_or(&Value::Null);
        let account = str_field(data, "virtualAccountNo").ok_or_else(|| {
            ProviderError::Unavailable("IDRX response lacked virtualAccountNo".into())
        })?;
        let total = data
            .get("amount")
            .and_then(num)
            .filter(|t| *t > input.amount);
        Ok(ChargeOutput {
            instructions: Instructions::VirtualAccount {
                bank: method.bank().unwrap_or_default().to_string(),
                account_number: Some(account.to_string()),
                account_name: str_field(data, "virtualAccountName").map(str::to_string),
                biller_code: None,
                bill_key: None,
            },
            provider_ref: data
                .get("merchantOrderId")
                .and_then(|v| v.as_str().map(str::to_string)),
            provider_data: json!({ "kind": "direct_va" }),
            payable_amount: total,
        })
    }

    async fn create_hosted(
        &self,
        ctx: &ProviderCtx<'_>,
        input: &ChargeInput<'_>,
    ) -> Result<ChargeOutput, ProviderError> {
        let body = mint_body(ctx.creds, input, None).map_err(ProviderError::Rejected)?;
        let reply = signed(ctx, "POST", "/transaction/mint-request", Some(&body)).await?;
        if !(200..300).contains(&reply.status) {
            return Err(api_error(&reply));
        }
        let data = reply.json.get("data").unwrap_or(&Value::Null);
        let url = str_field(data, "paymentUrl")
            .or_else(|| str_field(data, "checkoutUrl"))
            .ok_or_else(|| ProviderError::Unavailable("IDRX response lacked paymentUrl".into()))?;
        Ok(ChargeOutput {
            instructions: Instructions::Redirect {
                url: url.to_string(),
            },
            provider_ref: data
                .get("merchantOrderId")
                .and_then(|v| v.as_str().map(str::to_string)),
            provider_data: json!({ "kind": "hosted" }),
            payable_amount: None,
        })
    }

    async fn fetch_status(
        &self,
        ctx: &ProviderCtx<'_>,
        payment: &PaymentRef<'_>,
    ) -> Result<Option<StatusUpdate>, ProviderError> {
        let Some(order) = payment.provider_ref else {
            return Ok(None);
        };
        Ok(history_record(ctx, order)
            .await?
            .map(|r| status_from_record(&r)))
    }

    async fn parse_webhook(
        &self,
        ctx: &ProviderCtx<'_>,
        _headers: &HeaderMap,
        body: &[u8],
    ) -> Result<ParsedWebhook, WebhookError> {
        let v: Value = serde_json::from_slice(body)
            .map_err(|e| WebhookError::Malformed(format!("body is not JSON: {e}")))?;
        let is_mint = v.get("paymentStatus").is_some() || v.get("userMintStatus").is_some();
        let order = v.get("merchantOrderId").and_then(|o| {
            o.as_str()
                .map(str::to_string)
                .or_else(|| num(o).map(|n| n.to_string()))
        });
        let (true, Some(order)) = (is_mint, order) else {
            // Redeem/bridge callbacks and anything else: not a cash-in.
            return Ok(ParsedWebhook {
                lookup: Lookup::Ignore,
                update: None,
            });
        };
        // The callback is unsigned: trust only what IDRX's authenticated API says.
        match history_record(ctx, &order).await {
            Ok(Some(rec)) => Ok(ParsedWebhook {
                lookup: Lookup::ProviderRef(order),
                update: Some(status_from_record(&rec)),
            }),
            Ok(None) => Err(WebhookError::Unauthorized(
                "merchantOrderId not found in IDRX transaction history".into(),
            )),
            Err(e) => Err(WebhookError::Upstream(format!(
                "could not verify with IDRX: {e}"
            ))),
        }
    }

    async fn test_connection(&self, ctx: &ProviderCtx<'_>) -> Result<String, ProviderError> {
        // `/transaction/method` is public, so it would "succeed" with any key; history is authenticated.
        let path = "/transaction/user-transaction-history?transactionType=MINT&page=1&take=1";
        let reply = signed(ctx, "GET", path, None).await?;
        if (200..300).contains(&reply.status) {
            Ok("Authenticated against IDRX".to_string())
        } else {
            Err(api_error(&reply))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn creds(pairs: &[(&str, &str)]) -> Credentials {
        Credentials(
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect::<BTreeMap<_, _>>(),
        )
    }

    #[test]
    fn latin1_key_matches_node_semantics() {
        // atob("/w==") is the single byte 0xFF; Node's string key encodes it as UTF-8 (C3 BF).
        assert_eq!(hmac_key("/w=="), vec![0xC3, 0xBF]);
        assert_eq!(hmac_key("YWJj"), b"abc".to_vec());
    }

    #[test]
    fn signatures_differ_per_scheme_and_are_deterministic() {
        let a = sign(
            Scheme::Legacy,
            "YWJj",
            "1700000000000",
            "POST",
            "/transaction/mint-request",
            "{}",
        );
        let b = sign(
            Scheme::PathHash,
            "YWJj",
            "1700000000000",
            "POST",
            "/transaction/mint-request",
            "{}",
        );
        assert_ne!(a, b);
        assert_eq!(
            a,
            sign(
                Scheme::Legacy,
                "YWJj",
                "1700000000000",
                "POST",
                "/transaction/mint-request",
                "{}"
            )
        );
        assert!(!a.contains('=') && !a.contains('+') && !a.contains('/'));
    }

    #[test]
    fn target_resolution() {
        let c = creds(&[
            ("default_wallet_address", "0xabc"),
            ("default_chain_id", "8453"),
        ]);
        assert_eq!(
            resolve_target(&c, &json!({})).unwrap(),
            ("0xabc".into(), "8453".into())
        );
        assert_eq!(
            resolve_target(
                &c,
                &json!({ "destination_wallet_address": "0xdef", "chain_id": 137 })
            )
            .unwrap(),
            ("0xdef".into(), "137".into())
        );
        assert!(resolve_target(&creds(&[]), &json!({})).is_err());
        assert!(resolve_target(&c, &json!({ "chain_id": "abc" })).is_err());
    }

    #[test]
    fn status_mapping() {
        assert_eq!(map_status("PAID", "MINTED"), PaymentStatus::Paid);
        assert_eq!(map_status("PAID", "PROCESSING"), PaymentStatus::Pending);
        assert_eq!(
            map_status("EXPIRED", "NOT_AVAILABLE"),
            PaymentStatus::Expired
        );
        assert_eq!(map_status("PAID", "REFUND"), PaymentStatus::Cancelled);
        assert_eq!(
            map_status("WAITING_FOR_PAYMENT", "NOT_AVAILABLE"),
            PaymentStatus::Pending
        );
    }

    #[test]
    fn record_to_update() {
        let rec = json!({
            "merchantOrderId": "2026112610",
            "paymentStatus": "PAID", "userMintStatus": "MINTED",
            "paymentAmount": 54500, "toBeMinted": "50000",
            "paymentCode": "BR", "updatedAt": "2026-11-26T10:00:00.000Z"
        });
        let u = status_from_record(&rec);
        assert_eq!(u.status, PaymentStatus::Paid);
        assert_eq!(u.fee_amount, Some(4500));
        assert_eq!(u.method, Some(Method::VaBri));
        assert_eq!(u.provider_ref.as_deref(), Some("2026112610"));
        assert!(u.paid_at.is_some());
    }

    #[test]
    fn classifies_auth_failures() {
        let reply = |status: u16, msg: &str| HttpReply {
            status,
            json: json!({ "statusCode": status, "message": msg }),
            text: msg.to_string(),
        };
        assert!(matches!(
            auth_failure(&reply(400, "API Key not found")),
            AuthFailure::UnknownKey
        ));
        assert!(matches!(
            auth_failure(&reply(401, "Unauthorized")),
            AuthFailure::BadSignature
        ));
        assert!(matches!(
            auth_failure(&reply(401, "User is not verified")),
            AuthFailure::NotVerified
        ));
        assert!(matches!(
            auth_failure(&reply(400, "Invalid signature")),
            AuthFailure::BadSignature
        ));
        assert!(matches!(
            auth_failure(&reply(400, "invalid toBeMinted: too small")),
            AuthFailure::None
        ));
        assert!(matches!(
            auth_failure(&reply(200, "success")),
            AuthFailure::None
        ));
    }

    #[test]
    fn url_encoding() {
        assert_eq!(url_encode("a b/c"), "a%20b%2Fc");
        assert_eq!(url_encode("20260508154603"), "20260508154603");
    }

    #[test]
    fn expiry_is_capped_at_the_24_hours_idrx_enforces() {
        let c = creds(&[
            (
                "default_wallet_address",
                "0x1111111111111111111111111111111111111111",
            ),
            ("default_chain_id", "137"),
        ]);
        let opts = json!({});
        let input = ChargeInput {
            payment_id: uuid::Uuid::nil(),
            order_id: "INV-1",
            amount: 50_000,
            expires_at: Utc::now() + chrono::Duration::days(10),
            description: None,
            customer: None,
            items: &[],
            return_url: None,
            snap_url: "https://pay.example.com/snap/t",
            snap_api_url: "https://pay.example.com/snap/api/t",
            provider_options: &opts,
            method: None,
        };
        let body = mint_body(&c, &input, None).unwrap();
        assert_eq!(body["expiryPeriod"], 24 * 60);
    }

    #[test]
    fn a_mint_whose_answer_was_lost_can_be_found_again_by_its_tag() {
        let opts = json!({});
        let id = Uuid::parse_str("01a21bf7-f298-71c0-9bd1-95eb988e4ba2").unwrap();
        let mut input = ChargeInput {
            payment_id: id,
            order_id: "INV-1",
            amount: 50_000,
            expires_at: Utc::now() + chrono::Duration::hours(1),
            description: Some("Order INV-1"),
            customer: None,
            items: &[],
            return_url: None,
            snap_url: "https://pay.example.com/snap/t",
            snap_api_url: "https://pay.example.com/snap/api/t",
            provider_options: &opts,
            method: None,
        };
        let details = product_details(&input);
        assert_eq!(details, "Order INV-1 (ref:01a21bf7f29871c0)");
        input.description = None;
        assert_eq!(product_details(&input), "ref:01a21bf7f29871c0");
        let long = "x".repeat(400);
        input.description = Some(&long);
        assert!(product_details(&input).chars().count() <= 255);
        assert!(product_details(&input).ends_with("(ref:01a21bf7f29871c0)"));

        let tag = order_tag(id);
        let records = vec![
            json!({ "merchantOrderId": "1", "productDetails": "Order INV-9 (ref:ffffffffffffffff)", "toBeMinted": "50000" }),
            json!({ "merchantOrderId": "2", "productDetails": details, "toBeMinted": "49000" }),
            json!({ "merchantOrderId": "3", "productDetails": details, "toBeMinted": "50000" }),
        ];
        // same tag but another amount is not our order
        let found = find_tagged(&records, &tag, 50_000).unwrap();
        assert_eq!(found["merchantOrderId"], "3");
        assert!(find_tagged(&records, &tag, 10_000).is_none());
        assert!(find_tagged(&records[..1], &tag, 50_000).is_none());
    }

    #[test]
    fn the_stored_payload_rebuilds_the_virtual_account_instructions() {
        let data = json!({
            "merchantOrderId": "20260728130000",
            "virtualAccountNo": "8680770000001234",
            "virtualAccountName": "JOHN SMITH",
            "paymentAmount": 54000,
            "toBeMinted": "50000",
            "paymentStatus": "WAITING_FOR_PAYMENT"
        });
        let out =
            direct_output_from_payload(&data, Method::VaMandiri, 50_000, "20260728130000").unwrap();
        assert_eq!(out.provider_ref.as_deref(), Some("20260728130000"));
        assert_eq!(out.payable_amount, Some(54_000));
        assert_eq!(
            out.instructions,
            Instructions::VirtualAccount {
                bank: "mandiri".into(),
                account_number: Some("8680770000001234".into()),
                account_name: Some("JOHN SMITH".into()),
                biller_code: None,
                bill_key: None,
            }
        );
        assert!(direct_output_from_payload(&json!({}), Method::VaBri, 50_000, "1").is_none());
    }
}
