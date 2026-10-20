//! Compile-checked bucket-partitioning and local-state composition examples.
//!
//! Required feature: `rate-limit`. Mode: construction and deterministic local
//! execution. Documentation owner: `docs/rate_limits.md`. Proving tests:
//! `tests/construction.rs` and `tests/local_runtime.rs`.
//!
//! The root policy partitions by the built-in `endpoint` dimension using an
//! explicitly pinned operational ID (IDENTITY-01). Inline dimensions have
//! explicit labels, so their names form part of the bucket contract.

use std::error::Error;
use std::sync::Arc;

use concord::rate_limit::{LocalRateLimiter, RateLimiter};
use concord::{Binary, Bytes, api};

// ANCHOR: rate_partitioning_recipe
// ANCHOR: partitioning_contract
api! {
    client PartitioningApi {
        base "http://partitioning.example.test/";
        timeout 30s;
        rate_limit {
            id "concord.partitioning.1";
            key [endpoint];
            cost 1;
            allow 100 per 1m;
        }

        scope regions(region: String) {
            subdomain replace [region, "api"];

            scope accounts(account: String) {
                rate_limit {
                    id "concord.partitioning.2";
                    key ["accounts.account" = account];
                    cost 1;
                    allow 100 per 1m;
                }

                GET channel(channel: String) -> Bytes via Binary {
                    id "partitioning_api.regions.accounts.channel";
                    rate_limit {
                        id "concord.partitioning.3";
                        key ["endpoint.channel" = channel];
                        cost 1;
                        allow 20 per 1s;
                    }
                }

                GET composite(account: String) -> Bytes via Binary {
                    id "partitioning_api.regions.accounts.composite";
                    rate_limit {
                        id "concord.partitioning.4";
                        key [
                            origin,
                            "regions.region" = regions.region,
                            "endpoint.account" = account,
                        ];
                        cost 2;
                        allow 100 per 1m;
                    }
                }
            }
        }
    }
}
// ANCHOR_END: partitioning_contract

api! {
    client ResourceStateApi {
        base "http://resource-state.example.test/";
        timeout 30s;

        GET channel(channel: String) -> Bytes via Binary {
            rate_limit {
                id "concord.partitioning.5";
                key ["endpoint.channel" = channel];
                cost 1;
                allow 2 per 1h;
            }
        }
    }
}

api! {
    client SharedStateApi {
        base "http://shared-state.example.test/";
        timeout 30s;
        rate_limit { id "concord.partitioning.6"; key [origin]; cost 1; allow 1 per 1h; }

        GET ping() -> Bytes via Binary;
    }
}

/// Constructs every partition shape and two clients sharing one limiter.
///
/// Ordinary clones of either built client also share its immutable runtime
/// snapshot and limiter object.
// ANCHOR: shared_limiter
pub fn construct() -> Result<(), Box<dyn Error>> {
    let partitioned = PartitioningApi::builder()
        .configure_runtime(|runtime| runtime.rate_limiter(Arc::new(LocalRateLimiter::new())))
        .build()?;
    let account = partitioned.regions("eu").accounts("account-42");
    let _: partitioning_api::regions::accounts::ChannelRequest = account.channel("alerts");
    let _: partitioning_api::regions::accounts::CompositeRequest = account.composite("resource-7");

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
    let _: resource_state_api::ChannelRequest = high_cardinality.channel("first-key");
    Ok(())
}
// ANCHOR_END: shared_limiter
// ANCHOR_END: rate_partitioning_recipe

/// Returns whether an execution failure is recoverable local-state pressure.
#[must_use]
pub fn is_recoverable_state_capacity(error: &concord::ApiError) -> bool {
    error.rate_limit_kind() == Some(concord::rate_limit::RateLimitErrorKind::StateCapacity)
}
