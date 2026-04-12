// Path: ".\\concord_examples\\src\\riot.rs" (DSL migrated to evolution-style scope/params/host/path)
use concord_core::prelude::*;
use concord_macros::api;
use serde::{Deserialize, Serialize};

api! {
    client RiotClient {
        scheme: https,
        host: "riotgames.com",
        secret {
            api_key: String
        }
        auth {
            credential riot_api_key: ApiKey(secret.api_key)
        }
        headers {
            "user-agent" = "ClientApiRiotExample/1.0",
            "x-client-trace" = false
        }
    }

    scope platform {
        use_auth HeaderAuth("X-Riot-Token", riot_api_key)

        params {
            platform: PlatformRoute
        }
        host[platform, "api"]
        path["lol"]

        scope summoner_v4 {
            path["summoner", "v4", "summoners"]

            GET GetSummonerByPuuid {
                params { puuid: String }
                path["by-puuid", puuid]
                -> Json<models::SummonerDto>;
            }

            GET GetSummonerById {
                params { summoner_id: String }
                path[summoner_id]
                -> Json<models::SummonerDto>;
            }

            GET GetSummonerByName {
                params { summoner_name: String }
                path["by-name", summoner_name]
                -> Json<models::SummonerDto>;
            }
        }

        scope champion_mastery_v4 {
            path["champion-mastery", "v4"]

            scope champion_masteries {
                path["champion-masteries"]

                GET GetChampionMasteriesBySummoner {
                    params { summoner_id: String }
                    path["by-summoner", summoner_id]
                    -> Json<Vec<models::ChampionMasteryDto>>;
                }

                GET GetChampionMasteryByChampion {
                    params {
                        summoner_id: String,
                        champion_id: i64
                    }
                    path["by-summoner", summoner_id, "by-champion", champion_id]
                    -> Json<models::ChampionMasteryDto>;
                }
            }

            scope scores {
                path["scores"]

                GET GetChampionMasteryScore {
                    params { summoner_id: String }
                    path["by-summoner", summoner_id]
                    -> Json<i32>;
                }
            }
        }

        scope league_v4 {
            path["league", "v4"]

            scope challengerleagues {
                path["challengerleagues"]

                GET GetChallengerLeagueByQueue {
                    params { queue: LeagueQueue }
                    path["by-queue", queue]
                    -> Json<models::LeagueListDto>;
                }
            }

            scope grandmasterleagues {
                path["grandmasterleagues"]

                GET GetGrandmasterLeagueByQueue {
                    params { queue: LeagueQueue }
                    path["by-queue", queue]
                    -> Json<models::LeagueListDto>;
                }
            }

            scope masterleagues {
                path["masterleagues"]

                GET GetMasterLeagueByQueue {
                    params { queue: LeagueQueue }
                    path["by-queue", queue]
                    -> Json<models::LeagueListDto>;
                }
            }

            scope leagues {
                path["leagues"]

                GET GetLeagueById {
                    params { league_id: String }
                    path[league_id]
                    -> Json<models::LeagueListDto>;
                }
            }

            scope entries {
                path["entries"]

                GET GetLeagueEntriesBySummoner {
                    params { summoner_id: String }
                    path["by-summoner", summoner_id]
                    -> Json<Vec<models::LeagueEntryDto>>;
                }

                GET GetLeagueEntries {
                    params {
                        queue: String,
                        tier: String,
                        division: String,
                        page?: u32
                    }
                    path[queue, tier, division]
                    query {
                        page = page
                    }
                    -> Json<Vec<models::LeagueEntryDto>>;
                }
            }
        }

        scope spectator_v4 {
            path["spectator", "v4"]

            scope featured_games {
                path["featured-games"]

                GET GetFeaturedGames {
                    -> Json<models::FeaturedGamesDto>;
                }
            }

            scope active_games_by_summoner {
                path["active-games", "by-summoner"]

                GET GetCurrentGameInfoBySummoner {
                    params { summoner_id: String }
                    path[summoner_id]
                    -> Json<models::CurrentGameInfoDto>;
                }
            }
        }

        scope status_v4 {
            path["status", "v4"]

            GET GetPlatformData {
                path["platform-data"]
                -> Json<models::PlatformDataDto>;
            }
        }
    }

    scope regional {
        params {
            region: RegionalRoute
        }
        host[region, "api"]

        scope account_v1_accounts {
            path["riot", "account", "v1", "accounts"]

            GET GetAccountByRiotId {
                params {
                    game_name: String,
                    tag_line: String
                }
                path["by-riot-id", game_name, tag_line]
                -> Json<models::AccountDto>;
            }

            GET GetAccountByPuuid {
                params { puuid: String }
                path["by-puuid", puuid]
                -> Json<models::AccountDto>;
            }
        }

        scope account_v1_active_shards {
            path["riot", "account", "v1", "active-shards"]

            GET GetActiveShardByGameAndPuuid {
                params {
                    game: String,
                    puuid: String
                }
                path["by-game", game, "by-puuid", puuid]
                -> Json<models::ActiveShardDto>;
            }
        }

        scope match_v5_matches {
            path["lol", "match", "v5", "matches"]

            GET GetMatchIdsByPuuid {
                params {
                    puuid: String,
                    queue?: u16,
                    start_time?: i64,
                    end_time?: i64,
                    start: u64 = 0,
                    count: u64 = 20
                }
                path["by-puuid", puuid, "ids"]
                query {
                    queue = queue,
                    "startTime" = start_time,
                    "endTime" = end_time,
                    start = start,
                    count = count
                }
                paginate OffsetLimitPagination {
                    offset = start,
                    limit = count
                }
                -> Json<Vec<String>>;
            }

            GET GetMatch {
                params { match_id: String }
                path[match_id]
                -> Json<models::MatchDto>;
            }

            GET GetTimeline {
                params { match_id: String }
                path[match_id, "timeline"]
                -> Json<models::TimelineDto>;
            }
        }
    }
}

api! {
    client DDragonClient {
        scheme: https,
        host: "leagueoflegends.com",
        headers {
            "user-agent" = "ClientApiDDragonExample/1.0"
        }
    }

    scope ddragon {
        host["ddragon"]

        scope api_root {
            path["api"]

            GET GetVersions {
                path["versions.json"]
                -> Json<Vec<String>>;
            }

            GET GetRealmByRegion {
                params { region: String }
                path["realms", region]
                -> Json<models::RealmDto>;
            }
        }

        scope cdn_root {
            path["cdn"]

            GET GetLanguages {
                path["languages.json"]
                -> Json<Vec<String>>;
            }
        }

        scope cdn_versioned {
            params { version: String }
            path["cdn", version]

            scope data_localized {
                params {
                    locale: String = "en_US".to_string()
                }
                path["data", locale]

                scope champion {
                    path["champion"]

                    GET GetChampionList {
                        path["champion.json"]
                        -> Json<models::ChampionListDto>;
                    }

                    GET GetChampionFull {
                        path["championFull.json"]
                        -> Json<serde_json::Value>;
                    }

                    GET GetChampionDetail {
                        params { champion: String }
                        path[champion]
                        -> Json<models::ChampionDetailDto>;
                    }
                }

                GET GetSummonerSpells {
                    path["summoner.json"]
                    -> Json<models::SummonerSpellListDto>;
                }

                GET GetItems {
                    path["item.json"]
                    -> Json<models::ItemListDto>;
                }

                GET GetRunesReforged {
                    path["runesReforged.json"]
                    -> Json<models::RunesReforgedDto>;
                }
            }
        }
    }
}
/// Platform routing values (LoL).
/// Ex: euw1.api.riotgames.com
#[derive(Clone, Copy, Debug)]
pub enum PlatformRoute {
    BR1,
    EUN1,
    EUW1,
    JP1,
    KR,
    LA1,
    LA2,
    NA1,
    OC1,
    PH2,
    RU,
    SG2,
    TH2,
    TR1,
    TW2,
    VN2,
    // ME1, // décommenter si nécessaire (selon jeu/API)
}
impl core::fmt::Display for PlatformRoute {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let s = match self {
            PlatformRoute::BR1 => "br1",
            PlatformRoute::EUN1 => "eun1",
            PlatformRoute::EUW1 => "euw1",
            PlatformRoute::JP1 => "jp1",
            PlatformRoute::KR => "kr",
            PlatformRoute::LA1 => "la1",
            PlatformRoute::LA2 => "la2",
            PlatformRoute::NA1 => "na1",
            PlatformRoute::OC1 => "oc1",
            PlatformRoute::PH2 => "ph2",
            PlatformRoute::RU => "ru",
            PlatformRoute::SG2 => "sg2",
            PlatformRoute::TH2 => "th2",
            PlatformRoute::TR1 => "tr1",
            PlatformRoute::TW2 => "tw2",
            PlatformRoute::VN2 => "vn2",
        };
        f.write_str(s)
    }
}

/// Regional routing values (LoL account-v1, match-v5).
/// Ex: europe.api.riotgames.com
#[derive(Clone, Copy, Debug)]
pub enum RegionalRoute {
    Americas,
    Europe,
    Asia,
    Sea,
}
impl core::fmt::Display for RegionalRoute {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let s = match self {
            RegionalRoute::Americas => "americas",
            RegionalRoute::Europe => "europe",
            RegionalRoute::Asia => "asia",
            RegionalRoute::Sea => "sea",
        };
        f.write_str(s)
    }
}

/// Quelques queues utiles pour league-v4.
#[derive(Clone, Copy, Debug)]
pub enum LeagueQueue {
    RankedSolo5x5,
    RankedFlexSr,
    RankedFlexTt,
}
impl core::fmt::Display for LeagueQueue {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let s = match self {
            LeagueQueue::RankedSolo5x5 => "RANKED_SOLO_5x5",
            LeagueQueue::RankedFlexSr => "RANKED_FLEX_SR",
            LeagueQueue::RankedFlexTt => "RANKED_FLEX_TT",
        };
        f.write_str(s)
    }
}

pub mod models {
    use super::*;
    use serde_json::Value;
    use std::collections::HashMap;

    // ---------------------------
    // ACCOUNT-V1 (regional routing)
    // ---------------------------
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct AccountDto {
        pub puuid: String,
        pub game_name: String,
        pub tag_line: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct ActiveShardDto {
        pub puuid: String,
        pub game: String,
        pub active_shard: String,
    }

    // ---------------------------
    // SUMMONER-V4 (platform routing)
    // ---------------------------
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct SummonerDto {
        pub id: String,         // encryptedSummonerId
        pub account_id: String, // encryptedAccountId
        pub puuid: String,
        pub name: String,
        pub profile_icon_id: u32,
        pub revision_date: i64,
        pub summoner_level: u64,
    }

    // ---------------------------
    // CHAMPION-MASTERY-V4
    // ---------------------------
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct ChampionMasteryDto {
        pub champion_id: i64,
        pub champion_level: i32,
        pub champion_points: i64,
        pub last_play_time: i64,
        pub champion_points_since_last_level: i64,
        pub champion_points_until_next_level: i64,
        pub chest_granted: bool,
        pub tokens_earned: i32,
        pub summoner_id: String, // encryptedSummonerId
    }

    // ---------------------------
    // LEAGUE-V4
    // ---------------------------
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct MiniSeriesDto {
        pub losses: i32,
        pub progress: String,
        pub target: i32,
        pub wins: i32,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct LeagueEntryDto {
        pub league_id: String,
        pub summoner_id: String,
        pub summoner_name: String,
        pub queue_type: String,
        pub tier: String,
        pub rank: String,
        pub league_points: i32,
        pub wins: i32,
        pub losses: i32,
        pub hot_streak: bool,
        pub veteran: bool,
        pub fresh_blood: bool,
        pub inactive: bool,
        pub mini_series: Option<MiniSeriesDto>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct LeagueItemDto {
        pub summoner_id: String,
        pub summoner_name: String,
        pub league_points: i32,
        pub rank: String,
        pub wins: i32,
        pub losses: i32,
        pub veteran: bool,
        pub inactive: bool,
        pub fresh_blood: bool,
        pub hot_streak: bool,
        pub mini_series: Option<MiniSeriesDto>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct LeagueListDto {
        pub league_id: String,
        pub tier: String,
        pub name: String,
        pub queue: String,
        pub entries: Vec<LeagueItemDto>,
    }

    // ---------------------------
    // SPECTATOR-V4 (réponses larges => Value)
    // ---------------------------
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct FeaturedGamesDto {
        #[serde(flatten)]
        pub raw: Value,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct CurrentGameInfoDto {
        #[serde(flatten)]
        pub raw: Value,
    }

    // ---------------------------
    // STATUS-V4 (réponse large => Value)
    // ---------------------------
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct PlatformDataDto {
        #[serde(flatten)]
        pub raw: Value,
    }

    // ---------------------------
    // MATCH-V5 (très large => top-level + Value)
    // ---------------------------
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct MatchDto {
        pub metadata: HashMap<String, Value>,
        pub info: HashMap<String, Value>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct TimelineDto {
        #[serde(flatten)]
        pub raw: Value,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct RealmDto {
        pub v: String,
        pub l: String,
        pub cdn: String,
        pub dd: String,
        pub lg: String,
        pub css: String,
        pub profileiconmax: u32,
        pub store: Option<String>,
    }

    // /cdn/{version}/data/{locale}/champion.json
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct ChampionListDto {
        #[serde(rename = "type")]
        pub ty: String,
        pub format: String,
        pub version: String,
        pub data: HashMap<String, ChampionBriefDto>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct ChampionBriefDto {
        pub version: String,
        pub id: String,
        pub key: String,
        pub name: String,
        pub title: String,
        pub blurb: String,
        #[serde(default)]
        pub tags: Vec<String>,
        #[serde(default)]
        pub info: Option<Value>,
        #[serde(default)]
        pub image: Option<Value>,
    }

    // /cdn/{version}/data/{locale}/champion/{champion}.json (très large)
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ChampionDetailDto {
        #[serde(flatten)]
        pub raw: Value,
    }

    // /cdn/{version}/data/{locale}/summoner.json
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct SummonerSpellListDto {
        #[serde(rename = "type")]
        pub ty: String,
        pub version: String,
        pub data: HashMap<String, Value>,
    }

    // /cdn/{version}/data/{locale}/item.json (très large)
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ItemListDto {
        #[serde(flatten)]
        pub raw: Value,
    }

    // /cdn/{version}/data/{locale}/runesReforged.json
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct RunesReforgedDto(pub Value);
}

pub async fn test_riot() -> Result<(), ApiClientError> {
    dotenvy::dotenv().ok();
    let api_key = dotenvy::var("RIOT_API_KEY").expect("RIOT_API_KEY missing");

    let riot = riot_client::RiotClient::new(api_key);
    let default_region = RegionalRoute::Europe;

    let account = riot
        .request(riot_client::endpoints::GetAccountByRiotId::new(
            "Random Iron".to_string(),
            default_region,
            "EUVV".to_string(),
        ))
        .execute()
        .await?;
    println!(
        "Account: {}#{} puuid={}",
        account.game_name, account.tag_line, account.puuid
    );

    let match_ids = riot
        .request(
            riot_client::endpoints::GetMatchIdsByPuuid::new(account.puuid.clone(), default_region)
                .count(100),
        )
        .paginate()
        .max_items(10_000)
        .collect()
        .await?;
    println!("match_ids Len: {:?}", match_ids.len());

    Ok(())
}
