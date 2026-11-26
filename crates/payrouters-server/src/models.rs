//! Row types (sqlx) and their conversion to the wire types.

use chrono::{DateTime, Utc};
use payrouters_types::{
    Checkout, Customer, Instructions, Item, Method, Mode, Payment, PaymentStatus, Provider,
};
use serde::Serialize;
use sqlx::FromRow;
use sqlx::types::Json;
use uuid::Uuid;

use crate::error::AppError;

fn parse<T: std::str::FromStr<Err = String>>(s: &str) -> Result<T, AppError> {
    s.parse::<T>()
        .map_err(|e| AppError::Internal(anyhow::anyhow!("corrupt row: {e}")))
}

#[derive(Debug, Clone, FromRow)]
pub struct PaymentRow {
    pub id: Uuid,
    pub product_id: Uuid,
    pub provider_account_id: Uuid,
    pub order_id: String,
    pub mode: String,
    pub provider: String,
    pub status: String,
    pub amount: i64,
    pub currency: String,
    pub fee_amount: Option<i64>,
    pub net_amount: Option<i64>,
    pub payable_amount: Option<i64>,
    pub method: Option<String>,
    pub checkout: String,
    pub snap_token: String,
    pub instructions: Option<Json<Instructions>>,
    pub description: Option<String>,
    pub customer: Option<Json<Customer>>,
    pub items: Json<Vec<Item>>,
    pub metadata: Json<serde_json::Value>,
    pub return_url: Option<String>,
    pub provider_options: Json<serde_json::Value>,
    pub provider_ref: Option<String>,
    pub provider_data: Json<serde_json::Value>,
    pub expires_at: DateTime<Utc>,
    pub paid_at: Option<DateTime<Utc>>,
    pub last_synced_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Only present on admin queries that join `products`.
    #[sqlx(default)]
    pub product_name: Option<String>,
}

impl PaymentRow {
    pub fn status(&self) -> Result<PaymentStatus, AppError> {
        parse(&self.status)
    }

    pub fn mode(&self) -> Result<Mode, AppError> {
        parse(&self.mode)
    }

    pub fn provider(&self) -> Result<Provider, AppError> {
        parse(&self.provider)
    }

    pub fn method(&self) -> Result<Option<Method>, AppError> {
        self.method.as_deref().map(parse::<Method>).transpose()
    }

    pub fn checkout(&self) -> Result<Checkout, AppError> {
        parse(&self.checkout)
    }

    pub fn view(&self, public_base_url: &str) -> Result<Payment, AppError> {
        let mode = self.mode()?;
        Ok(Payment {
            id: self.id,
            order_id: self.order_id.clone(),
            status: self.status()?,
            mode,
            livemode: mode.livemode(),
            amount: self.amount,
            currency: self.currency.clone(),
            fee_amount: self.fee_amount,
            net_amount: self.net_amount,
            payable_amount: self.payable_amount,
            provider: self.provider()?,
            method: self.method()?,
            checkout: self.checkout()?,
            snap_url: format!("{public_base_url}/snap/{}", self.snap_token),
            instructions: self.instructions.as_ref().map(|j| j.0.clone()),
            description: self.description.clone(),
            customer: self.customer.as_ref().map(|j| j.0.clone()),
            items: self.items.0.clone(),
            metadata: self.metadata.0.clone(),
            return_url: self.return_url.clone(),
            provider_ref: self.provider_ref.clone(),
            expires_at: self.expires_at,
            paid_at: self.paid_at,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct ProductRow {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub webhook_url: Option<String>,
    pub webhook_secret_enc: String,
    pub default_provider: Option<String>,
    pub archived_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
pub struct ApiKeyRow {
    pub id: Uuid,
    pub product_id: Uuid,
    pub mode: String,
    pub name: String,
    pub prefix: String,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApiKeyView {
    pub id: Uuid,
    pub mode: String,
    pub name: String,
    pub prefix: String,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
}

impl From<ApiKeyRow> for ApiKeyView {
    fn from(r: ApiKeyRow) -> Self {
        ApiKeyView {
            id: r.id,
            mode: r.mode,
            name: r.name,
            prefix: r.prefix,
            created_at: r.created_at,
            last_used_at: r.last_used_at,
            revoked_at: r.revoked_at,
        }
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct AccountRow {
    pub id: Uuid,
    pub provider: String,
    pub mode: String,
    pub label: String,
    pub enabled: bool,
    pub credentials_enc: String,
    pub fees: Json<payrouters_types::FeeSchedule>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct DeliveryRow {
    pub id: Uuid,
    pub payment_id: Uuid,
    pub product_id: Uuid,
    pub event_id: String,
    pub event_type: String,
    pub payload: serde_json::Value,
    pub status: String,
    pub attempts: i32,
    pub next_attempt_at: Option<DateTime<Utc>>,
    pub last_status_code: Option<i32>,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub delivered_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, FromRow)]
pub struct EventRow {
    pub id: Uuid,
    pub provider_account_id: Uuid,
    pub provider: String,
    pub payment_id: Option<Uuid>,
    pub verified: bool,
    pub outcome: String,
    pub error: Option<String>,
    pub headers: serde_json::Value,
    pub body_text: String,
    pub received_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
pub struct CashInRow {
    pub id: Uuid,
    pub payment_id: Uuid,
    pub product_id: Uuid,
    pub provider_account_id: Uuid,
    pub provider: String,
    pub mode: String,
    pub method: Option<String>,
    pub currency: String,
    pub gross_amount: i64,
    pub fee_amount: i64,
    pub net_amount: i64,
    pub paid_at: DateTime<Utc>,
    pub provider_ref: Option<String>,
    pub created_at: DateTime<Utc>,
    #[sqlx(default)]
    pub order_id: Option<String>,
    #[sqlx(default)]
    pub product_name: Option<String>,
}
