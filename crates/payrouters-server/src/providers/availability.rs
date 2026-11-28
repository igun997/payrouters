//! Which standard methods a provider account has been seen to refuse.
//!
//! Providers do not say which channels an account has activated, and the dashboard is not always the
//! whole truth (Midtrans' Core API needs its own activation: every charge answers `402 Payment
//! channel is not activated` while the dashboard lists the channels as active). So availability is
//! learned from what the provider answers: a charge refused as "not activated" marks that method
//! inactive for the account, the Snap page stops offering it, and payments for it go straight to
//! the provider's hosted checkout. Marks expire, so an account that gets the channel activated
//! recovers by itself, and editing the account clears them at once.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use payrouters_types::Method;
use uuid::Uuid;

const TTL: Duration = Duration::from_secs(30 * 60);

static INACTIVE: LazyLock<Mutex<HashMap<(Uuid, Method), Instant>>> =
    LazyLock::new(Default::default);

pub fn mark_inactive(account: Uuid, method: Method) {
    mark_at(account, method, Instant::now());
}

pub fn is_inactive(account: Uuid, method: Method) -> bool {
    inactive_at(account, method, Instant::now())
}

/// Forget everything learned about an account (its credentials changed).
pub fn clear(account: Uuid) {
    if let Ok(mut m) = INACTIVE.lock() {
        m.retain(|(a, _), _| *a != account);
    }
}

/// The methods worth showing: those not known to be refused. When every one is known to be refused
/// the whole list comes back, so the customer can still pick one and be sent to the hosted checkout.
pub fn offered(account: Uuid, methods: &[Method]) -> Vec<Method> {
    offered_at(account, methods, Instant::now())
}

/// Every standard method is known to be refused.
pub fn all_inactive(account: Uuid, methods: &[Method]) -> bool {
    let now = Instant::now();
    !methods.is_empty() && methods.iter().all(|m| inactive_at(account, *m, now))
}

fn mark_at(account: Uuid, method: Method, now: Instant) {
    if let Ok(mut m) = INACTIVE.lock() {
        m.retain(|_, at| now.saturating_duration_since(*at) < TTL);
        m.insert((account, method), now);
    }
}

fn inactive_at(account: Uuid, method: Method, now: Instant) -> bool {
    INACTIVE
        .lock()
        .ok()
        .and_then(|m| m.get(&(account, method)).copied())
        .is_some_and(|at| now.saturating_duration_since(at) < TTL)
}

fn offered_at(account: Uuid, methods: &[Method], now: Instant) -> Vec<Method> {
    let live: Vec<Method> = methods
        .iter()
        .copied()
        .filter(|m| !inactive_at(account, *m, now))
        .collect();
    if live.is_empty() {
        methods.to_vec()
    } else {
        live
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Method; 3] = [Method::VaBca, Method::VaBni, Method::Qris];

    fn id() -> Uuid {
        crate::util::new_id()
    }

    #[test]
    fn a_refused_method_is_hidden_for_that_account_only() {
        let (a, b) = (id(), id());
        mark_inactive(a, Method::VaBca);
        assert!(is_inactive(a, Method::VaBca));
        assert!(!is_inactive(a, Method::Qris));
        assert!(!is_inactive(b, Method::VaBca));
        assert_eq!(offered(a, &ALL), vec![Method::VaBni, Method::Qris]);
        assert_eq!(offered(b, &ALL), ALL.to_vec());
        assert!(!all_inactive(a, &ALL));
    }

    #[test]
    fn when_every_method_is_refused_all_of_them_stay_offered() {
        let a = id();
        for m in ALL {
            mark_inactive(a, m);
        }
        assert!(all_inactive(a, &ALL));
        assert_eq!(offered(a, &ALL), ALL.to_vec());
        // no methods at all is not "all refused": that provider is hosted-only
        assert!(!all_inactive(a, &[]));
    }

    #[test]
    fn marks_expire_and_editing_the_account_clears_them() {
        let (a, other) = (id(), id());
        let t0 = Instant::now();
        mark_at(a, Method::Qris, t0);
        mark_at(other, Method::Qris, t0);
        assert!(inactive_at(
            a,
            Method::Qris,
            t0 + TTL - Duration::from_secs(1)
        ));
        assert!(!inactive_at(a, Method::Qris, t0 + TTL));
        assert_eq!(offered_at(a, &ALL, t0 + TTL), ALL.to_vec());

        clear(a);
        assert!(!inactive_at(a, Method::Qris, t0));
        assert!(inactive_at(other, Method::Qris, t0));
    }
}
