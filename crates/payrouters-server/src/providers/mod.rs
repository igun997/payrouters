//! Provider adapters. Each adapter maps the standardized PayRouters model onto one
//! payment gateway's API and maps its statuses/webhooks back.

use std::collections::BTreeMap;

use async_trait::async_trait;
use axum::http::HeaderMap;
use chrono::{DateTime, Utc};
use payrouters_types::{Customer, Instructions, Item, Method, Mode, PaymentStatus, Provider};
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

pub mod availability;
pub mod fx;
pub mod idrx;
pub mod mayar;
pub mod midtrans;
pub mod mock;
pub mod paypal;
pub mod xendit;

#[derive(Debug, Clone, Default)]
pub struct Credentials(pub BTreeMap<String, String>);

impl Credentials {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.0
            .get(key)
            .map(String::as_str)
            .filter(|v| !v.is_empty())
    }

    pub fn require(&self, key: &str) -> Result<&str, ProviderError> {
        self.get(key)
            .ok_or_else(|| ProviderError::Config(format!("credential `{key}` is not set")))
    }
}

pub struct ProviderCtx<'a> {
    pub http: &'a reqwest::Client,
    pub account_id: Uuid,
    pub mode: Mode,
    pub creds: &'a Credentials,
    pub public_base_url: &'a str,
}

#[derive(Debug, Clone, Serialize)]
pub struct CredentialField {
    pub key: &'static str,
    pub label: &'static str,
    pub secret: bool,
    pub required: bool,
    pub placeholder: &'static str,
}

#[derive(Debug, Clone)]
pub struct ProviderInfo {
    pub provider: Provider,
    pub supports_sandbox: bool,
    pub supports_provider_checkout: bool,
    pub credential_fields: Vec<CredentialField>,
    pub methods: Vec<Method>,
}

#[derive(Clone, Copy)]
pub struct ChargeInput<'a> {
    pub payment_id: Uuid,
    pub order_id: &'a str,
    pub amount: i64,
    pub expires_at: DateTime<Utc>,
    pub description: Option<&'a str>,
    pub customer: Option<&'a Customer>,
    pub items: &'a [Item],
    pub return_url: Option<&'a str>,
    pub snap_url: &'a str,
    /// The payment's `/snap/api/{token}` base. Providers whose payer comes back after approving
    /// (PayPal) send them to `{snap_api_url}/return` or `{snap_api_url}/cancel`.
    pub snap_api_url: &'a str,
    pub provider_options: &'a Value,
    pub method: Option<Method>,
}

pub struct ChargeOutput {
    pub instructions: Instructions,
    pub provider_ref: Option<String>,
    pub provider_data: Value,
    /// Exact total to pay when the provider adds its fee on top of `amount`.
    pub payable_amount: Option<i64>,
}

/// What the customer is really charged when the provider cannot take rupiah (PayPal): the amount in
/// the charge currency and the rate it was converted at.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Quote {
    pub currency: String,
    /// Decimal string as the provider is sent it, e.g. `"7.02"`.
    pub value: String,
    /// Rupiah per one unit of `currency`.
    pub idr_per_unit: f64,
    /// `fixed` (set on the account) or `ecb` (the European Central Bank reference rate).
    pub source: String,
    /// Date of the reference rate when `source` is `ecb`.
    pub as_of: Option<String>,
}

impl Quote {
    /// The charge a payment stored with its provider data (`charge`, `idr_rate`, `rate_source`,
    /// `rate_date`), if the provider converted its amount.
    pub fn from_provider_data(data: &Value) -> Option<Quote> {
        let charge = data.get("charge")?;
        Some(Quote {
            currency: charge.get("currency")?.as_str()?.to_string(),
            value: charge.get("value")?.as_str()?.to_string(),
            idr_per_unit: data.get("idr_rate")?.as_f64()?,
            source: data.get("rate_source")?.as_str()?.to_string(),
            as_of: data
                .get("rate_date")
                .and_then(Value::as_str)
                .map(str::to_string),
        })
    }
}

/// What a provider told us about a payment (webhook or status poll).
#[derive(Debug, Clone)]
pub struct StatusUpdate {
    pub status: PaymentStatus,
    pub method: Option<Method>,
    pub fee_amount: Option<i64>,
    pub paid_at: Option<DateTime<Utc>>,
    pub provider_ref: Option<String>,
    /// Amount the provider says was charged; a mismatch with our record rejects a `paid` update.
    pub amount: Option<i64>,
}

impl StatusUpdate {
    pub fn new(status: PaymentStatus) -> StatusUpdate {
        StatusUpdate {
            status,
            method: None,
            fee_amount: None,
            paid_at: None,
            provider_ref: None,
            amount: None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Lookup {
    PaymentId(Uuid),
    ProviderRef(String),
    /// Authentic but irrelevant event (e.g. redeem callbacks).
    Ignore,
}

#[derive(Debug, Clone)]
pub struct ParsedWebhook {
    pub lookup: Lookup,
    /// `None` = authentic event that carries no status change.
    pub update: Option<StatusUpdate>,
}

#[derive(Debug, thiserror::Error)]
pub enum WebhookError {
    #[error("{0}")]
    Unauthorized(String),
    #[error("{0}")]
    Malformed(String),
    /// Verification needed a provider call that failed; the provider should retry.
    #[error("{0}")]
    Upstream(String),
}

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    /// The provider refused the request (validation, bad credentials, ...).
    #[error("{0}")]
    Rejected(String),
    /// Transport failure or provider 5xx.
    #[error("{0}")]
    Unavailable(String),
    /// The provider says this payment channel (or the API behind it) is not activated on the
    /// account. Remembered by [`availability`], and the payment falls back to the provider's
    /// hosted checkout where there is one.
    #[error("{0}")]
    ChannelInactive(String),
    #[error("{0}")]
    Config(String),
}

pub struct PaymentRef<'a> {
    pub id: Uuid,
    pub provider_ref: Option<&'a str>,
    pub provider_data: &'a Value,
}

#[async_trait]
pub trait ProviderAdapter: Send + Sync {
    fn info(&self) -> ProviderInfo;

    /// Provider-specific input rules, checked before anything is persisted.
    fn validate(
        &self,
        _creds: &Credentials,
        _input: &ChargeInput<'_>,
        _hosted: bool,
    ) -> Result<(), String> {
        Ok(())
    }

    /// Core API charge for a standard method (PayRouters Snap flow).
    async fn create_charge(
        &self,
        ctx: &ProviderCtx<'_>,
        input: &ChargeInput<'_>,
    ) -> Result<ChargeOutput, ProviderError>;

    /// Provider-hosted checkout page.
    async fn create_hosted(
        &self,
        ctx: &ProviderCtx<'_>,
        input: &ChargeInput<'_>,
    ) -> Result<ChargeOutput, ProviderError>;

    /// Poll the provider. `None` = provider has no record yet.
    async fn fetch_status(
        &self,
        ctx: &ProviderCtx<'_>,
        payment: &PaymentRef<'_>,
    ) -> Result<Option<StatusUpdate>, ProviderError>;

    async fn parse_webhook(
        &self,
        ctx: &ProviderCtx<'_>,
        headers: &HeaderMap,
        body: &[u8],
    ) -> Result<ParsedWebhook, WebhookError>;

    /// What the customer would be charged for `amount` rupiah when this provider converts it to
    /// another currency; `None` when it charges rupiah.
    async fn quote(
        &self,
        _ctx: &ProviderCtx<'_>,
        _amount: i64,
    ) -> Result<Option<Quote>, ProviderError> {
        Ok(None)
    }

    /// Cheap authenticated call proving the credentials work.
    async fn test_connection(&self, ctx: &ProviderCtx<'_>) -> Result<String, ProviderError>;
}

pub fn adapter(provider: Provider) -> &'static dyn ProviderAdapter {
    match provider {
        Provider::Midtrans => &midtrans::Midtrans,
        Provider::Xendit => &xendit::Xendit,
        Provider::Idrx => &idrx::Idrx,
        Provider::Mayar => &mayar::Mayar,
        Provider::Paypal => &paypal::Paypal,
        Provider::Mock => &mock::Mock,
    }
}

/// Default preference when neither the request nor the product names a provider.
pub const PROVIDER_ORDER: [Provider; 6] = [
    Provider::Midtrans,
    Provider::Xendit,
    Provider::Idrx,
    Provider::Mayar,
    Provider::Paypal,
    Provider::Mock,
];

/// Response of an HTTP call to a provider.
pub struct HttpReply {
    pub status: u16,
    pub json: Value,
    pub text: String,
}

pub async fn send(req: reqwest::RequestBuilder) -> Result<HttpReply, ProviderError> {
    let resp = req.send().await.map_err(|e| {
        ProviderError::Unavailable(format!("provider unreachable: {}", redact_err(&e)))
    })?;
    let status = resp.status().as_u16();
    let text = resp.text().await.map_err(|e| {
        ProviderError::Unavailable(format!("provider response unreadable: {}", redact_err(&e)))
    })?;
    let json = serde_json::from_str(&text).unwrap_or(Value::Null);
    Ok(HttpReply { status, json, text })
}

fn redact_err(e: &reqwest::Error) -> String {
    let mut msg = e.to_string();
    if msg.len() > 200 {
        msg.truncate(200);
    }
    msg
}

pub fn fail_by_status(provider: &str, reply: &HttpReply, detail: Option<String>) -> ProviderError {
    let detail = detail
        .filter(|d| !d.is_empty())
        .unwrap_or_else(|| truncate(&reply.text, 300));
    let msg = format!("{provider} responded {}: {detail}", reply.status);
    if reply.status >= 500 || reply.status == 429 {
        ProviderError::Unavailable(msg)
    } else {
        ProviderError::Rejected(msg)
    }
}

pub fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max).collect::<String>() + "…"
    }
}

pub fn str_field<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}

/// Whole minutes until `expires_at`, at least 1.
pub fn minutes_until(expires_at: DateTime<Utc>) -> i64 {
    let secs = (expires_at - Utc::now()).num_seconds().max(60);
    (secs + 59) / 60
}
