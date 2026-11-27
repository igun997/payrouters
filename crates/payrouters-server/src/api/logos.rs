//! Bank and QRIS logos for the Snap page and the dashboard.
//!
//! Fetched once from a public favicon service by the server, kept in memory and served from the
//! same origin, so the payer's browser never talks to a third party (and the page keeps working
//! under a strict image policy). Only the fixed codes below are accepted, so the route cannot be
//! used to make the server fetch anything else. When the fetch fails the route answers 404 and the
//! UI keeps its text badge.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use bytes::Bytes;

use crate::state::AppState;

/// Code (as in `instructions.bank` and `docs/api.md`) and the site whose icon is the logo.
const SOURCES: [(&str, &str); 6] = [
    ("bca", "bca.co.id"),
    ("bni", "bni.co.id"),
    ("bri", "bri.co.id"),
    ("mandiri", "bankmandiri.co.id"),
    ("permata", "permatabank.com"),
    ("qris", "qris.id"),
];

const MAX_BYTES: usize = 256 * 1024;
const KEEP: Duration = Duration::from_secs(7 * 24 * 3600);
const RETRY_AFTER_MISS: Duration = Duration::from_secs(600);

#[derive(Clone)]
enum Entry {
    Logo {
        content_type: &'static str,
        bytes: Bytes,
        at: Instant,
    },
    Missing {
        at: Instant,
    },
}

static CACHE: LazyLock<Mutex<HashMap<&'static str, Entry>>> = LazyLock::new(Default::default);

pub fn routes() -> Router<AppState> {
    Router::new().route("/assets/logo/{code}", get(logo))
}

fn upstream_url(domain: &str) -> String {
    // The direct address of what www.google.com/s2/favicons redirects to: no redirect to follow.
    format!(
        "https://t1.gstatic.com/faviconV2?client=SOCIAL&type=FAVICON&fallback_opts=TYPE,SIZE,URL&url=http://{domain}&size=128"
    )
}

/// `png`, `jpeg` or `gif` only: the type decides what the browser may do with the bytes.
fn image_type(content_type: &str) -> Option<&'static str> {
    match content_type.split(';').next()?.trim() {
        "image/png" => Some("image/png"),
        "image/jpeg" => Some("image/jpeg"),
        "image/gif" => Some("image/gif"),
        _ => None,
    }
}

async fn fetch(http: &reqwest::Client, domain: &str) -> Option<(&'static str, Bytes)> {
    let resp = http
        .get(upstream_url(domain))
        .timeout(Duration::from_secs(8))
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let content_type = image_type(resp.headers().get(header::CONTENT_TYPE)?.to_str().ok()?)?;
    if resp
        .content_length()
        .is_some_and(|n| n as usize > MAX_BYTES)
    {
        return None;
    }
    let bytes = resp.bytes().await.ok()?;
    (!bytes.is_empty() && bytes.len() <= MAX_BYTES).then_some((content_type, bytes))
}

fn cached(code: &str) -> Option<Entry> {
    let entry = CACHE.lock().ok()?.get(code)?.clone();
    let fresh = match &entry {
        Entry::Logo { at, .. } => at.elapsed() < KEEP,
        Entry::Missing { at } => at.elapsed() < RETRY_AFTER_MISS,
    };
    fresh.then_some(entry)
}

fn respond(entry: Entry) -> Response {
    match entry {
        Entry::Logo {
            content_type,
            bytes,
            ..
        } => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, HeaderValue::from_static(content_type))
            .header(
                header::CACHE_CONTROL,
                HeaderValue::from_static("public, max-age=86400"),
            )
            .body(Body::from(bytes))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response()),
        Entry::Missing { .. } => (
            StatusCode::NOT_FOUND,
            [(header::CACHE_CONTROL, "public, max-age=300")],
        )
            .into_response(),
    }
}

async fn logo(State(state): State<AppState>, Path(code): Path<String>) -> Response {
    let Some((key, domain)) = SOURCES.iter().find(|(c, _)| *c == code) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if let Some(entry) = cached(key) {
        return respond(entry);
    }
    let entry = match fetch(&state.http, domain).await {
        Some((content_type, bytes)) => Entry::Logo {
            content_type,
            bytes,
            at: Instant::now(),
        },
        None => Entry::Missing { at: Instant::now() },
    };
    if let Ok(mut cache) = CACHE.lock() {
        cache.insert(key, entry.clone());
    }
    respond(entry)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_raster_images_are_served() {
        assert_eq!(image_type("image/png"), Some("image/png"));
        assert_eq!(image_type("image/jpeg; charset=binary"), Some("image/jpeg"));
        assert_eq!(image_type("image/svg+xml"), None);
        assert_eq!(image_type("text/html"), None);
    }

    #[test]
    fn every_code_has_a_site_and_the_address_is_fixed() {
        for (code, domain) in SOURCES {
            assert!(!code.is_empty() && domain.contains('.'));
            let url = upstream_url(domain);
            assert!(url.starts_with("https://t1.gstatic.com/faviconV2?"));
            assert!(url.contains(&format!("url=http://{domain}&")));
        }
    }
}
