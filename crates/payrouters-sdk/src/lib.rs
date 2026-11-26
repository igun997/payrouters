//! Rust SDK for the PayRouters product API and webhook verification.
//!
//! ```no_run
//! # async fn demo() -> Result<(), payrouters_sdk::Error> {
//! use payrouters_sdk::{Client, types::CreatePaymentRequest};
//!
//! let client = Client::new("https://pay.example.com", "prk_test_...")?;
//! let created = client
//!     .create_payment(&CreatePaymentRequest {
//!         order_id: "INV-1".into(),
//!         amount: 150_000,
//!         ..Default::default()
//!     })
//!     .await?;
//! println!("send the customer to {}", created.payment.snap_url);
//! # Ok(()) }
//! ```

use std::time::Duration;

use payrouters_types::{CreatePaymentRequest, ListResponse, MethodCatalog, Payment, PaymentStatus};
use reqwest::{Method as HttpMethod, StatusCode};
use serde::Serialize;
use serde::de::DeserializeOwned;

pub use payrouters_types as types;
pub mod webhook;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The server answered with the standard error envelope.
    #[error("PayRouters API error {status} ({code}): {message}")]
    Api {
        status: u16,
        code: String,
        message: String,
        details: Option<serde_json::Value>,
    },
    #[error("transport error: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("unexpected response (HTTP {status}): {body}")]
    Decode { status: u16, body: String },
}

/// Result of `create_payment`.
#[derive(Debug, Clone)]
pub struct CreatedPayment {
    pub payment: Payment,
    /// `true` when an identical pending payment already existed for the `order_id`.
    pub replayed: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ListParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<PaymentStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
}

impl Client {
    /// `base_url` is your PayRouters origin, e.g. `https://pay.example.com`.
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>) -> Result<Client, Error> {
        let http = reqwest::Client::builder()
            .user_agent(concat!("payrouters-rust/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(15))
            .build()?;
        Ok(Client::with_http(http, base_url, api_key))
    }

    pub fn with_http(
        http: reqwest::Client,
        base_url: impl Into<String>,
        api_key: impl Into<String>,
    ) -> Client {
        Client {
            http,
            base_url: base_url.into().trim_end_matches('/').to_string(),
            api_key: api_key.into(),
        }
    }

    async fn send(
        &self,
        method: HttpMethod,
        path: &str,
        query: Option<&impl Serialize>,
        body: Option<&impl Serialize>,
    ) -> Result<(StatusCode, reqwest::header::HeaderMap, String), Error> {
        let mut req = self
            .http
            .request(method, format!("{}{path}", self.base_url))
            .bearer_auth(&self.api_key);
        if let Some(q) = query {
            req = req.query(q);
        }
        if let Some(b) = body {
            req = req.json(b);
        }
        let resp = req.send().await?;
        let status = resp.status();
        let headers = resp.headers().clone();
        let text = resp.text().await?;
        if status.is_success() {
            return Ok((status, headers, text));
        }
        match serde_json::from_str::<payrouters_types::ApiErrorBody>(&text) {
            Ok(env) => Err(Error::Api {
                status: status.as_u16(),
                code: env.error.code,
                message: env.error.message,
                details: env.error.details,
            }),
            Err(_) => Err(Error::Decode {
                status: status.as_u16(),
                body: text,
            }),
        }
    }

    async fn call<T: DeserializeOwned>(
        &self,
        method: HttpMethod,
        path: &str,
        query: Option<&impl Serialize>,
        body: Option<&impl Serialize>,
    ) -> Result<(T, reqwest::header::HeaderMap, StatusCode), Error> {
        let (status, headers, text) = self.send(method, path, query, body).await?;
        let value = serde_json::from_str(&text).map_err(|_| Error::Decode {
            status: status.as_u16(),
            body: text,
        })?;
        Ok((value, headers, status))
    }

    /// Create a payment. Safe to retry with the same `order_id`: an identical pending payment is
    /// returned with `replayed = true` instead of creating a second one.
    pub async fn create_payment(
        &self,
        req: &CreatePaymentRequest,
    ) -> Result<CreatedPayment, Error> {
        let (payment, headers, status) = self
            .call::<Payment>(HttpMethod::POST, "/v1/payments", None::<&()>, Some(req))
            .await?;
        let replayed = status == StatusCode::OK
            || headers
                .get("PayRouters-Idempotent-Replay")
                .is_some_and(|v| v == "true");
        Ok(CreatedPayment { payment, replayed })
    }

    pub async fn get_payment(&self, id: uuid::Uuid) -> Result<Payment, Error> {
        Ok(self
            .call(
                HttpMethod::GET,
                &format!("/v1/payments/{id}"),
                None::<&()>,
                None::<&()>,
            )
            .await?
            .0)
    }

    pub async fn sync_payment(&self, id: uuid::Uuid) -> Result<Payment, Error> {
        Ok(self
            .call(
                HttpMethod::POST,
                &format!("/v1/payments/{id}/sync"),
                None::<&()>,
                None::<&()>,
            )
            .await?
            .0)
    }

    pub async fn list_payments(&self, params: &ListParams) -> Result<ListResponse<Payment>, Error> {
        Ok(self
            .call(HttpMethod::GET, "/v1/payments", Some(params), None::<&()>)
            .await?
            .0)
    }

    pub async fn methods(&self) -> Result<MethodCatalog, Error> {
        Ok(self
            .call(HttpMethod::GET, "/v1/methods", None::<&()>, None::<&()>)
            .await?
            .0)
    }
}
