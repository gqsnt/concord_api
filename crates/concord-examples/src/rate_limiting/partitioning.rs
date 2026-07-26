//! Compile-checked bucket-partitioning and local-state composition examples.
//!
//! Required feature: `rate-limit`. Mode: construction and deterministic local
//! execution. Documentation owner:
//! `docs/features/rate-limiting.md`. Proving tests:
//! `tests/construction.rs` and `tests/local_runtime.rs`.

use std::error::Error;
use std::sync::Arc;

use concord::api;
use concord::rate_limit::{LocalRateLimiter, RateLimiter};

// ANCHOR: partitioning_contract
api! {
    client PartitioningApi {
        base "http://partitioning.example.test/";
        rate_limit {
            by [endpoint];
            100 / 1m;
        }
    }

    scope regions(region: String) {
        host [scope.region, "api"];

        scope accounts(account: String) {
            rate_limit {
                by [scope.account];
                100 / 1m;
            }

            GET channel(channel: String) -> Bytes {
                rate_limit {
                    by [ep.channel];
                    20 / 1s;
                }
            }

            GET composite(account: String) -> Bytes {
                rate_limit {
                    by [host, scope.region, ep.account];
                    cost 2;
                    100 / 1m;
                }
            }
        }
    }
}
// ANCHOR_END: partitioning_contract

api! {
    client ResourceStateApi {
        base "http://resource-state.example.test/";
    }

    GET channel(channel: String) -> Bytes {
        rate_limit {
            by [ep.channel];
            2 / 1h;
        }
    }
}

api! {
    client SharedStateApi {
        base "http://shared-state.example.test/";
        rate_limit { 1 / 1h; }
    }

    GET ping() -> Bytes;
}

/// Constructs every partition shape and two clients sharing one limiter.
///
/// Ordinary clones of either built client also share its immutable runtime
/// snapshot and limiter object.
// ANCHOR: shared_limiter
pub fn construct() -> Result<(), Box<dyn Error>> {
    let partitioned = PartitioningApi::builder().build()?;
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

/// Returns whether an execution failure is recoverable local-state pressure.
#[must_use]
pub fn is_recoverable_state_capacity(error: &concord::ApiError) -> bool {
    error.rate_limit_kind() == Some(concord::rate_limit::RateLimitErrorKind::StateCapacity)
}
