//! Multi-host scopes with inherited authentication and lexical limits.
//!
//! Required features: `json` and, for execution, a rate limiter.
//! Mode: deterministic construction and credential-free planning.
//! Documentation owner: `docs/spec.md`.
//! Proving tests: `tests/construction.rs` and `tests/local_runtime.rs`.
//!
//! Each scope replaces the generated DNS labels (`subdomain replace`), so the
//! root `platform` label is suppressed at every static leaf; it stays live in
//! the root owner's runtime-binding projection (VALUE-06).

use std::num::NonZeroU32;
use std::sync::Arc;

use concord::pagination::{OffsetInit, OffsetPagination};
use concord::rate_limit::NoopRateLimiter;
use concord::{Json, api};

// ANCHOR: regional_api_recipe
api! {
    client RegionalGameApi {
        base "https://regional-game.example.test/";
        timeout 30s;

        values {
            platform: String;
        }

        credentials {
            secret api_key;
            credential developer_key = api_key(secret.api_key);
        }

        subdomain replace [platform, "api"];
        path "game";
        headers { "X-Developer-Key" = "example-only"; }
        auth { header "X-Game-Token" = developer_key; }
        rate_limit {
            id "concord.regional_scopes.1";
            key [origin];
            cost 1;
            allow 20 per 1s;
            allow 100 per 2m;
        }

        scope accounts(region: String) {
            subdomain replace [region, "accounts"];
            path "accounts";

            GET by_id(account_id: String) -> Vec<String> via Json {
                path "by-id/{account_id}";
                rate_limit { id "concord.regional_scopes.2"; key [origin]; cost 1; allow 10 per 10s; }
            }

            GET by_name(game_name: String, tag_line: String) -> Vec<String> via Json {
                path "by-name/{game_name}/{tag_line}";
                rate_limit { id "concord.regional_scopes.3"; key [origin]; cost 1; allow 10 per 10s; }
            }
        }

        scope matches(region: String) {
            subdomain replace [region, "matches"];
            path "matches";

            GET by_id(match_id: String) -> Vec<String> via Json {
                path "{match_id}";
                rate_limit { id "concord.regional_scopes.4"; key [origin]; cost 1; allow 10 per 10s; }
            }

            GET by_account(
                account_id: String,
                query start: u64,
                query count: NonZeroU32,
            ) -> Vec<String> via Json {
                path "by-account/{account_id}";
                rate_limit { id "concord.regional_scopes.5"; key [origin]; cost 1; allow 10 per 10s; }
                paginate via OffsetPagination {
                    init OffsetInit { offset: start, page_size: count };
                    cursor -> start;
                }
            }

            GET timeline(match_id: String) -> Vec<String> via Json {
                path "{match_id}/timeline";
                rate_limit { id "concord.regional_scopes.6"; key [origin]; cost 1; allow 10 per 10s; }
            }
        }

        scope league(region: String) {
            subdomain replace [region, "league"];
            path "league";

            GET challenger(query queue: String) -> Vec<String> via Json {
                path "challenger";
                rate_limit { id "concord.regional_scopes.7"; key [origin]; cost 1; allow 10 per 10s; }
            }

            GET grandmaster(query queue: String) -> Vec<String> via Json {
                path "grandmaster";
                rate_limit { id "concord.regional_scopes.8"; key [origin]; cost 1; allow 10 per 10s; }
            }

            GET master(query queue: String) -> Vec<String> via Json {
                path "master";
                rate_limit { id "concord.regional_scopes.9"; key [origin]; cost 1; allow 10 per 10s; }
            }
        }
    }
}
// ANCHOR_END: regional_api_recipe

/// Constructs representative requests from each regional scope.
// ANCHOR: regional_api_usage
pub fn construct() -> Result<(), concord::prelude::ClientBuildError> {
    let client = RegionalGameApi::builder()
        .set_platform("euw1")
        .api_key(
            concord::SecretString::try_new("example-developer-key")
                .expect("bounded example developer key"),
        )
        // Planning is the only operation in this recipe; the explicit test
        // limiter keeps rate enforcement a required construction decision.
        .configure_runtime(|runtime| runtime.rate_limiter(Arc::new(NoopRateLimiter::new())))
        .build()?;
    let _: regional_game_api::accounts::ByIdRequest = client.accounts("europe").by_id("account-42");
    let _: regional_game_api::matches::ByIdRequest = client.matches("europe").by_id("match-42");
    let _: regional_game_api::league::ChallengerRequest =
        client.league("euw1").challenger("ranked-solo");
    Ok(())
}
// ANCHOR_END: regional_api_usage
