#![forbid(unsafe_code)]

use std::sync::atomic::{AtomicUsize, Ordering};

use runtime::advanced::{Cooldown, RateLimitObserver, RateLimitResponseHead};
use runtime::{Bytes, api};

pub static DEFAULT_OBSERVER_CALLS: AtomicUsize = AtomicUsize::new(0);

pub struct FixtureObserver;

impl Default for FixtureObserver {
    fn default() -> Self {
        DEFAULT_OBSERVER_CALLS.fetch_add(1, Ordering::SeqCst);
        Self
    }
}

impl RateLimitObserver for FixtureObserver {
    fn observe(&self, _response: &RateLimitResponseHead<'_>) -> Option<Cooldown> {
        None
    }
}

api! {
    client EmptyGeneratedApi {
        base "http://no-governor-empty.example.test/";
    }

    GET Read() -> Bytes {
        path ["read"];
    }
}

api! {
    client LimitedGeneratedApi {
        base "http://no-governor-limited.example.test/";
        rate_limit {
            cost 2;
            3 / 1m;
            30 / 10m;
        }
        rate_limit_observer crate::FixtureObserver;
    }

    GET Read() -> Bytes {
        path ["read"];
    }
}
