use std::sync::Arc;
use std::time::Duration;

use sqlx::PgPool;

use crate::config::Config;
use crate::crypto::Cipher;
use crate::rate_limit::{Limiter, MAX_KEYS};

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: Arc<Config>,
    /// Per-client request windows and failed-auth lockouts.
    pub limiter: Arc<Limiter>,
    pub cipher: Arc<Cipher>,
    /// Client for provider APIs.
    pub http: reqwest::Client,
    /// Client for product webhooks: short timeout, never follows redirects.
    pub hooks: reqwest::Client,
}

impl AppState {
    pub fn new(pool: PgPool, config: Config) -> anyhow::Result<AppState> {
        let cipher = Cipher::new(&config.master_key);
        let ua = concat!("payrouters/", env!("CARGO_PKG_VERSION"));
        let http = reqwest::Client::builder()
            .user_agent(ua)
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .build()?;
        let hooks = reqwest::Client::builder()
            .user_agent(concat!("PayRouters/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(10))
            .connect_timeout(Duration::from_secs(5))
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        Ok(AppState {
            pool,
            config: Arc::new(config),
            cipher: Arc::new(cipher),
            limiter: Arc::new(Limiter::new(MAX_KEYS)),
            http,
            hooks,
        })
    }

    pub fn snap_url(&self, token: &str) -> String {
        format!("{}/snap/{}", self.config.public_base_url, token)
    }

    pub fn provider_webhook_url(&self, account_id: uuid::Uuid) -> String {
        format!("{}/hooks/{}", self.config.public_base_url, account_id)
    }
}
