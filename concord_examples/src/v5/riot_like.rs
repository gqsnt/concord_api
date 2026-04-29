use concord_core::prelude::*;
use concord_macros::api;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug)]
pub enum Region {
    Europe,
}

impl core::fmt::Display for Region {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Region::Europe => f.write_str("europe"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub puuid: String,
    pub game_name: String,
}

api! {
    client RiotLikeApi {
        base https "riotgames.com"
        secret api_key: String
        credential key = api_key(secret.api_key)

        default {
            retry read
            rate_limit app
        }

        retry read {
            max_attempts 2
            methods [GET]
            on [429, 500, 502, 503, 504]
            retry_after
        }

        rate_limit app {
            bucket application by [host] {
                500 / 10s
            }
        }
    }

    scope regional(region: Region) {
        host [region, "api"]
        auth header "X-Riot-Token" = key

        scope accounts {
            path ["riot", "account", "v1", "accounts"]

            GET GetAccountByRiotId(game_name: String, tag_line: String)
                as by_riot_id
                path ["by-riot-id", game_name, tag_line]
                -> Json<Account>
        }
    }
}

pub async fn facade_usage(api_key: String) -> Result<Account, ApiClientError> {
    let api = riot_like_api::RiotLikeApi::new(api_key);

    api.regional(Region::Europe)
        .accounts()
        .by_riot_id("Random Iron".to_string(), "EUVV".to_string())
        .await
}
