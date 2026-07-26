//! Lexical applicability composes independently from bucket partitioning.
//!
//! Required feature: `rate-limit`. Mode: deterministic local
//! execution. Documentation owner:
//! `docs/guide/06-pagination-and-rate-limits.md`.
//! Proving tests: `tests/construction.rs` and `tests/local_runtime.rs`.

use std::sync::Arc;

use concord::api;
use concord::rate_limit::LocalRateLimiter;

api! {
    client RateLimitApi {
        base "http://rate-limit.example.test/";
        vars {
            tenant: String;
        }
        rate_limit { 500 / 10s; }
    }

    scope region(region: String) {
        host [scope.region, "api"];
        rate_limit {
            by [host];
            200 / 1m;
        }

        GET item(item: String) -> Bytes {
            rate_limit {
                by [ep.item];
                cost 2;
                100 / 1m;
            }
        }
    }

    GET public() -> Bytes;
}

/// Installs the first-party limiter explicitly and constructs both plan shapes.
pub fn construct() -> Result<(), concord::prelude::ClientBuildError> {
    let limiter =
        LocalRateLimiter::new().observer(Arc::new(super::observer::DelegatingRetryAfterObserver));
    let client = RateLimitApi::builder()
        .tenant("example-tenant".to_owned())
        .configure_runtime(|runtime| runtime.rate_limiter(Arc::new(limiter)))
        .build()?;
    let _: rate_limit_api::region::ItemRequest =
        client.region("eu".to_owned()).item("record-42".to_owned());
    let _: rate_limit_api::PublicRequest = client.public();
    Ok(())
}
