//! Background reconciliation: poll providers for pending payments (webhooks can be lost; IDRX
//! never retries) and expire payments whose window has passed.

use std::time::Duration;

use payrouters_types::PaymentStatus;
use uuid::Uuid;

use super::payments::{self, ApplyResult};
use crate::providers::StatusUpdate;
use crate::state::AppState;

const TICK: Duration = Duration::from_secs(30);
const POLL_AFTER_SECS: f64 = 300.0;
const EXPIRE_GRACE_SECS: f64 = 120.0;

pub async fn run_loop(state: AppState) {
    loop {
        if let Err(e) = tick(&state).await {
            tracing::error!(error = ?e, "reconciliation tick failed");
        }
        tokio::time::sleep(TICK).await;
    }
}

pub async fn tick(state: &AppState) -> anyhow::Result<()> {
    poll_pending(state).await?;
    expire_overdue(state).await?;
    Ok(())
}

async fn poll_pending(state: &AppState) -> anyhow::Result<()> {
    let ids: Vec<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM payments
         WHERE status = 'pending' AND provider <> 'mock' AND instructions IS NOT NULL
           AND (last_synced_at IS NULL OR last_synced_at < now() - make_interval(secs => $1))
         ORDER BY last_synced_at NULLS FIRST
         LIMIT 20",
    )
    .bind(POLL_AFTER_SECS)
    .fetch_all(&state.pool)
    .await?;
    for (id,) in ids {
        if let Err(e) = payments::sync(state, id).await {
            tracing::warn!(payment = %id, error = %e, "poll failed");
        }
    }
    Ok(())
}

async fn expire_overdue(state: &AppState) -> anyhow::Result<()> {
    let ids: Vec<(Uuid, bool)> = sqlx::query_as(
        "SELECT id, (provider <> 'mock' AND instructions IS NOT NULL) AS check_provider
         FROM payments
         WHERE status = 'pending' AND expires_at < now() - make_interval(secs => $1)
         ORDER BY expires_at
         LIMIT 50",
    )
    .bind(EXPIRE_GRACE_SECS)
    .fetch_all(&state.pool)
    .await?;
    for (id, check_provider) in ids {
        // Never expire a payment the provider may already have collected.
        if check_provider && let Err(e) = payments::sync(state, id).await {
            tracing::warn!(payment = %id, error = %e, "pre-expiry sync failed; expiring on schedule anyway");
        }
        match payments::apply_update(state, id, StatusUpdate::new(PaymentStatus::Expired)).await {
            Ok(ApplyResult::Applied(_)) | Ok(ApplyResult::Noop) | Ok(ApplyResult::Rejected(_)) => {}
            Err(e) => tracing::warn!(payment = %id, error = %e, "expiry failed"),
        }
    }
    Ok(())
}
