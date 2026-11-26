pub mod api;
pub mod config;
pub mod crypto;
pub mod error;
pub mod models;
pub mod providers;
pub mod rate_limit;
pub mod services;
pub mod state;
pub mod util;

use sqlx::postgres::PgPoolOptions;

pub use config::Config;
pub use state::AppState;

/// Connect, migrate, seed. Shared by `main` and the integration tests.
pub async fn bootstrap(config: Config) -> anyhow::Result<AppState> {
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(std::time::Duration::from_secs(10))
        .connect(&config.database_url)
        .await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    let state = AppState::new(pool, config)?;
    services::accounts::seed_mock(&state).await?;
    Ok(state)
}

pub fn spawn_workers(state: &AppState) {
    tokio::spawn(services::outbound::run_loop(state.clone()));
    tokio::spawn(services::worker::run_loop(state.clone()));
    tokio::spawn(rate_limit::run_cleanup(state.limiter.clone()));
}
