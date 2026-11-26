use std::net::SocketAddr;

use anyhow::Context;
use base64::Engine;
use payrouters_server::util::{random_bytes, random_hex};
use payrouters_server::{Config, api, bootstrap, spawn_workers};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if std::env::args().nth(1).as_deref() == Some("gen-secrets") {
        println!(
            "MASTER_KEY={}\nADMIN_TOKEN={}",
            base64::engine::general_purpose::STANDARD.encode(random_bytes::<32>()),
            random_hex::<24>()
        );
        return Ok(());
    }

    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn"));
    if std::env::var("LOG_FORMAT").is_ok_and(|v| v == "json") {
        tracing_subscriber::fmt()
            .json()
            .with_env_filter(filter)
            .init();
    } else {
        tracing_subscriber::fmt().with_env_filter(filter).init();
    }

    let config = Config::from_env()?;
    let addr = config.listen_addr;
    if !config.web_dir.join("index.html").exists() {
        tracing::warn!(dir = %config.web_dir.display(), "WEB_DIR has no index.html; the dashboard will 404 (build it with `pnpm build` in web/)");
    }
    let state = bootstrap(config).await.context("startup failed")?;
    spawn_workers(&state);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("cannot bind {addr}"))?;
    tracing::info!(%addr, public = %state.config.public_base_url, "payrouters listening");
    // ConnectInfo gives the rate limiter the socket peer when no trusted proxy header is set.
    axum::serve(
        listener,
        api::router(state).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown())
    .await?;
    Ok(())
}

async fn shutdown() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let term = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = term => {} }
    tracing::info!("shutting down");
}
