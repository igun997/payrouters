//! Built-in simulator. Needs no credentials, never calls out, and only changes state when the
//! dashboard/snap simulator says so (which goes through the same status pipeline as a real
//! webhook). Sandbox mode only.

use async_trait::async_trait;
use axum::http::HeaderMap;
use payrouters_types::{Instructions, Method, Provider};
use serde_json::json;

use super::*;

pub struct Mock;

const METHODS: [Method; 6] = [
    Method::VaBca,
    Method::VaBni,
    Method::VaBri,
    Method::VaMandiri,
    Method::VaPermata,
    Method::Qris,
];

pub(crate) fn fake_instructions(method: Method, id: Uuid) -> Instructions {
    let digits: String = id.as_u128().to_string().chars().rev().take(12).collect();
    match method {
        Method::Qris => Instructions::Qris {
            qr_string: format!("00020101021226MOCK{}5303360", id.simple()),
            qr_image_url: None,
        },
        Method::VaMandiri => Instructions::VirtualAccount {
            bank: "mandiri".into(),
            account_number: None,
            account_name: None,
            biller_code: Some("70012".into()),
            bill_key: Some(digits),
        },
        m => Instructions::VirtualAccount {
            bank: m.bank().unwrap_or_default().to_string(),
            account_number: Some(format!("8{digits}")),
            account_name: Some("PAYROUTERS MOCK".into()),
            biller_code: None,
            bill_key: None,
        },
    }
}

#[async_trait]
impl ProviderAdapter for Mock {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            provider: Provider::Mock,
            supports_sandbox: true,
            supports_provider_checkout: false,
            credential_fields: vec![],
            methods: METHODS.to_vec(),
        }
    }

    async fn create_charge(
        &self,
        _ctx: &ProviderCtx<'_>,
        input: &ChargeInput<'_>,
    ) -> Result<ChargeOutput, ProviderError> {
        let method = input
            .method
            .ok_or_else(|| ProviderError::Rejected("method required".into()))?;
        Ok(ChargeOutput {
            instructions: fake_instructions(method, input.payment_id),
            provider_ref: Some(format!("mock_{}", input.payment_id.simple())),
            provider_data: json!({ "kind": "mock" }),
            payable_amount: None,
            expires_at: None,
        })
    }

    async fn create_hosted(
        &self,
        _ctx: &ProviderCtx<'_>,
        _input: &ChargeInput<'_>,
    ) -> Result<ChargeOutput, ProviderError> {
        Err(ProviderError::Rejected(
            "mock provider has no hosted checkout".into(),
        ))
    }

    async fn fetch_status(
        &self,
        _ctx: &ProviderCtx<'_>,
        _payment: &PaymentRef<'_>,
    ) -> Result<Option<StatusUpdate>, ProviderError> {
        Ok(None)
    }

    async fn parse_webhook(
        &self,
        _ctx: &ProviderCtx<'_>,
        _headers: &HeaderMap,
        _body: &[u8],
    ) -> Result<ParsedWebhook, WebhookError> {
        Err(WebhookError::Unauthorized(
            "mock provider does not receive webhooks; use the simulator".into(),
        ))
    }

    async fn test_connection(&self, _ctx: &ProviderCtx<'_>) -> Result<String, ProviderError> {
        Ok("Mock provider is always available".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fake_instructions_by_method() {
        let id = Uuid::now_v7();
        assert!(matches!(
            fake_instructions(Method::Qris, id),
            Instructions::Qris { .. }
        ));
        match fake_instructions(Method::VaBca, id) {
            Instructions::VirtualAccount {
                bank,
                account_number: Some(n),
                ..
            } => {
                assert_eq!(bank, "bca");
                assert!(n.starts_with('8') && n.len() >= 10);
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            fake_instructions(Method::VaMandiri, id),
            Instructions::VirtualAccount {
                account_number: None,
                bill_key: Some(_),
                ..
            }
        ));
    }
}
