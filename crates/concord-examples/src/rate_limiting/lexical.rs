//! Lexical applicability composes independently from bucket partitioning.
//!
//! Required feature: `rate-limit`. Mode: deterministic local
//! execution. Documentation owner: `docs/rate_limits.md`.
//! Proving tests: `tests/construction.rs` and `tests/local_runtime.rs`.

use std::sync::Arc;

use concord::api;
use concord::rate_limit::LocalRateLimiter;

// ANCHOR: rate_limit_contract
api! {
    client RateLimitApi {
        timeout off;
        base "http://rate-limit.example.test/";
        values {
            tenant: String;
        }
        rate_limit {
            id "example.application";
            allow 500 per 10s;
        }
        rate_limits {
            account_reads(account) {
                id "example.account.read";
                key [account];
                cost 1;
                allow 100 per 1m;
                allow 1_000 per 1h;
            }
        }


    scope region(region: String) {
        subdomain replace [region.region, "api"];
        rate_limit account_reads(account = region.region);
        rate_limit {
            key [host];
            allow 200 per 1m;
        }

        GET item(item: String) -> Bytes using Bytes {
            rate_limit {
                key [endpoint.item];
                cost 2;
                allow 100 per 1m;
            }
        }
    }

    GET lookup(account: String) -> Bytes using Bytes {
        rate_limit account_reads(account = endpoint.account) { cost 2; }
    }

    GET public() -> Bytes using Bytes;
}}
// ANCHOR_END: rate_limit_contract

/// Installs the first-party limiter explicitly and constructs both plan shapes.
// ANCHOR: configured_rate_limiter
pub fn construct() -> Result<(), concord::prelude::ClientBuildError> {
    let limiter =
        LocalRateLimiter::new().observer(Arc::new(super::observer::DelegatingRetryAfterObserver));
    let client = RateLimitApi::builder()
        .set_tenant("example-tenant".to_owned())
        .configure_runtime(|runtime| runtime.rate_limiter(Arc::new(limiter)))
        .build()?;
    let _: rate_limit_api::region::ItemRequest =
        client.region("eu".to_owned()).item("record-42".to_owned());
    let _: rate_limit_api::LookupRequest = client.lookup("account-7".to_owned());
    let _: rate_limit_api::PublicRequest = client.public();
    Ok(())
}
// ANCHOR_END: configured_rate_limiter
