//! Multi-host scopes with inherited authentication and lexical limits.
//!
//! Required features: `json` and, for execution, a rate limiter.
//! Mode: deterministic construction and safe planning.
//! Documentation owner: `docs/recipes/regional-apis.md`.
//! Proving tests: `tests/construction.rs` and `tests/local_runtime.rs`.

use concord::api;

api! {
    client RegionalGameApi {
        base "https://regional-game.example.test/";
        vars {
            platform: String;
        }
        credentials {
            secret api_key: String;
            credential developer_key = api_key(secret.api_key);
        }
        host [vars.platform, "api"];
        path ["game"];
        headers { "X-Developer-Key" = "example-only"; }
        auth header "X-Game-Token" = developer_key challenge never;
        rate_limit { 20 / 1s; 100 / 2m; }
    }

    scope accounts(region: String) {
        host [scope.region, "accounts"];
        path ["accounts"];
        GET by_id(account_id: String) -> Json<Vec<String>> {
            path ["by-id", ep.account_id];
            rate_limit { 10 / 10s; }
        }
        GET by_name(game_name: String, tag_line: String) -> Json<Vec<String>> {
            path ["by-name", ep.game_name, ep.tag_line];
            rate_limit { 10 / 10s; }
        }
    }

    scope matches(region: String) {
        host [scope.region, "matches"];
        path ["matches"];
        GET by_id(match_id: String) -> Json<Vec<String>> {
            path [ep.match_id];
            rate_limit { 10 / 10s; }
        }
        GET by_account(account_id: String, start: u64, count: std::num::NonZeroU32) -> Json<Vec<String>> {
            path ["by-account", ep.account_id];
            query { "start" = ep.start; "count" = ep.count; }
            rate_limit { 10 / 10s; }
            paginate concord::pagination::OffsetPagination {
                offset: bind ep.start,
                page_size: bind ep.count,
            }
        }
        GET timeline(match_id: String) -> Json<Vec<String>> {
            path [ep.match_id, "timeline"];
            rate_limit { 10 / 10s; }
        }
    }

    scope league(region: String) {
        host [scope.region, "league"];
        path ["league"];
        GET challenger(queue: String) -> Json<Vec<String>> {
            path ["challenger"];
            query { "queue" = ep.queue; }
            rate_limit { 10 / 10s; }
        }
        GET grandmaster(queue: String) -> Json<Vec<String>> {
            path ["grandmaster"];
            query { "queue" = ep.queue; }
            rate_limit { 10 / 10s; }
        }
        GET master(queue: String) -> Json<Vec<String>> {
            path ["master"];
            query { "queue" = ep.queue; }
            rate_limit { 10 / 10s; }
        }
    }
}

/// Constructs representative requests from each regional scope.
pub fn construct() -> Result<(), concord::prelude::ClientBuildError> {
    let client = RegionalGameApi::builder()
        .set_platform("euw1".to_owned())
        .api_key(
            concord::SecretString::try_new("example-developer-key")
                .expect("bounded example developer key"),
        )
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
