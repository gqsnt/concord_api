use std::sync::Arc;

use concord_core::advanced::NoopRateLimiter;
use concord_macros::api;

pub struct Displayless;

api! {
    client Api {
        base "https://example.test";
        rate_limits { item { by [ep.item]; 1 / 1s; } }
        GET Read(item: crate::Displayless) -> Bytes {
            rate_limit item;
        }
    }
}

fn main() {
    let client = Api::builder()
        .runtime(|runtime| runtime.rate_limiter(Arc::new(NoopRateLimiter::new())))
        .build()
        .unwrap();
    let _ = client.read(Displayless);
}
