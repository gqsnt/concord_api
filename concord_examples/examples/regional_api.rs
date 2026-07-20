#![forbid(unsafe_code)]

use concord_core::Request;
use concord_macros::api;

api! {
    client RegionalGameApi {
        base "https://regional-game.example.test/";
        var platform: String;
        credentials {
            secret api_key: String;
            credential developer_key = api_key(secret.api_key);
        }
        host [vars.platform, "api"];
        path ["game"];
        headers { "X-Developer-Key" = "example-only"; }
        auth header "X-Game-Token" = developer_key challenge never;
        rate_limits {
            application { by [host]; 20 / 1s; 100 / 2m; }
            method { by [host, endpoint]; 10 / 10s; }
        }
        rate_limit application;

        scope Accounts(region: String) {
            host [scope.region, "accounts"];
            path ["accounts"];
            GET ById(account_id: String) -> Json<Vec<String>> {
                path ["by-id", ep.account_id];
                rate_limit method;
            }
            GET ByName(game_name: String, tag_line: String) -> Json<Vec<String>> {
                path ["by-name", ep.game_name, ep.tag_line];
                rate_limit method;
            }
        }

        scope Matches(region: String) {
            host [scope.region, "matches"];
            path ["matches"];
            GET ById(match_id: String) -> Json<Vec<String>> {
                path [ep.match_id];
                rate_limit method;
            }
            GET ByAccount(account_id: String, start: u64, count: std::num::NonZeroU32) -> Json<Vec<String>> {
                path ["by-account", ep.account_id];
                query { "start" = ep.start; "count" = ep.count; }
                rate_limit method;
                paginate concord_core::advanced::OffsetPagination {
                    offset: bind ep.start,
                    page_size: bind ep.count,
                }
            }
            GET Timeline(match_id: String) -> Json<Vec<String>> {
                path [ep.match_id, "timeline"];
                rate_limit method;
            }
        }

        scope League(region: String) {
            host [scope.region, "league"];
            path ["league"];
            GET Challenger(queue: String) -> Json<Vec<String>> {
                path ["challenger"];
                query { "queue" = ep.queue; }
                rate_limit method;
            }
            GET Grandmaster(queue: String) -> Json<Vec<String>> {
                path ["grandmaster"];
                query { "queue" = ep.queue; }
                rate_limit method;
            }
            GET Master(queue: String) -> Json<Vec<String>> {
                path ["master"];
                query { "queue" = ep.queue; }
                rate_limit method;
            }
        }
    }
}

fn main() -> Result<(), concord_core::prelude::ClientBuildError> {
    let client = RegionalGameApi::new("euw1".to_owned(), "example-developer-key")?;
    let _: Request<_> = client
        .accounts("europe".to_owned())
        .by_id("account-42".to_owned());
    let _: Request<_> = client
        .matches("europe".to_owned())
        .by_id("match-42".to_owned());
    let _: Request<_> = client
        .league("euw1".to_owned())
        .challenger("ranked-solo".to_owned());
    Ok(())
}
