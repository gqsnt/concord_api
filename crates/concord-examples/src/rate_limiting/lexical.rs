//! Lexical applicability composes independently from bucket partitioning.
//!
//! Required feature: `rate-limit`. Mode: deterministic local
//! execution. Documentation owner: `docs/rate_limits.md`.
//! Proving tests: `tests/construction.rs` and `tests/local_runtime.rs`.
//!
//! Key labels are explicit. The named policy uses its formal name as its
//! label; the inline item key uses `"endpoint.item"`. The tenant has an
//! explicit header role because required inputs need a semantic use
//! (VALUE-06).

use std::sync::Arc;

use concord::rate_limit::LocalRateLimiter;
use concord::{Binary, Bytes, api};

// ANCHOR: rate_limit_contract
api! {
    client RateLimitApi {
        base "http://rate-limit.example.test/";
        timeout 30s;

        values {
            header("X-Tenant") tenant: String;
        }

        rate_limits {
            policy account_reads(account) {
                id "example.account.read";
                key ["account" = account];
                cost 1;
                allow 100 per 1m;
                allow 1_000 per 1h;
            }
        }

        rate_limit {
            id "example.application";
            key [origin];
            cost 1;
            allow 500 per 10s;
        }

        scope region(region: String) {
            subdomain replace [region, "api"];
            rate_limit account_reads(account = region);
            rate_limit {
                id "concord.lexical.3";
                key [origin];
                cost 1;
                allow 200 per 1m;
            }

            GET item(item: String) -> Bytes via Binary {
                rate_limit {
                    id "concord.lexical.4";
                    key ["endpoint.item" = item];
                    cost 2;
                    allow 100 per 1m;
                }
            }
        }

        GET lookup(account: String) -> Bytes via Binary {
            rate_limit account_reads(account = account) { cost 2; }
        }

        GET public() -> Bytes via Binary;
    }
}
// ANCHOR_END: rate_limit_contract

/// Installs the first-party limiter explicitly and constructs both plan shapes.
// ANCHOR: configured_rate_limiter
pub fn construct() -> Result<(), concord::prelude::ClientBuildError> {
    let limiter = LocalRateLimiter::new();
    let client = RateLimitApi::builder()
        .set_tenant("example-tenant")
        .configure_runtime(|runtime| runtime.rate_limiter(Arc::new(limiter)))
        .build()?;
    let _: rate_limit_api::region::ItemRequest = client.region("eu").item("record-42");
    let _: rate_limit_api::LookupRequest = client.lookup("account-7");
    let _: rate_limit_api::PublicRequest = client.public();
    Ok(())
}
// ANCHOR_END: configured_rate_limiter
