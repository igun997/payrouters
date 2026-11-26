use std::fmt::Display;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;

use anyhow::{Context, bail};
use axum::http::HeaderName;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use chrono_tz::Tz;

#[derive(Clone)]
pub struct Config {
    pub database_url: String,
    pub listen_addr: SocketAddr,
    /// Public origin, no trailing slash. Used for snap URLs and webhook URLs.
    pub public_base_url: String,
    pub admin_token: String,
    pub master_key: [u8; 32],
    pub web_dir: PathBuf,
    pub timezone: Tz,
    /// Header holding the real client address, set only behind a proxy that overwrites it.
    pub trusted_proxy_header: Option<HeaderName>,
    pub rate: RateConfig,
}

/// Rate limits and failed-auth lockouts. A limit of `0` turns that limiter off.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RateConfig {
    /// Failed admin-token attempts per client per window before lockout.
    pub auth_fails: u32,
    /// Failed product API-key attempts per client per window before lockout.
    pub api_key_fails: u32,
    pub fail_window: Duration,
    /// Requests per minute per client on `/snap/api/*`.
    pub snap_per_min: u32,
    /// POST `/snap/api/*/pay` and `/simulate` per client per minute (counted in addition to `snap_per_min`).
    pub snap_pay_per_min: u32,
    /// Inbound provider webhooks on `/hooks/*` per client per minute.
    pub hooks_per_min: u32,
    /// `/v1`, `/admin` and `/healthz` per client per minute.
    pub global_per_min: u32,
}

impl Default for RateConfig {
    fn default() -> Self {
        RateConfig {
            auth_fails: 10,
            api_key_fails: 20,
            fail_window: Duration::from_secs(300),
            snap_per_min: 120,
            snap_pay_per_min: 20,
            hooks_per_min: 600,
            global_per_min: 600,
        }
    }
}

impl RateConfig {
    /// Reads the `RATE_*` variables through `get` (the process environment in production).
    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> anyhow::Result<RateConfig> {
        let d = RateConfig::default();
        let window_secs: u64 = parse_or(&get, "RATE_AUTH_WINDOW_SECS", d.fail_window.as_secs())?;
        if window_secs == 0 {
            bail!("RATE_AUTH_WINDOW_SECS must be at least 1");
        }
        Ok(RateConfig {
            auth_fails: parse_or(&get, "RATE_AUTH_FAILS", d.auth_fails)?,
            api_key_fails: parse_or(&get, "RATE_API_KEY_FAILS", d.api_key_fails)?,
            fail_window: Duration::from_secs(window_secs),
            snap_per_min: parse_or(&get, "RATE_SNAP_PER_MIN", d.snap_per_min)?,
            snap_pay_per_min: parse_or(&get, "RATE_SNAP_PAY_PER_MIN", d.snap_pay_per_min)?,
            hooks_per_min: parse_or(&get, "RATE_HOOKS_PER_MIN", d.hooks_per_min)?,
            global_per_min: parse_or(&get, "RATE_GLOBAL_PER_MIN", d.global_per_min)?,
        })
    }
}

/// `TRUSTED_PROXY_HEADER`: unset or blank means the socket peer is the client.
pub fn trusted_proxy_header(
    get: impl Fn(&str) -> Option<String>,
) -> anyhow::Result<Option<HeaderName>> {
    match non_blank(get("TRUSTED_PROXY_HEADER")) {
        None => Ok(None),
        Some(name) => HeaderName::from_bytes(name.as_bytes())
            .map(Some)
            .map_err(|_| {
                anyhow::anyhow!("TRUSTED_PROXY_HEADER must be a header name, e.g. x-forwarded-for")
            }),
    }
}

impl Config {
    pub fn from_env() -> anyhow::Result<Config> {
        let get = |key: &str| std::env::var(key).ok();
        let database_url = req("DATABASE_URL")?;
        let listen_addr = opt("LISTEN_ADDR", "0.0.0.0:8080")
            .parse()
            .context("LISTEN_ADDR must be host:port")?;
        let public_base_url = req("PUBLIC_BASE_URL")?.trim_end_matches('/').to_string();
        if !(public_base_url.starts_with("http://") || public_base_url.starts_with("https://")) {
            bail!("PUBLIC_BASE_URL must start with http:// or https://");
        }
        let admin_token = req("ADMIN_TOKEN")?;
        if admin_token.len() < 16 {
            bail!("ADMIN_TOKEN must be at least 16 characters (try: openssl rand -hex 24)");
        }
        let master_key = parse_master_key(&req("MASTER_KEY")?)?;
        let timezone: Tz = opt("STATS_TIMEZONE", "Asia/Jakarta")
            .parse()
            .map_err(|e| anyhow::anyhow!("STATS_TIMEZONE: {e}"))?;
        Ok(Config {
            database_url,
            listen_addr,
            public_base_url,
            admin_token,
            master_key,
            web_dir: PathBuf::from(opt("WEB_DIR", "web/build")),
            timezone,
            trusted_proxy_header: trusted_proxy_header(get)?,
            rate: RateConfig::from_lookup(get)?,
        })
    }
}

pub fn parse_master_key(raw: &str) -> anyhow::Result<[u8; 32]> {
    let bytes = STANDARD
        .decode(raw.trim())
        .context("MASTER_KEY must be base64 (try: openssl rand -base64 32)")?;
    bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("MASTER_KEY must decode to exactly 32 bytes"))
}

fn parse_or<T>(get: &impl Fn(&str) -> Option<String>, key: &str, default: T) -> anyhow::Result<T>
where
    T: FromStr,
    T::Err: Display,
{
    match non_blank(get(key)) {
        None => Ok(default),
        Some(raw) => raw
            .parse()
            .map_err(|e| anyhow::anyhow!("{key} must be a whole number: {e}")),
    }
}

fn non_blank(value: Option<String>) -> Option<String> {
    value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn req(key: &str) -> anyhow::Result<String> {
    std::env::var(key).with_context(|| format!("missing env var {key}"))
}

fn opt(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |k: &str| map.get(k).cloned()
    }

    #[test]
    fn master_key_must_be_32_bytes() {
        let ok = STANDARD.encode([7u8; 32]);
        assert!(parse_master_key(&ok).is_ok());
        assert!(parse_master_key(&STANDARD.encode([7u8; 16])).is_err());
        assert!(parse_master_key("not base64!!").is_err());
    }

    #[test]
    fn rate_defaults_apply_when_unset_or_blank() {
        assert_eq!(
            RateConfig::from_lookup(env(&[])).unwrap(),
            RateConfig::default()
        );
        let blank = env(&[("RATE_AUTH_FAILS", "  "), ("RATE_SNAP_PER_MIN", "")]);
        assert_eq!(
            RateConfig::from_lookup(blank).unwrap(),
            RateConfig::default()
        );
    }

    #[test]
    fn rate_values_parse_and_zero_disables() {
        let cfg = RateConfig::from_lookup(env(&[
            ("RATE_AUTH_FAILS", "0"),
            ("RATE_API_KEY_FAILS", "5"),
            ("RATE_AUTH_WINDOW_SECS", "60"),
            ("RATE_SNAP_PER_MIN", "7"),
            ("RATE_SNAP_PAY_PER_MIN", "3"),
            ("RATE_HOOKS_PER_MIN", "0"),
            ("RATE_GLOBAL_PER_MIN", "9"),
        ]))
        .unwrap();
        assert_eq!(cfg.auth_fails, 0);
        assert_eq!(cfg.api_key_fails, 5);
        assert_eq!(cfg.fail_window, Duration::from_secs(60));
        assert_eq!(cfg.snap_per_min, 7);
        assert_eq!(cfg.snap_pay_per_min, 3);
        assert_eq!(cfg.hooks_per_min, 0);
        assert_eq!(cfg.global_per_min, 9);
    }

    #[test]
    fn rate_rejects_garbage_and_zero_window() {
        let bad = RateConfig::from_lookup(env(&[("RATE_HOOKS_PER_MIN", "many")]));
        let msg = bad.unwrap_err().to_string();
        assert!(msg.contains("RATE_HOOKS_PER_MIN"), "{msg}");
        assert!(RateConfig::from_lookup(env(&[("RATE_AUTH_WINDOW_SECS", "0")])).is_err());
        assert!(RateConfig::from_lookup(env(&[("RATE_GLOBAL_PER_MIN", "-1")])).is_err());
    }

    #[test]
    fn trusted_proxy_header_is_optional_and_validated() {
        assert_eq!(trusted_proxy_header(env(&[])).unwrap(), None);
        assert_eq!(
            trusted_proxy_header(env(&[("TRUSTED_PROXY_HEADER", " ")])).unwrap(),
            None
        );
        assert_eq!(
            trusted_proxy_header(env(&[("TRUSTED_PROXY_HEADER", " X-Forwarded-For ")])).unwrap(),
            Some(HeaderName::from_static("x-forwarded-for"))
        );
        assert!(trusted_proxy_header(env(&[("TRUSTED_PROXY_HEADER", "not a header")])).is_err());
    }
}
