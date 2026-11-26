use axum::Router;
use axum::http::{HeaderName, HeaderValue};
use axum::middleware;
use axum::routing::get;
use tower_http::compression::CompressionLayer;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::TraceLayer;

use crate::error::Result;
use crate::rate_limit;
use crate::state::AppState;

pub mod admin;
pub mod auth;
pub mod snap;
pub mod v1;
pub mod webhooks;

async fn healthz(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> Result<&'static str> {
    sqlx::query("SELECT 1").execute(&state.pool).await?;
    Ok("ok")
}

pub fn router(state: AppState) -> Router {
    let index = state.config.web_dir.join("index.html");
    // The SPA owns every path that is not an API route (including /snap/<token>).
    let spa = ServeDir::new(&state.config.web_dir).fallback(ServeFile::new(index));
    let limited = state.clone();

    Router::new()
        .nest("/v1", v1::routes())
        .nest("/admin", admin::routes(state.clone()))
        .nest("/snap/api", snap::routes())
        .nest("/hooks", webhooks::routes())
        .route("/healthz", get(healthz))
        .fallback_service(spa)
        .with_state(state)
        // Outside the handlers so a refused request never reaches auth or the database.
        .layer(middleware::from_fn_with_state(
            limited,
            rate_limit::middleware,
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            HeaderName::from_static("x-content-type-options"),
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            // snap URLs carry a capability token; never leak it through Referer
            HeaderName::from_static("referrer-policy"),
            HeaderValue::from_static("no-referrer"),
        ))
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
}
