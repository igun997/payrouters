//! Per-client rate limits and failed-authentication lockouts, held in memory per process.
//!
//! Two kinds of limit share one table of fixed windows:
//! - request limits ([`Limiter::hit`]): at most `limit` requests per window; the rest get 429.
//! - failure lockouts ([`Limiter::check_locked`] + [`Limiter::record_failure`]): after `limit`
//!   failed attempts in a window, every attempt in that window is refused before its credential
//!   is looked at. Only failures are counted, so a valid credential cannot reset the counter.
//!
//! Memory is bounded: expired windows are purged periodically, and at capacity the window that
//! expires soonest is evicted.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use axum::extract::{ConnectInfo, Request, State};
use axum::http::{Extensions, HeaderMap, HeaderName, Method};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::config::RateConfig;
use crate::error::AppError;
use crate::state::AppState;

/// Most keys tracked at once. Each entry is a few dozen bytes.
pub const MAX_KEYS: usize = 100_000;
/// Key used when no client address is known (handlers called without a socket).
pub const UNKNOWN_CLIENT: &str = "unknown";
pub const ADMIN_FAIL: &str = "admin-fail";
pub const API_KEY_FAIL: &str = "api-key-fail";
/// Invalid MCP access tokens and failed OAuth token exchanges share one lockout.
pub const OAUTH_FAIL: &str = "oauth-fail";

const MINUTE: Duration = Duration::from_secs(60);
const CLEANUP_EVERY: Duration = Duration::from_secs(60);

/// The client key of a request, inserted by [`middleware`] for handlers and extractors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientKey(pub String);

/// The client key recorded for this request, or [`UNKNOWN_CLIENT`].
pub fn client_key_from(extensions: &Extensions) -> &str {
    extensions
        .get::<ClientKey>()
        .map_or(UNKNOWN_CLIENT, |k| k.0.as_str())
}

struct Entry {
    count: u32,
    expires: Instant,
}

pub struct Limiter {
    entries: Mutex<HashMap<String, Entry>>,
    cap: usize,
}

impl Limiter {
    pub fn new(cap: usize) -> Limiter {
        Limiter {
            entries: Mutex::new(HashMap::new()),
            cap: cap.max(1),
        }
    }

    /// Counts one request for `key` in `scope`. Returns `Err(retry_after_secs)` once more than
    /// `limit` requests were counted in the current window.
    pub fn hit(
        &self,
        scope: &str,
        key: &str,
        limit: u32,
        window: Duration,
        now: Instant,
    ) -> Result<(), u64> {
        if limit == 0 {
            return Ok(());
        }
        let mut map = self.lock();
        let entry = self.entry(&mut map, scope, key, window, now);
        entry.count = entry.count.saturating_add(1);
        if entry.count > limit {
            Err(retry_after(entry.expires, now))
        } else {
            Ok(())
        }
    }

    /// `Err(retry_after_secs)` while `key` has `limit` recorded failures in the current window.
    pub fn check_locked(
        &self,
        scope: &str,
        key: &str,
        limit: u32,
        now: Instant,
    ) -> Result<(), u64> {
        if limit == 0 {
            return Ok(());
        }
        let map = self.lock();
        match map.get(&slot(scope, key)) {
            Some(e) if e.expires > now && e.count >= limit => Err(retry_after(e.expires, now)),
            _ => Ok(()),
        }
    }

    /// Records one failed attempt for `key`. Successes are never recorded.
    pub fn record_failure(
        &self,
        scope: &str,
        key: &str,
        limit: u32,
        window: Duration,
        now: Instant,
    ) {
        if limit == 0 {
            return;
        }
        let mut map = self.lock();
        let entry = self.entry(&mut map, scope, key, window, now);
        entry.count = entry.count.saturating_add(1);
    }

    /// Drops every window that has already ended.
    pub fn purge_expired(&self, now: Instant) {
        self.lock().retain(|_, e| e.expires > now);
    }

    /// The window for `scope|key`, opened now if absent or ended.
    fn entry<'a>(
        &self,
        map: &'a mut HashMap<String, Entry>,
        scope: &str,
        key: &str,
        window: Duration,
        now: Instant,
    ) -> &'a mut Entry {
        let slot = slot(scope, key);
        if !map.contains_key(&slot) && map.len() >= self.cap {
            make_room(map, self.cap, now);
        }
        let entry = map.entry(slot).or_insert(Entry {
            count: 0,
            expires: now + window,
        });
        if entry.expires <= now {
            entry.count = 0;
            entry.expires = now + window;
        }
        entry
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<String, Entry>> {
        // A poisoned lock only means another request panicked mid-update; the counts stay usable.
        self.entries.lock().unwrap_or_else(|p| p.into_inner())
    }

    #[cfg(test)]
    fn tracked(&self) -> usize {
        self.lock().len()
    }
}

fn slot(scope: &str, key: &str) -> String {
    format!("{scope}|{key}")
}

/// Frees one slot: expired windows first, then the window that expires soonest.
fn make_room(map: &mut HashMap<String, Entry>, cap: usize, now: Instant) {
    map.retain(|_, e| e.expires > now);
    if map.len() >= cap {
        let oldest = map
            .iter()
            .min_by_key(|(_, e)| e.expires)
            .map(|(k, _)| k.clone());
        if let Some(k) = oldest {
            map.remove(&k);
        }
    }
}

/// Seconds until `expires`, rounded up, and never 0 so clients always get a usable Retry-After.
fn retry_after(expires: Instant, now: Instant) -> u64 {
    let left = expires.saturating_duration_since(now);
    let secs = left.as_secs() + u64::from(left.subsec_nanos() > 0);
    secs.max(1)
}

/// The rate-limit key for a request. The first value of `trusted` (when configured and
/// well-formed) wins; otherwise the socket peer. IPv6 is grouped by /64 so one host cannot
/// rotate addresses to dodge limits.
pub fn client_key(
    headers: &HeaderMap,
    peer: Option<IpAddr>,
    trusted: Option<&HeaderName>,
) -> String {
    let forwarded = trusted
        .and_then(|name| headers.get(name))
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .and_then(|v| v.trim().parse::<IpAddr>().ok());
    match forwarded.or(peer) {
        Some(ip) => ip_key(ip.to_canonical()),
        None => UNKNOWN_CLIENT.to_string(),
    }
}

fn ip_key(ip: IpAddr) -> String {
    match ip {
        IpAddr::V4(v4) => v4.to_string(),
        IpAddr::V6(v6) => {
            let s = v6.segments();
            format!("{:x}:{:x}:{:x}:{:x}::/64", s[0], s[1], s[2], s[3])
        }
    }
}

fn is_under(path: &str, prefix: &str) -> bool {
    path == prefix
        || path
            .strip_prefix(prefix)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// Which request limits apply to a request: `(scope, limit per minute)` pairs. The SPA and
/// static files have none.
fn route_limits(rate: &RateConfig, method: &Method, path: &str) -> Vec<(&'static str, u32)> {
    if is_under(path, "/snap/api") {
        let mut out = vec![("snap", rate.snap_per_min)];
        let pay = path.ends_with("/pay") || path.ends_with("/simulate");
        if *method == Method::POST && pay {
            out.push(("snap-pay", rate.snap_pay_per_min));
        }
        out
    } else if is_under(path, "/hooks") {
        vec![("hooks", rate.hooks_per_min)]
    } else if (is_under(path, "/oauth") && path != "/oauth/authorize")
        || is_under(path, "/.well-known")
    {
        vec![("oauth", rate.oauth_per_min)]
    } else if is_under(path, "/v1")
        || is_under(path, "/admin")
        || is_under(path, "/mcp")
        || path == "/healthz"
    {
        vec![("global", rate.global_per_min)]
    } else {
        Vec::new()
    }
}

/// Applies the per-minute request limits and records the client key for the handlers.
pub async fn middleware(State(state): State<AppState>, mut req: Request, next: Next) -> Response {
    let peer = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0.ip());
    let key = client_key(
        req.headers(),
        peer,
        state.config.trusted_proxy_header.as_ref(),
    );
    let now = Instant::now();
    for (scope, limit) in route_limits(&state.config.rate, req.method(), req.uri().path()) {
        if let Err(retry) = state.limiter.hit(scope, &key, limit, MINUTE, now) {
            return AppError::RateLimited(retry).into_response();
        }
    }
    req.extensions_mut().insert(ClientKey(key));
    next.run(req).await
}

/// Purges expired windows every minute for the life of the process.
pub async fn run_cleanup(limiter: Arc<Limiter>) {
    let mut tick = tokio::time::interval(CLEANUP_EVERY);
    loop {
        tick.tick().await;
        limiter.purge_expired(Instant::now());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    const WIN: Duration = Duration::from_secs(300);

    fn t0() -> Instant {
        Instant::now()
    }

    #[test]
    fn hits_allow_limit_then_refuse_until_window_ends() {
        let lim = Limiter::new(16);
        let now = t0();
        for _ in 0..3 {
            assert_eq!(lim.hit("s", "a", 3, MINUTE, now), Ok(()));
        }
        assert_eq!(lim.hit("s", "a", 3, MINUTE, now), Err(60));
        // still refused a few seconds later, with the remaining time
        let later = now + Duration::from_secs(20);
        assert_eq!(lim.hit("s", "a", 3, MINUTE, later), Err(40));
        // a fresh window opens once the first has ended
        let next = now + MINUTE;
        assert_eq!(lim.hit("s", "a", 3, MINUTE, next), Ok(()));
    }

    #[test]
    fn keys_and_scopes_are_independent() {
        let lim = Limiter::new(16);
        let now = t0();
        assert_eq!(lim.hit("s", "a", 1, MINUTE, now), Ok(()));
        assert!(lim.hit("s", "a", 1, MINUTE, now).is_err());
        assert_eq!(lim.hit("s", "b", 1, MINUTE, now), Ok(()));
        assert_eq!(lim.hit("t", "a", 1, MINUTE, now), Ok(()));
    }

    #[test]
    fn lockout_starts_at_limit_failures_and_ignores_successes() {
        let lim = Limiter::new(16);
        let now = t0();
        for _ in 0..9 {
            assert_eq!(lim.check_locked("f", "ip", 10, now), Ok(()));
            lim.record_failure("f", "ip", 10, WIN, now);
        }
        assert_eq!(lim.check_locked("f", "ip", 10, now), Ok(()));
        lim.record_failure("f", "ip", 10, WIN, now);
        // the tenth failure locks the key for the rest of the window
        assert_eq!(lim.check_locked("f", "ip", 10, now), Err(300));
        let mid = now + Duration::from_secs(100);
        assert_eq!(lim.check_locked("f", "ip", 10, mid), Err(200));
        // other keys are not affected
        assert_eq!(lim.check_locked("f", "other", 10, mid), Ok(()));
        // the lock lifts when the window ends, and the counter starts again
        let after = now + WIN;
        assert_eq!(lim.check_locked("f", "ip", 10, after), Ok(()));
        lim.record_failure("f", "ip", 10, WIN, after);
        assert_eq!(lim.check_locked("f", "ip", 10, after), Ok(()));
    }

    #[test]
    fn zero_limit_disables_each_operation() {
        let lim = Limiter::new(16);
        let now = t0();
        for _ in 0..100 {
            assert_eq!(lim.hit("s", "a", 0, MINUTE, now), Ok(()));
            lim.record_failure("f", "a", 0, WIN, now);
        }
        assert_eq!(lim.check_locked("f", "a", 0, now), Ok(()));
        assert_eq!(lim.tracked(), 0);
    }

    #[test]
    fn purge_drops_only_ended_windows() {
        let lim = Limiter::new(16);
        let now = t0();
        lim.hit("s", "short", 5, Duration::from_secs(10), now)
            .unwrap();
        lim.hit("s", "long", 5, Duration::from_secs(100), now)
            .unwrap();
        lim.purge_expired(now + Duration::from_secs(10));
        assert_eq!(lim.tracked(), 1);
        lim.purge_expired(now + Duration::from_secs(101));
        assert_eq!(lim.tracked(), 0);
    }

    #[test]
    fn full_table_evicts_the_window_closest_to_expiry() {
        let lim = Limiter::new(2);
        let now = t0();
        lim.hit("s", "a", 5, Duration::from_secs(100), now).unwrap();
        lim.hit("s", "b", 5, Duration::from_secs(10), now).unwrap();
        // a third key evicts "b", which ends first
        lim.hit("s", "c", 5, Duration::from_secs(100), now).unwrap();
        assert_eq!(lim.tracked(), 2);
        assert!(lim.entries.lock().unwrap().contains_key("s|a"));
        assert!(lim.entries.lock().unwrap().contains_key("s|c"));
        assert!(!lim.entries.lock().unwrap().contains_key("s|b"));
    }

    #[test]
    fn full_table_prefers_dropping_ended_windows() {
        let lim = Limiter::new(2);
        let now = t0();
        lim.hit("s", "old", 5, Duration::from_secs(1), now).unwrap();
        lim.hit("s", "live", 5, Duration::from_secs(100), now)
            .unwrap();
        let later = now + Duration::from_secs(5);
        lim.hit("s", "new", 5, Duration::from_secs(100), later)
            .unwrap();
        let map = lim.entries.lock().unwrap();
        assert!(map.contains_key("s|live") && map.contains_key("s|new"));
        assert_eq!(map.len(), 2);
    }

    #[test]
    fn retry_after_rounds_up_and_is_never_zero() {
        let now = t0();
        assert_eq!(retry_after(now + Duration::from_millis(1200), now), 2);
        assert_eq!(retry_after(now + Duration::from_secs(300), now), 300);
        assert_eq!(retry_after(now + Duration::from_millis(1), now), 1);
        assert_eq!(retry_after(now, now), 1);
    }

    fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            h.append(*k, HeaderValue::from_str(v).unwrap());
        }
        h
    }

    #[test]
    fn client_key_prefers_trusted_header_leftmost_entry() {
        let xff = HeaderName::from_static("x-forwarded-for");
        let peer: IpAddr = "10.0.0.1".parse().unwrap();
        let h = headers(&[("x-forwarded-for", " 203.0.113.9 , 10.0.0.1")]);
        assert_eq!(client_key(&h, Some(peer), Some(&xff)), "203.0.113.9");
    }

    #[test]
    fn client_key_ignores_headers_unless_trusted() {
        let peer: IpAddr = "10.0.0.1".parse().unwrap();
        let h = headers(&[("x-forwarded-for", "203.0.113.9")]);
        assert_eq!(client_key(&h, Some(peer), None), "10.0.0.1");
    }

    #[test]
    fn client_key_falls_back_to_peer_then_unknown() {
        let xff = HeaderName::from_static("x-forwarded-for");
        let peer: IpAddr = "10.0.0.1".parse().unwrap();
        let garbage = headers(&[("x-forwarded-for", "not-an-ip")]);
        assert_eq!(client_key(&garbage, Some(peer), Some(&xff)), "10.0.0.1");
        let empty = HeaderMap::new();
        assert_eq!(client_key(&empty, None, Some(&xff)), UNKNOWN_CLIENT);
        assert_eq!(client_key(&empty, None, None), UNKNOWN_CLIENT);
    }

    #[test]
    fn client_key_groups_ipv6_by_slash_64_and_canonicalises_mapped_v4() {
        let a: IpAddr = "2001:db8:1:2:aaaa::1".parse().unwrap();
        let b: IpAddr = "2001:db8:1:2:bbbb::2".parse().unwrap();
        let empty = HeaderMap::new();
        assert_eq!(
            client_key(&empty, Some(a), None),
            client_key(&empty, Some(b), None)
        );
        assert_eq!(client_key(&empty, Some(a), None), "2001:db8:1:2::/64");
        let mapped: IpAddr = "::ffff:192.0.2.5".parse().unwrap();
        assert_eq!(client_key(&empty, Some(mapped), None), "192.0.2.5");
    }

    #[test]
    fn route_limits_classify_paths() {
        let rate = RateConfig::default();
        let post = Method::POST;
        let get = Method::GET;
        assert_eq!(
            route_limits(&rate, &get, "/snap/api/tok"),
            vec![("snap", 120)]
        );
        assert_eq!(
            route_limits(&rate, &post, "/snap/api/tok/pay"),
            vec![("snap", 120), ("snap-pay", 20)]
        );
        assert_eq!(
            route_limits(&rate, &post, "/snap/api/tok/simulate"),
            vec![("snap", 120), ("snap-pay", 20)]
        );
        assert_eq!(
            route_limits(&rate, &get, "/snap/api/tok/pay"),
            vec![("snap", 120)]
        );
        assert_eq!(
            route_limits(&rate, &post, "/hooks/abc"),
            vec![("hooks", 600)]
        );
        assert_eq!(
            route_limits(&rate, &get, "/v1/payments"),
            vec![("global", 600)]
        );
        assert_eq!(
            route_limits(&rate, &get, "/admin/me"),
            vec![("global", 600)]
        );
        assert_eq!(route_limits(&rate, &get, "/healthz"), vec![("global", 600)]);
        assert_eq!(route_limits(&rate, &post, "/mcp"), vec![("global", 600)]);
        assert_eq!(
            route_limits(&rate, &post, "/oauth/token"),
            vec![("oauth", 60)]
        );
        assert_eq!(
            route_limits(&rate, &get, "/.well-known/oauth-authorization-server"),
            vec![("oauth", 60)]
        );
        // the consent page is a dashboard page, like the rest of the SPA
        assert!(route_limits(&rate, &get, "/oauth/authorize").is_empty());
        // not API routes: the SPA and static files are never limited
        assert!(route_limits(&rate, &get, "/").is_empty());
        assert!(route_limits(&rate, &get, "/snap/tok").is_empty());
        assert!(route_limits(&rate, &get, "/snapshot").is_empty());
        assert!(route_limits(&rate, &get, "/administrator").is_empty());
        assert!(route_limits(&rate, &get, "/assets/app.js").is_empty());
    }
}
