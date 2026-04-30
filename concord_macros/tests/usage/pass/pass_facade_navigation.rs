use concord_core::prelude::*;
use concord_macros::api;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct MatchDto;
use self::usage_facade_api::UsageFacadeApi;

api! {
    client UsageFacadeApi { base "https://example.com" }

    scope regional(region: String) {
        host [region, "api"]

        scope match_api_matches {
            path ["lol", "match", "api", "matches"]

            GET GetMatchIdsByPuuid(puuid: String)
                as ids_by_puuid
                path ["by-puuid", puuid, "ids"]
                -> Json<Vec<String>>

            GET GetMatch(match_id: String)
                path [match_id]
                -> Json<MatchDto>
        }
    }
}

async fn facade_usage(api: UsageFacadeApi) -> Result<(), ApiClientError> {
    let _ids = api
        .regional("americas".to_string())
        .match_api_matches()
        .ids_by_puuid("puuid".to_string())
        .await?;

    let _match = api
        .regional("americas".to_string())
        .match_api_matches()
        .get_match("match-id".to_string())
        .await?;

    Ok(())
}

fn main() {}

