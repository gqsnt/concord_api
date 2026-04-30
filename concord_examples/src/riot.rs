// Path: ".\\concord_examples\\src\\riot.rs" (DSL migrated to evolution-style scope/params/host/path)
use concord_core::prelude::*;
use concord_macros::api;
use serde::{Deserialize, Serialize};

#[derive(Default)]
pub struct RiotRateLimitHeaders;

impl RateLimitObserver for RiotRateLimitHeaders {
    fn observe(&self, ctx: RateLimitResponseContext<'_>) -> RateLimitObservation {
        ctx.on_429().scope_header("x-rate-limit-type").retry_after()
    }
}

api! {
    client RiotClient {
        base "https://riotgames.com"
        secret api_key: String
        credential riot_api_key = api_key(secret.api_key)
        headers {
            "user-agent" = "ClientApiRiotExample/1.0",
            "x-client-trace" = false
        }
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
        observe rate_limit RiotRateLimitHeaders
        rate_limit app {
                bucket application by [host] {
                    500 / 10s
                    30000 / 10m
                }
        }
        rate_limit summoner_by_puuid {
                bucket method by [host, endpoint] {
                    1600 / 1m
                }
        }
        rate_limit league_queue_slow {
                bucket method by [host, endpoint] {
                    30 / 10s
                    500 / 10m
                }
        }
        rate_limit league_by_id {
                bucket method by [host, endpoint] {
                    500 / 10s
                }
        }
        rate_limit league_entries {
                bucket method by [host, endpoint] {
                    50 / 10s
                }
        }
        rate_limit clash_team_or_by_team {
                bucket method by [host, endpoint] {
                    200 / 1m
                }
        }
        rate_limit clash_tournament_lookup {
                bucket method by [host, endpoint] {
                    10 / 1m
                }
        }
        rate_limit account_standard_method {
                bucket method by [host, endpoint] {
                    1000 / 1m
                }
        }
        rate_limit riot_high_volume_method {
                bucket method by [host, endpoint] {
                    20000 / 10s
                    1200000 / 10m
                }
        }
        rate_limit match_v5_method {
                bucket method by [host, endpoint] {
                    2000 / 10s
                }
        }
    }

    scope platform(platform: PlatformRoute) {
        host [platform, "api"]
        path ["lol"]

        auth header "X-Riot-Token" = riot_api_key

        scope champion_v3 {
            path ["platform", "v3"]

            GET GetChampionRotations
            as rotations
            path ["champion-rotations"]
            -> Json<models::ChampionRotationsDto>
            rate_limit league_queue_slow
        }

        scope summoner_v4 {
            path ["summoner", "v4", "summoners"]

            GET GetSummonerByPuuid(puuid: String)
            as by_puuid
            path ["by-puuid", puuid]
            -> Json<models::SummonerDto>
            rate_limit summoner_by_puuid

            GET GetSummonerById(summoner_id: String)
            as by_id
            path [summoner_id]
            -> Json<models::SummonerDto>

            GET GetSummonerByName(summoner_name: String)
            as by_name
            path ["by-name", summoner_name]
            -> Json<models::SummonerDto>
        }

        scope champion_mastery_v4 {
            path ["champion-mastery", "v4"]

            scope champion_masteries {
                path ["champion-masteries"]

                GET GetChampionMasteriesBySummoner(summoner_id: String)
                path ["by-summoner", summoner_id]
                -> Json<Vec<models::ChampionMasteryDto>>
                rate_limit riot_high_volume_method

                GET GetChampionMasteryByChampion(summoner_id: String, champion_id: i64)
                path ["by-summoner", summoner_id, "by-champion", champion_id]
                -> Json<models::ChampionMasteryDto>
                rate_limit riot_high_volume_method

                GET GetChampionMasteriesByPuuid(encrypted_puuid: String)
                as by_puuid
                path ["by-puuid", encrypted_puuid]
                -> Json<Vec<models::ChampionMasteryDto>>
                rate_limit riot_high_volume_method

                GET GetChampionMasteryByPuuidAndChampion(encrypted_puuid: String, champion_id: i64)
                path ["by-puuid", encrypted_puuid, "by-champion", champion_id]
                -> Json<models::ChampionMasteryDto>
                rate_limit riot_high_volume_method

                GET GetTopChampionMasteriesByPuuid(encrypted_puuid: String, count?: u32)
                path ["by-puuid", encrypted_puuid, "top"]
                query {
                    count
                }
                -> Json<Vec<models::ChampionMasteryDto>>
                rate_limit riot_high_volume_method
            }

            scope scores {
                path ["scores"]

                GET GetChampionMasteryScore(summoner_id: String)
                as score
                path ["by-summoner", summoner_id]
                -> Json<i32>
                rate_limit riot_high_volume_method

                GET GetChampionMasteryScoreByPuuid(encrypted_puuid: String)
                path ["by-puuid", encrypted_puuid]
                -> Json<i32>
                rate_limit riot_high_volume_method
            }
        }

        scope league_v4 {
            path ["league", "v4"]

            scope challengerleagues {
                path ["challengerleagues"]

                GET GetChallengerLeagueByQueue(queue: LeagueQueue)
                path ["by-queue", queue]
                -> Json<models::LeagueListDto>
                rate_limit league_queue_slow
            }

            scope grandmasterleagues {
                path ["grandmasterleagues"]

                GET GetGrandmasterLeagueByQueue(queue: LeagueQueue)
                path ["by-queue", queue]
                -> Json<models::LeagueListDto>
                rate_limit league_queue_slow
            }

            scope masterleagues {
                path ["masterleagues"]

                GET GetMasterLeagueByQueue(queue: LeagueQueue)
                path ["by-queue", queue]
                -> Json<models::LeagueListDto>
                rate_limit league_queue_slow
            }

            scope leagues {
                path ["leagues"]

                GET GetLeagueById(league_id: String)
                path [league_id]
                -> Json<models::LeagueListDto>
                rate_limit league_by_id
            }

            scope entries {
                path ["entries"]

                GET GetLeagueEntriesBySummoner(summoner_id: String)
                path ["by-summoner", summoner_id]
                -> Json<Vec<models::LeagueEntryDto>>
                rate_limit riot_high_volume_method

                GET GetLeagueEntriesByPuuid(encrypted_puuid: String)
                path ["by-puuid", encrypted_puuid]
                -> Json<Vec<models::LeagueEntryDto>>
                rate_limit riot_high_volume_method

                GET GetLeagueEntries(queue: String, tier: String, division: String, page?: u32)
                as by_queue
                path [queue, tier, division]
                query {
                    page
                }
                -> Json<Vec<models::LeagueEntryDto>>
                rate_limit league_entries
            }
        }

        scope league_exp_v4 {
            path ["league-exp", "v4", "entries"]

            GET GetLeagueExpEntries(queue: String, tier: String, division: String, page?: u32)
            as by_queue
            path [queue, tier, division]
            query {
                page
            }
            -> Json<Vec<models::LeagueEntryDto>>
            rate_limit league_entries
        }

        scope clash_v1 {
            path ["clash", "v1"]

            GET GetClashTeam(team_id: String)
            path ["teams", team_id]
            -> Json<models::ClashTeamDto>
            rate_limit clash_team_or_by_team

            GET GetClashTournament(tournament_id: i64)
            path ["tournaments", tournament_id]
            -> Json<models::ClashTournamentDto>
            rate_limit clash_tournament_lookup

            GET GetClashTournamentByTeam(team_id: String)
            path ["tournaments", "by-team", team_id]
            -> Json<models::ClashTournamentDto>
            rate_limit clash_team_or_by_team

            GET GetClashTournaments
            path ["tournaments"]
            -> Json<Vec<models::ClashTournamentDto>>
            rate_limit clash_tournament_lookup

            GET GetClashPlayersByPuuid(puuid: String)
            path ["players", "by-puuid", puuid]
            -> Json<Vec<models::ClashPlayerDto>>
            rate_limit riot_high_volume_method
        }

        scope challenges_v1 {
            path ["challenges", "v1"]

            GET GetChallengePercentiles
            path ["challenges", "percentiles"]
            -> Json<serde_json::Value>
            rate_limit riot_high_volume_method

            GET GetChallengeLeaderboardsByLevel(challenge_id: i64, level: String, limit?: u32)
            path ["challenges", challenge_id, "leaderboards", "by-level", level]
            query {
                limit
            }
            -> Json<serde_json::Value>
            rate_limit riot_high_volume_method

            GET GetChallengePercentilesByChallenge(challenge_id: i64)
            path ["challenges", challenge_id, "percentiles"]
            -> Json<serde_json::Value>
            rate_limit riot_high_volume_method

            GET GetChallengeConfig(challenge_id: i64)
            path ["challenges", challenge_id, "config"]
            -> Json<serde_json::Value>
            rate_limit riot_high_volume_method

            GET GetChallengePlayerData(puuid: String)
            path ["player-data", puuid]
            -> Json<serde_json::Value>
            rate_limit riot_high_volume_method

            GET GetChallengeConfigs
            path ["challenges", "config"]
            -> Json<serde_json::Value>
            rate_limit riot_high_volume_method
        }

        scope spectator_v4 {
            path ["spectator", "v4"]

            scope featured_games {
                path ["featured-games"]

                GET GetFeaturedGames
                -> Json<models::FeaturedGamesDto>;
            }

            scope active_games_by_summoner {
                path ["active-games", "by-summoner"]

                GET GetCurrentGameInfoBySummoner(summoner_id: String)
                path [summoner_id]
                -> Json<models::CurrentGameInfoDto>
            }
        }

        scope spectator_v5 {
            path ["spectator", "v5", "active-games"]

            GET GetSpectatorV5ActiveGameBySummoner(encrypted_puuid: String)
            path ["by-summoner", encrypted_puuid]
            -> Json<models::CurrentGameInfoDto>
            rate_limit riot_high_volume_method
        }

        scope status_v4 {
            path ["status", "v4"]

            GET GetPlatformData
            as platform_data
            path ["platform-data"]
            -> Json<models::PlatformDataDto>
            rate_limit riot_high_volume_method
        }
    }

    scope regional(region: RegionalRoute) {
        host [region, "api"]

        scope account_v1_accounts {
            path ["riot", "account", "v1", "accounts"]

            GET GetAccountByRiotId(game_name: String, tag_line: String)
            as by_riot_id
            path ["by-riot-id", game_name, tag_line]
            -> Json<models::AccountDto>
            rate_limit [account_standard_method, riot_high_volume_method]

            GET GetAccountByPuuid(puuid: String)
            as by_puuid
            path ["by-puuid", puuid]
            -> Json<models::AccountDto>
            rate_limit [account_standard_method, riot_high_volume_method]
        }

        scope account_v1_region {
            path ["riot", "account", "v1", "region"]

            GET GetAccountRegionByGameAndPuuid(game: String, puuid: String)
            path ["by-game", game, "by-puuid", puuid]
            -> Json<models::AccountRegionDto>
            rate_limit riot_high_volume_method
        }

        scope account_v1_active_shards {
            path ["riot", "account", "v1", "active-shards"]

            GET GetActiveShardByGameAndPuuid(game: String, puuid: String)
            path ["by-game", game, "by-puuid", puuid]
            -> Json<models::ActiveShardDto>
            rate_limit riot_high_volume_method
        }

        scope match_v5_matches {
            path ["lol", "match", "v5", "matches"]

            GET GetMatchIdsByPuuid(puuid: String, queue?: u16, start_time?: i64, end_time?: i64, start: u64 = 0, count: u64 = 20)
            as ids_by_puuid
            path ["by-puuid", puuid, "ids"]
            query {
                queue,
                "startTime" = start_time,
                "endTime" = end_time,
                start,
                count
            }
            headers {
                "X-Riot-Puuid" = fmt["puuid:", puuid]
            }
            paginate OffsetLimitPagination {
                offset = start,
                limit = count
            }
            -> Json<Vec<String>>
            rate_limit match_v5_method

            GET GetMatch(match_id: String)
            as by_id
            path [match_id]
            -> Json<models::MatchDto>
            rate_limit match_v5_method

            GET GetTimeline(match_id: String)
            as timeline
            path [match_id, "timeline"]
            -> Json<models::TimelineDto>
            rate_limit match_v5_method

            GET GetMatchReplaysByPuuid(puuid: String)
            as replays_by_puuid
            path ["by-puuid", puuid, "replays"]
            -> Json<serde_json::Value>
            rate_limit riot_high_volume_method
        }

        scope tournament_stub_v5 {
            path ["lol", "tournament-stub", "v5"]

            POST CreateTournamentStubCodes(tournament_id: i64, count?: u32, body: Json<serde_json::Value>)
            path ["codes"]
            query {
                "tournamentId" = tournament_id,
                count
            }
            -> Json<Vec<String>>
            rate_limit riot_high_volume_method

            GET GetTournamentStubLobbyEventsByCode(tournament_code: String)
            path ["lobby-events", "by-code", tournament_code]
            -> Json<serde_json::Value>
            rate_limit riot_high_volume_method

            GET GetTournamentStubCode(tournament_code: String)
            path ["codes", tournament_code]
            -> Json<serde_json::Value>
            rate_limit riot_high_volume_method

            POST RegisterTournamentStubProvider(body: Json<serde_json::Value>)
            path ["providers"]
            -> Json<i32>
            rate_limit riot_high_volume_method

            POST RegisterTournamentStubTournament(body: Json<serde_json::Value>)
            path ["tournaments"]
            -> Json<i32>
            rate_limit riot_high_volume_method
        }
    }
}

api! {
    client DDragonClient {
        base "https://leagueoflegends.com"
        headers {
            "user-agent" = "ClientApiDDragonExample/1.0"
        }
        default {
            retry read
        }
        retry read {
                max_attempts 2
                methods [GET]
                on [429, 500, 502, 503, 504]
                retry_after
        }
    }

    scope ddragon {
        host ["ddragon"]

        scope api_root {
            path ["api"]

            GET GetVersions
            path ["versions.json"]
            -> Json<Vec<String>>

            GET GetRealmByRegion(region: String)
            path ["realms", region]
            -> Json<models::RealmDto>
        }

        scope cdn_root {
            path ["cdn"]

            GET GetLanguages
            path ["languages.json"]
            -> Json<Vec<String>>
        }

        scope cdn_versioned(version: String) {
            path ["cdn", version]

            scope data_localized(locale: String = "en_US".to_string()) {
                path ["data", locale]

                scope champion {
                    path ["champion"]

                    GET GetChampionList
                    path ["champion.json"]
                    -> Json<models::ChampionListDto>

                    GET GetChampionFull
                    path ["championFull.json"]
                    -> Json<serde_json::Value>

                    GET GetChampionDetail(champion: String)
                    path [fmt[champion, ".json"]]
                    -> Json<models::ChampionDetailDto>
                }

                GET GetSummonerSpells
                path ["summoner.json"]
                -> Json<models::SummonerSpellListDto>

                GET GetItems
                path ["item.json"]
                -> Json<models::ItemListDto>

                GET GetRunesReforged
                path ["runesReforged.json"]
                -> Json<models::RunesReforgedDto>
            }
        }
    }
}

use crate::riot::d_dragon_client::DDragonClient;

pub use self::riot_client::{RiotClient, endpoints as riot_endpoints};
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

/// Regional routing values for account/match routes.
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
    // ACCOUNT (regional routing)
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

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct AccountRegionDto {
        #[serde(flatten)]
        pub raw: Value,
    }

    // ---------------------------
    // CHAMPION-V3
    // ---------------------------
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct ChampionRotationsDto {
        pub free_champion_ids: Vec<i64>,
        pub free_champion_ids_for_new_players: Vec<i64>,
        pub max_new_player_level: i64,
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
        #[serde(default)]
        pub summoner_id: Option<String>,
        #[serde(default)]
        pub puuid: Option<String>,
        #[serde(flatten)]
        pub extra: HashMap<String, Value>,
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
    // CLASH
    // ---------------------------
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ClashTeamDto {
        #[serde(flatten)]
        pub raw: Value,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ClashTournamentDto {
        #[serde(flatten)]
        pub raw: Value,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ClashPlayerDto {
        #[serde(flatten)]
        pub raw: Value,
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
        .regional(default_region)
        .account_v1_accounts()
        .by_riot_id("Random Iron".to_string(), "EUVV".to_string())
        .await?;
    println!(
        "Account: {}#{} puuid={}",
        account.game_name, account.tag_line, account.puuid
    );

    let match_ids: Vec<String> = riot
        .regional(default_region)
        .match_v5_matches()
        .ids_by_puuid(account.puuid.clone())
        .paginate()
        .max_items(10_000)
        .collect()
        .await?;
    println!("match_ids Len: {:?}", match_ids.len());

    let ddragon = DDragonClient::new().configure(|config| {
        config.debug(DebugLevel::VV);
    });
    let version = ddragon
        .ddragon()
        .api_root()
        .get_versions()
        .await?
        .first()
        .map(|v| v.clone())
        .unwrap_or_default();
    let champion = ddragon
        .ddragon()
        .cdn_versioned(version.clone())
        .data_localized()
        .locale("Fr-fr".into())
        .champion()
        .get_champion_detail("Vayne".into())
        .await?;
    let champions = ddragon
        .ddragon()
        .cdn_versioned(version.clone())
        .data_localized()
        .locale("Fr-fr".into())
        .champion()
        .get_champion_list()
        .await?;
    println!("champions Len: {:?}", champions.data.keys().len());

    Ok(())
}
