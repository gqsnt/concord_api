//! Compile-checked bucket-partitioning and local-state composition examples.
//!
//! Required feature: `rate-limit`. Mode: construction and deterministic local
//! execution. Documentation owner: `docs/rate_limits.md`. Proving tests:
//! `tests/construction.rs` and `tests/local_runtime.rs`.

use std::error::Error;
use std::sync::Arc;

use concord::rate_limit::{LocalRateLimiter, RateLimiter};
use concord::{Bytes, api};

// ANCHOR: rate_partitioning_recipe
// ANCHOR: partitioning_contract
api! {
    client PartitioningApi {
        timeout 30s;
        base "http://partitioning.example.test/";
        rate_limit { id "concord.partitioning.1"; cost 1;
            key [endpoint];
            allow 100 per 1m;
        }


    scope regions(region: String) {
        subdomain replace [regions.region, "api"];

        scope accounts(account: String) {
            rate_limit { id "concord.partitioning.2"; cost 1;
                key [accounts.account];
                allow 100 per 1m;
            }

            GET channel(channel: String) -> Bytes via Binary {
                rate_limit { id "concord.partitioning.3"; cost 1;
                    key [endpoint.channel];
                    allow 20 per 1s;
                }
            }

            GET composite(account: String) -> Bytes via Binary {
                rate_limit { id "concord.partitioning.4";
                    key [origin, regions.region, endpoint.account];
                    cost 2;
                    allow 100 per 1m;
                }
            }
        }
    }
}}
// ANCHOR_END: partitioning_contract

api! {
    client ResourceStateApi {
        timeout 30s;
        base "http://resource-state.example.test/";


    GET channel(channel: String) -> Bytes via Binary {
        rate_limit { id "concord.partitioning.5"; cost 1;
            key [endpoint.channel];
            allow 2 per 1h;
        }
    }
}}

api! {
    client SharedStateApi {
        timeout 30s;
        base "http://shared-state.example.test/";
        rate_limit { id "concord.partitioning.6"; key [origin]; cost 1;  allow 1 per 1h; }


    GET ping() -> Bytes via Binary;
}}

/// Constructs every partition shape and two clients sharing one limiter.
///
/// Ordinary clones of either built client also share its immutable runtime
/// snapshot and limiter object.
// ANCHOR: shared_limiter
pub fn construct() -> Result<(), Box<dyn Error>> {
    let partitioned = PartitioningApi::builder()
        .configure_runtime(|runtime| runtime.rate_limiter(Arc::new(LocalRateLimiter::new())))
        .build()?;
    let account = partitioned
        .regions("eu".to_owned())
        .accounts("account-42".to_owned());
    let _: partitioning_api::regions::accounts::ChannelRequest =
        account.channel("alerts".to_owned());
    let _: partitioning_api::regions::accounts::CompositeRequest =
        account.composite("resource-7".to_owned());

    let limiter: Arc<dyn RateLimiter> = Arc::new(LocalRateLimiter::new());
    let first = SharedStateApi::builder()
        .configure_runtime({
            let limiter = Arc::clone(&limiter);
            move |runtime| runtime.rate_limiter(limiter)
        })
        .build()?;
    let second = SharedStateApi::builder()
        .configure_runtime(move |runtime| runtime.rate_limiter(limiter))
        .build()?;
    let _: shared_state_api::PingRequest = first.ping();
    let _: shared_state_api::PingRequest = second.ping();

    let bounded: Arc<dyn RateLimiter> = Arc::new(LocalRateLimiter::new().quota_capacity(1)?);
    let high_cardinality = ResourceStateApi::builder()
        .configure_runtime(move |runtime| runtime.rate_limiter(bounded))
        .build()?;
    let _: resource_state_api::ChannelRequest = high_cardinality.channel("first-key".to_owned());
    Ok(())
}
// ANCHOR_END: shared_limiter
// ANCHOR_END: rate_partitioning_recipe

/// Returns whether an execution failure is recoverable local-state pressure.
#[must_use]
pub fn is_recoverable_state_capacity(error: &concord::ApiError) -> bool {
    error.rate_limit_kind() == Some(concord::rate_limit::RateLimitErrorKind::StateCapacity)
}
