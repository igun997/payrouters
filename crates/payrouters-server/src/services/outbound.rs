//! Outbound webhooks (PayRouters → products): transactional outbox + retrying sender.

use std::time::Duration;

use chrono::Utc;
use payrouters_types::{EventData, EventType, PayRouterEvent, Payment};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::error::Result;
use crate::models::DeliveryRow;
use crate::state::AppState;
use crate::util::{hmac_sha256_hex, new_id};

/// Seconds to wait after the n-th failed attempt (index n-1).
pub const BACKOFF_SECS: [i64; 8] = [10, 60, 300, 1_800, 7_200, 21_600, 43_200, 86_400];
pub const MAX_ATTEMPTS: i32 = BACKOFF_SECS.len() as i32;
const LEASE: Duration = Duration::from_secs(300);

pub fn signature_header(secret: &str, timestamp: i64, body: &str) -> String {
    let sig = hmac_sha256_hex(
        secret.as_bytes(),
        &[timestamp.to_string().as_bytes(), b".", body.as_bytes()],
    );
    format!("t={timestamp},v1={sig}")
}

/// Queue an event inside the caller's transaction. No-op when the product has no webhook URL.
pub async fn enqueue(
    conn: &mut PgConnection,
    product_id: Uuid,
    payment: &Payment,
    event_type: EventType,
) -> Result<()> {
    let url: Option<(Option<String>,)> =
        sqlx::query_as("SELECT webhook_url FROM products WHERE id = $1")
            .bind(product_id)
            .fetch_optional(&mut *conn)
            .await?;
    if url.and_then(|(u,)| u).is_none() {
        return Ok(());
    }
    let event_id = format!("evt_{}", new_id().simple());
    let event = PayRouterEvent {
        id: event_id.clone(),
        event_type,
        livemode: payment.livemode,
        created_at: Utc::now(),
        data: EventData {
            payment: payment.clone(),
        },
    };
    sqlx::query(
        "INSERT INTO webhook_deliveries
           (id, event_id, payment_id, product_id, event_type, payload, status, next_attempt_at)
         VALUES ($1, $2, $3, $4, $5, $6, 'pending', now())",
    )
    .bind(new_id())
    .bind(event_id)
    .bind(payment.id)
    .bind(product_id)
    .bind(event_type.as_str())
    .bind(serde_json::to_value(&event).map_err(anyhow::Error::from)?)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Claim due deliveries (leasing them so a crash cannot lose or double-send forever).
async fn claim(state: &AppState, limit: i64) -> Result<Vec<DeliveryRow>> {
    let rows = sqlx::query_as::<_, DeliveryRow>(
        "UPDATE webhook_deliveries SET next_attempt_at = now() + make_interval(secs => $2)
         WHERE id IN (
             SELECT id FROM webhook_deliveries
             WHERE status = 'pending' AND next_attempt_at <= now()
             ORDER BY next_attempt_at
             LIMIT $1
             FOR UPDATE SKIP LOCKED
         )
         RETURNING *",
    )
    .bind(limit)
    .bind(LEASE.as_secs() as f64)
    .fetch_all(&state.pool)
    .await?;
    Ok(rows)
}

struct Attempt {
    status_code: Option<i32>,
    error: Option<String>,
    ok: bool,
}

async fn attempt(state: &AppState, d: &DeliveryRow) -> Result<Attempt> {
    let target: Option<(Option<String>, String)> =
        sqlx::query_as("SELECT webhook_url, webhook_secret_enc FROM products WHERE id = $1")
            .bind(d.product_id)
            .fetch_optional(&state.pool)
            .await?;
    let Some((Some(url), secret_enc)) = target else {
        return Ok(Attempt {
            status_code: None,
            error: Some("product has no webhook_url".into()),
            ok: false,
        });
    };
    let secret = state.cipher.decrypt_string(&secret_enc)?;
    let body = serde_json::to_string(&d.payload).map_err(anyhow::Error::from)?;
    let ts = Utc::now().timestamp();
    let res = state
        .hooks
        .post(&url)
        .header("Content-Type", "application/json")
        .header("PayRouters-Signature", signature_header(&secret, ts, &body))
        .header("PayRouters-Event-Id", &d.event_id)
        .body(body)
        .send()
        .await;
    Ok(match res {
        Ok(resp) => {
            let code = resp.status();
            Attempt {
                status_code: Some(i32::from(code.as_u16())),
                error: (!code.is_success()).then(|| format!("HTTP {code}")),
                ok: code.is_success(),
            }
        }
        Err(e) => Attempt {
            status_code: None,
            error: Some(e.without_url().to_string()),
            ok: false,
        },
    })
}

async fn record(state: &AppState, d: &DeliveryRow, a: &Attempt) -> Result<()> {
    let attempts = d.attempts + 1;
    if a.ok {
        sqlx::query(
            "UPDATE webhook_deliveries
             SET status = 'delivered', attempts = $2, next_attempt_at = NULL,
                 last_status_code = $3, last_error = NULL, delivered_at = now()
             WHERE id = $1",
        )
        .bind(d.id)
        .bind(attempts)
        .bind(a.status_code)
        .execute(&state.pool)
        .await?;
    } else if attempts >= MAX_ATTEMPTS {
        sqlx::query(
            "UPDATE webhook_deliveries
             SET status = 'failed', attempts = $2, next_attempt_at = NULL,
                 last_status_code = $3, last_error = $4
             WHERE id = $1",
        )
        .bind(d.id)
        .bind(attempts)
        .bind(a.status_code)
        .bind(&a.error)
        .execute(&state.pool)
        .await?;
    } else {
        let wait = BACKOFF_SECS[(attempts - 1) as usize];
        sqlx::query(
            "UPDATE webhook_deliveries
             SET attempts = $2, next_attempt_at = now() + make_interval(secs => $3),
                 last_status_code = $4, last_error = $5
             WHERE id = $1",
        )
        .bind(d.id)
        .bind(attempts)
        .bind(wait as f64)
        .bind(a.status_code)
        .bind(&a.error)
        .execute(&state.pool)
        .await?;
    }
    Ok(())
}

/// Process one batch; returns how many deliveries were attempted.
pub async fn run_once(state: &AppState) -> Result<usize> {
    let batch = claim(state, 20).await?;
    let n = batch.len();
    let jobs = batch.into_iter().map(|d| {
        let state = state.clone();
        async move {
            match attempt(&state, &d).await {
                Ok(a) => {
                    if let Err(e) = record(&state, &d, &a).await {
                        tracing::error!(error = ?e, delivery = %d.id, "failed to record delivery result");
                    }
                }
                Err(e) => tracing::error!(error = ?e, delivery = %d.id, "delivery attempt errored"),
            }
        }
    });
    futures_util::future::join_all(jobs).await;
    Ok(n)
}

pub async fn run_loop(state: AppState) {
    loop {
        match run_once(&state).await {
            Ok(n) if n >= 20 => continue,
            Ok(_) => {}
            Err(e) => tracing::error!(error = ?e, "outbound worker tick failed"),
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_format_is_stable() {
        let h = signature_header("whsec_test", 1_700_000_000, "{\"a\":1}");
        assert!(h.starts_with("t=1700000000,v1="));
        let v1 = h.split("v1=").nth(1).unwrap();
        assert_eq!(v1.len(), 64);
        assert_eq!(
            h,
            signature_header("whsec_test", 1_700_000_000, "{\"a\":1}")
        );
        assert_ne!(
            h,
            signature_header("whsec_test", 1_700_000_001, "{\"a\":1}")
        );
    }

    #[test]
    fn backoff_schedule() {
        assert_eq!(MAX_ATTEMPTS, 8);
        assert_eq!(BACKOFF_SECS[0], 10);
        assert_eq!(BACKOFF_SECS[7], 86_400);
    }
}
