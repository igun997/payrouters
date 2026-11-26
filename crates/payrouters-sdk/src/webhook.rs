//! Verify and parse PayRouters webhooks.
//!
//! The `PayRouters-Signature` header is `t=<unix seconds>,v1=<hex>[,v1=<hex>...]` where
//! `v1 = hex(HMAC_SHA256(webhook_secret, "<t>.<raw body>"))`. Always verify against the raw
//! request body, before any JSON re-serialization.

use hmac::{Hmac, KeyInit, Mac};
use payrouters_types::PayRouterEvent;
use sha2::Sha256;
use subtle::ConstantTimeEq;

/// Maximum clock skew accepted by default.
pub const DEFAULT_TOLERANCE_SECS: i64 = 300;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum WebhookError {
    #[error("malformed PayRouters-Signature header")]
    MalformedHeader,
    #[error("signature does not match")]
    SignatureMismatch,
    #[error("timestamp outside the tolerance window")]
    TimestampOutsideTolerance,
    #[error("payload is not a valid event: {0}")]
    InvalidPayload(String),
}

fn sign(secret: &str, timestamp: i64, payload: &[u8]) -> Vec<u8> {
    let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(secret.as_bytes())
        .expect("HMAC accepts any key length");
    mac.update(timestamp.to_string().as_bytes());
    mac.update(b".");
    mac.update(payload);
    mac.finalize().into_bytes().to_vec()
}

/// Verify a delivery. `now` is the current unix time in seconds (pass `None` for the system clock).
pub fn verify(
    payload: &[u8],
    header: &str,
    secret: &str,
    tolerance_secs: i64,
    now: Option<i64>,
) -> Result<(), WebhookError> {
    let mut timestamp: Option<i64> = None;
    let mut candidates: Vec<Vec<u8>> = Vec::new();
    for part in header.split(',') {
        let (k, v) = part
            .trim()
            .split_once('=')
            .ok_or(WebhookError::MalformedHeader)?;
        match k {
            "t" => timestamp = Some(v.parse().map_err(|_| WebhookError::MalformedHeader)?),
            "v1" => candidates.push(hex::decode(v).map_err(|_| WebhookError::MalformedHeader)?),
            _ => {}
        }
    }
    let timestamp = timestamp.ok_or(WebhookError::MalformedHeader)?;
    if candidates.is_empty() {
        return Err(WebhookError::MalformedHeader);
    }
    let now = now.unwrap_or_else(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    });
    if (now - timestamp).abs() > tolerance_secs {
        return Err(WebhookError::TimestampOutsideTolerance);
    }
    let expected = sign(secret, timestamp, payload);
    // Evaluate every candidate (no early exit) so timing does not reveal which one matched.
    let mut ok = false;
    for c in &candidates {
        ok |= bool::from(c.ct_eq(&expected));
    }
    if ok {
        Ok(())
    } else {
        Err(WebhookError::SignatureMismatch)
    }
}

/// Verify, then parse the event.
pub fn construct_event(
    payload: &[u8],
    header: &str,
    secret: &str,
    tolerance_secs: i64,
    now: Option<i64>,
) -> Result<PayRouterEvent, WebhookError> {
    verify(payload, header, secret, tolerance_secs, now)?;
    serde_json::from_slice(payload).map_err(|e| WebhookError::InvalidPayload(e.to_string()))
}

/// Build a header for tests and local tooling.
pub fn sign_header(secret: &str, timestamp: i64, payload: &[u8]) -> String {
    format!(
        "t={timestamp},v1={}",
        hex::encode(sign(secret, timestamp, payload))
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "whsec_test";
    const T: i64 = 1_700_000_000;

    #[test]
    fn accepts_valid_signature() {
        let h = sign_header(SECRET, T, b"{}");
        assert_eq!(verify(b"{}", &h, SECRET, 300, Some(T + 10)), Ok(()));
    }

    #[test]
    fn rejects_tampering_wrong_secret_and_staleness() {
        let h = sign_header(SECRET, T, b"{\"a\":1}");
        assert_eq!(
            verify(b"{\"a\":2}", &h, SECRET, 300, Some(T)),
            Err(WebhookError::SignatureMismatch)
        );
        assert_eq!(
            verify(b"{\"a\":1}", &h, "whsec_other", 300, Some(T)),
            Err(WebhookError::SignatureMismatch)
        );
        assert_eq!(
            verify(b"{\"a\":1}", &h, SECRET, 300, Some(T + 301)),
            Err(WebhookError::TimestampOutsideTolerance)
        );
    }

    #[test]
    fn supports_rotation_with_multiple_v1() {
        let good = sign_header(SECRET, T, b"x");
        let v1 = good.split("v1=").nth(1).unwrap();
        let header = format!("t={T},v1={},v1={v1}", "00".repeat(32));
        assert_eq!(verify(b"x", &header, SECRET, 300, Some(T)), Ok(()));
    }

    #[test]
    fn rejects_malformed_headers() {
        for h in ["", "garbage", "t=abc,v1=00", "t=1", "v1=00", "t=1,v1=zz"] {
            assert_eq!(
                verify(b"x", h, SECRET, 300, Some(1)),
                Err(WebhookError::MalformedHeader),
                "{h}"
            );
        }
    }

    #[test]
    fn matches_the_server_signature_format() {
        // Same vector as payrouters-server's outbound::signature_header: t.body, hex HMAC-SHA256.
        let h = sign_header("whsec_test", 1_700_000_000, b"{\"a\":1}");
        assert!(h.starts_with("t=1700000000,v1="));
        assert_eq!(h.split("v1=").nth(1).unwrap().len(), 64);
    }
}
