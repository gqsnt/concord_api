// Path: ".\\concord_examples\\src\\riot.rs" (DSL migrated to evolution-style scope/params/host/path)
use concord_core::prelude::*;
use concord_macros::api;
use serde::{Deserialize, Serialize};

#[derive(Default)]
pub struct RiotRateLimitHeaders;

impl RateLimitResponsePolicy for RiotRateLimitHeaders {
    fn observe(&self, ctx: &RateLimitResponseContext<'_>) -> RateLimitObservation {
        if ctx.status != http::StatusCode::TOO_MANY_REQUESTS {
            return RateLimitObservation::continue_();
        }

        let target = ctx
            .headers
            .get(http::header::HeaderName::from_static("x-rate-limit-type"))
            .and_then(|value| value.to_str().ok())
            .map(|value| value.trim().to_ascii_lowercase())
            .map(|value| match value.as_str() {
                "application" | "app" => {
                    RateLimitTarget::bucket_kind("application", RateLimitTarget::Host)
                }
                "method" => RateLimitTarget::bucket_kind("method", RateLimitTarget::Endpoint),
                "service" => RateLimitTarget::bucket_kind("service", RateLimitTarget::Host),
                _ => RateLimitTarget::current_plan_or_endpoint(),
            })
            .unwrap_or_else(RateLimitTarget::current_plan_or_endpoint);

        let mut observation = RateLimitObservation::limited().with_target(target);
        if let Some(delay) = parse_retry_after(ctx.headers) {
            observation = observation.with_delay(delay);
        }
        observation
    }
}

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
        retry {
            profile read {
                attempts 2
                methods [GET]
                on status[429, 500, 502, 503, 504]
                retry_after honor
                backoff none
            }
            default read
        }
        rate_limit {
            response custom RiotRateLimitHeaders

            profile app {
                bucket application by [route.host] {
                    limit 500 every 10 seconds
                    limit 30000 every 10 minutes
                }
            }

            profile summoner_by_puuid {
                bucket method by [route.host, endpoint] {
                    limit 1600 every 1 minute
                }
            }

            profile league_queue_slow {
                bucket method by [route.host, endpoint] {
                    limit 30 every 10 seconds
                    limit 500 every 10 minutes
                }
            }

            profile league_by_id {
                bucket method by [route.host, endpoint] {
                    limit 500 every 10 seconds
                }
            }

            profile league_entries {
                bucket method by [route.host, endpoint] {
                    limit 50 every 10 seconds
                }
            }

            profile clash_team_or_by_team {
                bucket method by [route.host, endpoint] {
                    limit 200 every 1 minute
                }
            }

            profile clash_tournament_lookup {
                bucket method by [route.host, endpoint] {
                    limit 10 every 1 minute
                }
            }

            profile account_standard_method {
                bucket method by [route.host, endpoint] {
                    limit 1000 every 1 minute
                }
            }

            profile riot_high_volume_method {
                bucket method by [route.host, endpoint] {
                    limit 20000 every 10 seconds
                    limit 1200000 every 10 minutes
                }
            }

            profile match_v5_method {
                bucket method by [route.host, endpoint] {
                    limit 2000 every 10 seconds
                }
            }

            default app
        }
    }

    scope platform {
        use_auth HeaderAuth("X-Riot-Token", riot_api_key)

        params {
            platform: PlatformRoute
        }
        host[platform, "api"]
        path["lol"]

        scope champion_v3 {
            path["platform", "v3"]

            GET GetChampionRotations {
                path["champion-rotations"]
                rate_limit league_queue_slow
                -> Json<models::ChampionRotationsDto>;
            }
        }

        scope summoner_v4 {
            path["summoner", "v4", "summoners"]

            GET GetSummonerByPuuid {
                params { puuid: String }
                path["by-puuid", puuid]
                rate_limit summoner_by_puuid
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
                    rate_limit riot_high_volume_method
                    -> Json<Vec<models::ChampionMasteryDto>>;
                }

                GET GetChampionMasteryByChampion {
                    params {
                        summoner_id: String,
                        champion_id: i64
                    }
                    path["by-summoner", summoner_id, "by-champion", champion_id]
                    rate_limit riot_high_volume_method
                    -> Json<models::ChampionMasteryDto>;
                }

                GET GetChampionMasteriesByPuuid {
                    params { encrypted_puuid: String }
                    path["by-puuid", encrypted_puuid]
                    rate_limit riot_high_volume_method
                    -> Json<Vec<models::ChampionMasteryDto>>;
                }

                GET GetChampionMasteryByPuuidAndChampion {
                    params {
                        encrypted_puuid: String,
                        champion_id: i64
                    }
                    path["by-puuid", encrypted_puuid, "by-champion", champion_id]
                    rate_limit riot_high_volume_method
                    -> Json<models::ChampionMasteryDto>;
                }

                GET GetTopChampionMasteriesByPuuid {
                    params {
                        encrypted_puuid: String,
                        count?: u32
                    }
                    path["by-puuid", encrypted_puuid, "top"]
                    query {
                        count = count
                    }
                    rate_limit riot_high_volume_method
                    -> Json<Vec<models::ChampionMasteryDto>>;
                }
            }

            scope scores {
                path["scores"]

                GET GetChampionMasteryScore {
                    params { summoner_id: String }
                    path["by-summoner", summoner_id]
                    rate_limit riot_high_volume_method
                    -> Json<i32>;
                }

                GET GetChampionMasteryScoreByPuuid {
                    params { encrypted_puuid: String }
                    path["by-puuid", encrypted_puuid]
                    rate_limit riot_high_volume_method
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
                    rate_limit league_queue_slow
                    -> Json<models::LeagueListDto>;
                }
            }

            scope grandmasterleagues {
                path["grandmasterleagues"]

                GET GetGrandmasterLeagueByQueue {
                    params { queue: LeagueQueue }
                    path["by-queue", queue]
                    rate_limit league_queue_slow
                    -> Json<models::LeagueListDto>;
                }
            }

            scope masterleagues {
                path["masterleagues"]

                GET GetMasterLeagueByQueue {
                    params { queue: LeagueQueue }
                    path["by-queue", queue]
                    rate_limit league_queue_slow
                    -> Json<models::LeagueListDto>;
                }
            }

            scope leagues {
                path["leagues"]

                GET GetLeagueById {
                    params { league_id: String }
                    path[league_id]
                    rate_limit league_by_id
                    -> Json<models::LeagueListDto>;
                }
            }

            scope entries {
                path["entries"]

                GET GetLeagueEntriesBySummoner {
                    params { summoner_id: String }
                    path["by-summoner", summoner_id]
                    rate_limit riot_high_volume_method
                    -> Json<Vec<models::LeagueEntryDto>>;
                }

                GET GetLeagueEntriesByPuuid {
                    params { encrypted_puuid: String }
                    path["by-puuid", encrypted_puuid]
                    rate_limit riot_high_volume_method
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
                    rate_limit league_entries
                    -> Json<Vec<models::LeagueEntryDto>>;
                }
            }
        }

        scope league_exp_v4 {
            path["league-exp", "v4", "entries"]

            GET GetLeagueExpEntries {
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
                rate_limit league_entries
                -> Json<Vec<models::LeagueEntryDto>>;
            }
        }

        scope clash_v1 {
            path["clash", "v1"]

            GET GetClashTeam {
                params { team_id: String }
                path["teams", team_id]
                rate_limit clash_team_or_by_team
                -> Json<models::ClashTeamDto>;
            }

            GET GetClashTournament {
                params { tournament_id: i64 }
                path["tournaments", tournament_id]
                rate_limit clash_tournament_lookup
                -> Json<models::ClashTournamentDto>;
            }

            GET GetClashTournamentByTeam {
                params { team_id: String }
                path["tournaments", "by-team", team_id]
                rate_limit clash_team_or_by_team
                -> Json<models::ClashTournamentDto>;
            }

            GET GetClashTournaments {
                path["tournaments"]
                rate_limit clash_tournament_lookup
                -> Json<Vec<models::ClashTournamentDto>>;
            }

            GET GetClashPlayersByPuuid {
                params { puuid: String }
                path["players", "by-puuid", puuid]
                rate_limit riot_high_volume_method
                -> Json<Vec<models::ClashPlayerDto>>;
            }
        }

        scope challenges_v1 {
            path["challenges", "v1"]

            GET GetChallengePercentiles {
                path["challenges", "percentiles"]
                rate_limit riot_high_volume_method
                -> Json<serde_json::Value>;
            }

            GET GetChallengeLeaderboardsByLevel {
                params {
                    challenge_id: i64,
                    level: String,
                    limit?: u32
                }
                path["challenges", challenge_id, "leaderboards", "by-level", level]
                query {
                    limit = limit
                }
                rate_limit riot_high_volume_method
                -> Json<serde_json::Value>;
            }

            GET GetChallengePercentilesByChallenge {
                params { challenge_id: i64 }
                path["challenges", challenge_id, "percentiles"]
                rate_limit riot_high_volume_method
                -> Json<serde_json::Value>;
            }

            GET GetChallengeConfig {
                params { challenge_id: i64 }
                path["challenges", challenge_id, "config"]
                rate_limit riot_high_volume_method
                -> Json<serde_json::Value>;
            }

            GET GetChallengePlayerData {
                params { puuid: String }
                path["player-data", puuid]
                rate_limit riot_high_volume_method
                -> Json<serde_json::Value>;
            }

            GET GetChallengeConfigs {
                path["challenges", "config"]
                rate_limit riot_high_volume_method
                -> Json<serde_json::Value>;
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

        scope spectator_v5 {
            path["spectator", "v5", "active-games"]

            GET GetSpectatorV5ActiveGameBySummoner {
                params { encrypted_puuid: String }
                path["by-summoner", encrypted_puuid]
                rate_limit riot_high_volume_method
                -> Json<models::CurrentGameInfoDto>;
            }
        }

        scope status_v4 {
            path["status", "v4"]

            GET GetPlatformData {
                path["platform-data"]
                rate_limit riot_high_volume_method
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
                rate_limit [account_standard_method, riot_high_volume_method]
                -> Json<models::AccountDto>;
            }

            GET GetAccountByPuuid {
                params { puuid: String }
                path["by-puuid", puuid]
                rate_limit [account_standard_method, riot_high_volume_method]
                -> Json<models::AccountDto>;
            }
        }

        scope account_v1_region {
            path["riot", "account", "v1", "region"]

            GET GetAccountRegionByGameAndPuuid {
                params {
                    game: String,
                    puuid: String
                }
                path["by-game", game, "by-puuid", puuid]
                rate_limit riot_high_volume_method
                -> Json<models::AccountRegionDto>;
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
                rate_limit riot_high_volume_method
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
                rate_limit match_v5_method
                -> Json<Vec<String>>;
            }

            GET GetMatch {
                params { match_id: String }
                path[match_id]
                rate_limit match_v5_method
                -> Json<models::MatchDto>;
            }

            GET GetTimeline {
                params { match_id: String }
                path[match_id, "timeline"]
                rate_limit match_v5_method
                -> Json<models::TimelineDto>;
            }

            GET GetMatchReplaysByPuuid {
                params { puuid: String }
                path["by-puuid", puuid, "replays"]
                rate_limit riot_high_volume_method
                -> Json<serde_json::Value>;
            }
        }

        scope tournament_stub_v5 {
            path["lol", "tournament-stub", "v5"]

            POST CreateTournamentStubCodes {
                params {
                    tournament_id: i64,
                    count?: u32
                }
                path["codes"]
                query {
                    "tournamentId" = tournament_id,
                    count = count
                }
                body Json<serde_json::Value>
                rate_limit riot_high_volume_method
                -> Json<Vec<String>>;
            }

            GET GetTournamentStubLobbyEventsByCode {
                params { tournament_code: String }
                path["lobby-events", "by-code", tournament_code]
                rate_limit riot_high_volume_method
                -> Json<serde_json::Value>;
            }

            GET GetTournamentStubCode {
                params { tournament_code: String }
                path["codes", tournament_code]
                rate_limit riot_high_volume_method
                -> Json<serde_json::Value>;
            }

            POST RegisterTournamentStubProvider {
                path["providers"]
                body Json<serde_json::Value>
                rate_limit riot_high_volume_method
                -> Json<i32>;
            }

            POST RegisterTournamentStubTournament {
                path["tournaments"]
                body Json<serde_json::Value>
                rate_limit riot_high_volume_method
                -> Json<i32>;
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
        retry {
            profile read {
                attempts 2
                methods [GET]
                on status[429, 500, 502, 503, 504]
                retry_after honor
                backoff none
            }
            default read
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
        pub summoner_id: Option<String>, // legacy encryptedSummonerId shape
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
