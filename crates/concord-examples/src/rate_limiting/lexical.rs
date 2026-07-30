//! Lexical applicability composes independently from bucket partitioning.
//!
//! Required feature: `rate-limit`. Mode: deterministic local
//! execution. Documentation owner:
//! `docs/retry_and_rate_limit.md`.
//! Proving tests: `tests/construction.rs` and `tests/local_runtime.rs`.

use std::sync::Arc;

use concord::api;
use concord::rate_limit::LocalRateLimiter;

// ANCHOR: rate_limit_contract
api! {
    client RateLimitApi {
        base "http://rate-limit.example.test/";
        vars {
            tenant: String;
        }
        rate_limit {
            id "example.application";
            500 / 10s;
        }
        rate_limits {
            rule account_reads {
                id "example.account.read";
                by [account];
                cost 1;
                100 / 1m;
                1_000 / 1h;
            }
        }
    }

    scope region(region: String) {
        host [scope.region, "api"];
        rate_limit use account_reads {
            account = scope.region;
        }
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

    GET lookup(account: String) -> Bytes {
        rate_limit use account_reads {
            account = ep.account;
            cost 2;
        }
    }

    GET public() -> Bytes;
}
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
