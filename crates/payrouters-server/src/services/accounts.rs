use std::collections::BTreeMap;

use anyhow::Context;
use chrono::{DateTime, Utc};
use payrouters_types::{FeeSchedule, Mode, Provider};
use uuid::Uuid;

use crate::error::{AppError, Result};
use crate::models::AccountRow;
use crate::providers::{Credentials, ProviderCtx};
use crate::state::AppState;
use crate::util::new_id;

/// A provider account with decrypted credentials. Never serialize this.
#[derive(Clone)]
pub struct AccountRecord {
    pub id: Uuid,
    pub provider: Provider,
    pub mode: Mode,
    pub label: String,
    pub enabled: bool,
    pub creds: Credentials,
    pub fees: FeeSchedule,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl AccountRecord {
    pub fn from_row(state: &AppState, row: AccountRow) -> Result<AccountRecord> {
        let json = state
            .cipher
            .decrypt_string(&row.credentials_enc)
            .with_context(|| format!("provider account {}", row.id))?;
        let map: BTreeMap<String, String> =
            serde_json::from_str(&json).context("stored credentials are not a string map")?;
        Ok(AccountRecord {
            id: row.id,
            provider: row
                .provider
                .parse()
                .map_err(|e: String| anyhow::anyhow!(e))?,
            mode: row.mode.parse().map_err(|e: String| anyhow::anyhow!(e))?,
            label: row.label,
            enabled: row.enabled,
            creds: Credentials(map),
            fees: row.fees.0,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }

    pub fn ctx<'a>(&'a self, state: &'a AppState) -> ProviderCtx<'a> {
        ProviderCtx {
            http: &state.http,
            account_id: self.id,
            mode: self.mode,
            creds: &self.creds,
            public_base_url: &state.config.public_base_url,
        }
    }
}

pub fn encrypt_creds(state: &AppState, creds: &BTreeMap<String, String>) -> Result<String> {
    let json = serde_json::to_string(creds).context("serialize credentials")?;
    Ok(state.cipher.encrypt_str(&json)?)
}

pub async fn load(state: &AppState, id: Uuid) -> Result<AccountRecord> {
    let row: Option<AccountRow> = sqlx::query_as("SELECT * FROM provider_accounts WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?;
    AccountRecord::from_row(
        state,
        row.ok_or_else(|| AppError::not_found("provider account"))?,
    )
}

pub async fn find_enabled(
    state: &AppState,
    mode: Mode,
    provider: Provider,
) -> Result<Option<AccountRecord>> {
    let row: Option<AccountRow> = sqlx::query_as(
        "SELECT * FROM provider_accounts WHERE mode = $1 AND provider = $2 AND enabled",
    )
    .bind(mode.as_str())
    .bind(provider.as_str())
    .fetch_optional(&state.pool)
    .await?;
    row.map(|r| AccountRecord::from_row(state, r)).transpose()
}

/// Providers that currently have an enabled account in `mode`, without decrypting anything.
pub async fn enabled_providers(state: &AppState, mode: Mode) -> Result<Vec<Provider>> {
    let rows: Vec<(String,)> =
        sqlx::query_as("SELECT provider FROM provider_accounts WHERE mode = $1 AND enabled")
            .bind(mode.as_str())
            .fetch_all(&state.pool)
            .await?;
    Ok(rows.into_iter().filter_map(|(p,)| p.parse().ok()).collect())
}

/// The enabled accounts of `mode` as `(provider, account id)`, without decrypting anything.
pub async fn enabled_accounts(state: &AppState, mode: Mode) -> Result<Vec<(Provider, Uuid)>> {
    let rows: Vec<(String, Uuid)> =
        sqlx::query_as("SELECT provider, id FROM provider_accounts WHERE mode = $1 AND enabled")
            .bind(mode.as_str())
            .fetch_all(&state.pool)
            .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(p, id)| p.parse().ok().map(|p| (p, id)))
        .collect())
}

/// The simulator is always available in sandbox so the playground works on a fresh install.
pub async fn seed_mock(state: &AppState) -> anyhow::Result<()> {
    let creds = encrypt_creds(state, &BTreeMap::new()).map_err(|e| anyhow::anyhow!("{e}"))?;
    sqlx::query(
        "INSERT INTO provider_accounts (id, provider, mode, label, credentials_enc)
         VALUES ($1, 'mock', 'sandbox', 'Mock simulator', $2)
         ON CONFLICT (provider, mode) DO NOTHING",
    )
    .bind(new_id())
    .bind(creds)
    .execute(&state.pool)
    .await?;
    Ok(())
}
