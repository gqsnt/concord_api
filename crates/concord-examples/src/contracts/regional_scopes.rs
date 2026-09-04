//! Multi-host scopes with inherited authentication and lexical limits.
//!
//! Required features: `json` and, for execution, a rate limiter.
//! Mode: deterministic construction and credential-free planning.
//! Documentation owner: `docs/spec.md`.
//! Proving tests: `tests/construction.rs` and `tests/local_runtime.rs`.

use std::sync::Arc;

use concord::api;
use concord::rate_limit::NoopRateLimiter;

// ANCHOR: regional_api_recipe
api! {
    client RegionalGameApi {
        timeout 30s;
        base "https://regional-game.example.test/";
        values {
            platform: String;
        }
        credentials {
            secret api_key;
            credential developer_key = api_key(secret.api_key);
        }
        subdomain replace [client.platform, "api"];
        path ["game"];
        headers { "X-Developer-Key" = "example-only"; }
        auth { header "X-Game-Token" = developer_key; }
        rate_limit { id "concord.regional_scopes.1"; key [origin]; cost 1;  allow 20 per 1s; allow 100 per 2m; }


    scope accounts(region: String) {
        subdomain replace [accounts.region, "accounts"];
        path ["accounts"];
        GET by_id(account_id: String) -> Vec<String> via Json {
            path ["by-id", endpoint.account_id];
            rate_limit { id "concord.regional_scopes.2"; key [origin]; cost 1;  allow 10 per 10s; }
        }
        GET by_name(game_name: String, tag_line: String) -> Vec<String> via Json {
            path ["by-name", endpoint.game_name, endpoint.tag_line];
            rate_limit { id "concord.regional_scopes.3"; key [origin]; cost 1;  allow 10 per 10s; }
        }
    }

    scope matches(region: String) {
        subdomain replace [matches.region, "matches"];
        path ["matches"];
        GET by_id(match_id: String) -> Vec<String> via Json {
            path [endpoint.match_id];
            rate_limit { id "concord.regional_scopes.4"; key [origin]; cost 1;  allow 10 per 10s; }
        }
        GET by_account(account_id: String, start: u64, count: std::num::NonZeroU32) -> Vec<String> via Json {
            path ["by-account", endpoint.account_id];
            query { "start" = endpoint.start; "count" = endpoint.count; }
            rate_limit { id "concord.regional_scopes.5"; key [origin]; cost 1;  allow 10 per 10s; }
            paginate via concord::pagination::OffsetPagination {
                offset -> endpoint.start;
                page_size -> endpoint.count;
            }
        }
        GET timeline(match_id: String) -> Vec<String> via Json {
            path [endpoint.match_id, "timeline"];
            rate_limit { id "concord.regional_scopes.6"; key [origin]; cost 1;  allow 10 per 10s; }
        }
    }

    scope league(region: String) {
        subdomain replace [league.region, "league"];
        path ["league"];
        GET challenger(queue: String) -> Vec<String> via Json {
            path ["challenger"];
            query { "queue" = endpoint.queue; }
            rate_limit { id "concord.regional_scopes.7"; key [origin]; cost 1;  allow 10 per 10s; }
        }
        GET grandmaster(queue: String) -> Vec<String> via Json {
            path ["grandmaster"];
            query { "queue" = endpoint.queue; }
            rate_limit { id "concord.regional_scopes.8"; key [origin]; cost 1;  allow 10 per 10s; }
        }
        GET master(queue: String) -> Vec<String> via Json {
            path ["master"];
            query { "queue" = endpoint.queue; }
            rate_limit { id "concord.regional_scopes.9"; key [origin]; cost 1;  allow 10 per 10s; }
        }
    }
}}
// ANCHOR_END: regional_api_recipe

/// Constructs representative requests from each regional scope.
// ANCHOR: regional_api_usage
pub fn construct() -> Result<(), concord::prelude::ClientBuildError> {
    let client = RegionalGameApi::builder()
        .set_platform("euw1".to_owned())
        .api_key(
            concord::SecretString::try_new("example-developer-key")
                .expect("bounded example developer key"),
        )
        // Planning is the only operation in this recipe; the explicit test
        // limiter keeps rate enforcement a required construction decision.
        .configure_runtime(|runtime| runtime.rate_limiter(Arc::new(NoopRateLimiter::new())))
        .build()?;
    let _: regional_game_api::accounts::ByIdRequest = client
        .accounts("europe".to_owned())
        .by_id("account-42".to_owned());
    let _: regional_game_api::matches::ByIdRequest = client
        .matches("europe".to_owned())
        .by_id("match-42".to_owned());
    let _: regional_game_api::league::ChallengerRequest = client
        .league("euw1".to_owned())
        .challenger("ranked-solo".to_owned());
    Ok(())
}
// ANCHOR_END: regional_api_usage
