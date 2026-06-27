use concord_core::prelude::*;
use concord_macros::api;
use serde::{Deserialize, Serialize};

#[derive(Default)]
pub struct RiotRateLimitHeaders;

impl RateLimitObserver for RiotRateLimitHeaders {
    fn observe(&self, ctx: RateLimitResponseContext<'_>) -> RateLimitObservation {
        // Static DSL buckets are the primary limiter. Riot response headers refine
        // 429 handling by scope and Retry-After when the platform sends them.
        ctx.on_429().scope_header("x-rate-limit-type").retry_after()
    }
}

api! {
    client RiotClient {
        base "https://riotgames.com"

        auth {
            secret api_key: String
            credential riot_api_key = api_key(secret.api_key)
        }

        headers {
            "user-agent" = "ConcordRiotExample/1.0"
        }

        policies {
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

            rate_limit league_apex_by_queue {
                bucket method by [host, endpoint] {
                    30 / 10s
                    500 / 10m
                }
            }

            rate_limit league_entries_page {
                bucket method by [host, endpoint] {
                    50 / 10s
                }
            }

            rate_limit league_entries_by_puuid {
                bucket method by [host, endpoint] {
                    20000 / 10s
                    1200000 / 10m
                }
            }

            rate_limit league_exp_entries_page {
                bucket method by [host, endpoint] {
                    50 / 10s
                }
            }

            rate_limit clash_team {
                bucket method by [host, endpoint] {
                    200 / 1m
                }
            }

            rate_limit clash_tournament {
                bucket method by [host, endpoint] {
                    10 / 1m
                }
            }

            rate_limit clash_players_by_puuid {
                bucket method by [host, endpoint] {
                    20000 / 10s
                    1200000 / 10m
                }
            }

            rate_limit account_standard_lookup {
                bucket method by [host, endpoint] {
                    1000 / 1m
                }
            }

            rate_limit account_high_volume {
                bucket method by [host, endpoint] {
                    20000 / 10s
                    1200000 / 10m
                }
            }

            rate_limit status_platform_data {
                bucket method by [host, endpoint] {
                    20000 / 10s
                    1200000 / 10m
                }
            }

            rate_limit match_v5_standard {
                bucket method by [host, endpoint] {
                    2000 / 10s
                }
            }

            rate_limit match_v5_replays {
                bucket method by [host, endpoint] {
                    20000 / 10s
                    1200000 / 10m
                }
            }

            rate_limit challenges_high_volume {
                bucket method by [host, endpoint] {
                    20000 / 10s
                    1200000 / 10m
                }
            }

            rate_limit champion_mastery_high_volume {
                bucket method by [host, endpoint] {
                    20000 / 10s
                    1200000 / 10m
                }
            }

            rate_limit tournament_stub_high_volume {
                bucket method by [host, endpoint] {
                    20000 / 10s
                    1200000 / 10m
                }
            }

            rate_limit spectator_live_game {
                bucket method by [host, endpoint] {
                    3000 / 10s
                    180000 / 10m
                }
            }

            rate_limit champion_rotation_high_volume {
                bucket method by [host, endpoint] {
                    20000 / 10s
                    1200000 / 10m
                }
            }
        }

        behaviors {
            behavior riot_read {
                auth header "X-Riot-Token" = riot_api_key
                retry read
                rate_limit app
            }

            behavior summoner_by_puuid_read {
                rate_limit summoner_by_puuid
            }

            behavior league_apex_read {
                rate_limit league_apex_by_queue
            }

            behavior league_entries_page_read {
                rate_limit league_entries_page
            }

            behavior league_entries_by_puuid_read {
                rate_limit league_entries_by_puuid
            }

            behavior league_exp_entries_page_read {
                rate_limit league_exp_entries_page
            }

            behavior clash_team_read {
                rate_limit clash_team
            }

            behavior clash_tournament_read {
                rate_limit clash_tournament
            }

            behavior clash_players_by_puuid_read {
                rate_limit clash_players_by_puuid
            }

            behavior account_standard_read {
                rate_limit account_standard_lookup
            }

            behavior account_high_volume_read {
                rate_limit account_high_volume
            }

            behavior status_platform_data_read {
                rate_limit status_platform_data
            }

            behavior match_v5_standard_read {
                rate_limit match_v5_standard
            }

            behavior match_v5_replays_read {
                rate_limit match_v5_replays
            }

            behavior challenges_high_volume_read {
                rate_limit challenges_high_volume
            }

            behavior champion_mastery_high_volume_read {
                rate_limit champion_mastery_high_volume
            }

            behavior tournament_stub_high_volume_read {
                rate_limit tournament_stub_high_volume
            }

            behavior champion_rotation_high_volume_read {
                rate_limit champion_rotation_high_volume
            }

            behavior spectator_live_game_read {
                rate_limit spectator_live_game
            }
        }

        defaults {
            behavior riot_read
        }
    }

    scope platform(platform: PlatformRoute) {
        host [platform, "api"]
        path ["lol"]

        scope champion_v3 {
            path ["platform", "v3"]

            GET GetChampionRotations
            as rotations
            path ["champion-rotations"]
            behavior champion_rotation_high_volume_read
            -> Json<models::ChampionRotationsDto>
        }

        scope summoner_v4 {
            path ["summoner", "v4", "summoners"]

            GET GetSummonerByPuuid(encrypted_puuid: String)
            as by_puuid
            path ["by-puuid", encrypted_puuid]
            behavior summoner_by_puuid_read
            -> Json<models::SummonerDto>
        }

        scope league_v4 {
            path ["league", "v4"]

            scope challengerleagues {
                path ["challengerleagues"]

                GET GetChallengerLeagueByQueue(queue: LeagueQueue)
                as by_queue
                path ["by-queue", queue]
                behavior league_apex_read
                -> Json<models::LeagueListDto>
            }

            scope grandmasterleagues {
                path ["grandmasterleagues"]

                GET GetGrandmasterLeagueByQueue(queue: LeagueQueue)
                as by_queue
                path ["by-queue", queue]
                behavior league_apex_read
                -> Json<models::LeagueListDto>
            }

            scope masterleagues {
                path ["masterleagues"]

                GET GetMasterLeagueByQueue(queue: LeagueQueue)
                as by_queue
                path ["by-queue", queue]
                behavior league_apex_read
                -> Json<models::LeagueListDto>
            }

            scope entries {
                path ["entries"]

                GET GetLeagueEntries(queue: LeagueQueue, tier: LeagueTier, division: LeagueDivision, page?: u32 = 1)
                as by_queue
                path [queue, tier, division]
                query {
                    page
                }
                behavior league_entries_page_read
                -> Json<Vec<models::LeagueEntryDto>>

                GET GetLeagueEntriesByPuuid(encrypted_puuid: String)
                as by_puuid
                path ["by-puuid", encrypted_puuid]
                behavior league_entries_by_puuid_read
                -> Json<Vec<models::LeagueEntryDto>>
            }
        }

        scope league_exp_v4 {
            path ["league-exp", "v4", "entries"]

            GET GetLeagueExpEntries(queue: LeagueQueue, tier: LeagueTier, division: LeagueDivision, page?: u32 = 1)
            as by_queue
            path [queue, tier, division]
            query {
                page
            }
            behavior league_exp_entries_page_read
            -> Json<Vec<models::LeagueEntryDto>>
        }

        scope clash_v1 {
            path ["clash", "v1"]

            GET GetClashTeam(team_id: String)
            as team
            path ["teams", team_id]
            behavior clash_team_read
            -> Json<models::ClashTeamDto>

            GET GetClashTournament(tournament_id: i64)
            as tournament
            path ["tournaments", tournament_id]
            behavior clash_tournament_read
            -> Json<models::ClashTournamentDto>

            GET GetClashTournamentByTeam(team_id: String)
            as tournament_by_team
            path ["tournaments", "by-team", team_id]
            behavior clash_team_read
            -> Json<models::ClashTournamentDto>

            GET GetClashTournaments
            as tournaments
            path ["tournaments"]
            behavior clash_tournament_read
            -> Json<Vec<models::ClashTournamentDto>>

            GET GetClashPlayersByPuuid(puuid: String)
            as players_by_puuid
            path ["players", "by-puuid", puuid]
            behavior clash_players_by_puuid_read
            -> Json<Vec<models::ClashPlayerDto>>
        }

        scope status_v4 {
            path ["status", "v4"]

            GET GetPlatformData
            as platform_data
            path ["platform-data"]
            behavior status_platform_data_read
            -> Json<models::PlatformDataDto>
        }

        scope challenges_v1 {
            path ["challenges", "v1"]
            behavior challenges_high_volume_read

            GET GetChallengePercentiles
            as percentiles
            path ["challenges", "percentiles"]
            -> Json<serde_json::Value>

            GET GetChallengeLeaderboardsByLevel(challenge_id: i64, level: String, limit?: u32)
            as leaderboards_by_level
            path ["challenges", challenge_id, "leaderboards", "by-level", level]
            query {
                limit
            }
            -> Json<serde_json::Value>

            GET GetChallengePercentilesByChallenge(challenge_id: i64)
            as percentiles_by_challenge
            path ["challenges", challenge_id, "percentiles"]
            -> Json<serde_json::Value>

            GET GetChallengeConfig(challenge_id: i64)
            as config
            path ["challenges", challenge_id, "config"]
            -> Json<serde_json::Value>

            GET GetChallengePlayerData(puuid: String)
            as player_data
            path ["player-data", puuid]
            -> Json<serde_json::Value>

            GET GetChallengeConfigs
            as configs
            path ["challenges", "config"]
            -> Json<serde_json::Value>
        }

        scope champion_mastery_v4 {
            path ["champion-mastery", "v4"]
            behavior champion_mastery_high_volume_read

            GET GetChampionMasteriesByPuuid(encrypted_puuid: String)
            as by_puuid
            path ["champion-masteries", "by-puuid", encrypted_puuid]
            -> Json<Vec<models::ChampionMasteryDto>>

            GET GetChampionMasteryByPuuidAndChampion(encrypted_puuid: String, champion_id: i64)
            as by_puuid_and_champion
            path ["champion-masteries", "by-puuid", encrypted_puuid, "by-champion", champion_id]
            -> Json<models::ChampionMasteryDto>

            GET GetChampionMasteryScoreByPuuid(encrypted_puuid: String)
            as score_by_puuid
            path ["scores", "by-puuid", encrypted_puuid]
            -> Json<i32>

            GET GetTopChampionMasteriesByPuuid(encrypted_puuid: String, count?: u32)
            as top_by_puuid
            path ["champion-masteries", "by-puuid", encrypted_puuid, "top"]
            query {
                count
            }
            -> Json<Vec<models::ChampionMasteryDto>>
        }

        scope spectator_v5 {
            path ["spectator", "v5"]

            GET GetActiveGameByPuuid(encrypted_puuid: String)
            as active_game_by_puuid
            path ["active-games", "by-summoner", encrypted_puuid]
            behavior spectator_live_game_read
            -> Json<models::CurrentGameInfo>
        }
    }

    scope regional(region: RegionalRoute) {
        host [region, "api"]

        scope account_v1_accounts {
            path ["riot", "account", "v1", "accounts"]

            GET GetAccountByRiotId(game_name: String, tag_line: String)
            as by_riot_id
            path ["by-riot-id", game_name, tag_line]
            behavior account_standard_read
            -> Json<models::AccountDto>

            GET GetAccountByPuuid(puuid: String)
            as by_puuid
            path ["by-puuid", puuid]
            behavior account_standard_read
            -> Json<models::AccountDto>
        }

        scope account_v1_region {
            path ["riot", "account", "v1", "region"]

            GET GetAccountRegionByGameAndPuuid(game: String, puuid: String)
            as by_game_and_puuid
            path ["by-game", game, "by-puuid", puuid]
            behavior account_high_volume_read
            -> Json<models::AccountRegionDto>
        }

        scope match_v5_matches {
            path ["lol", "match", "v5", "matches"]

            GET GetMatchIdsByPuuid(puuid: String, queue?: u16, match_type?: String, start_time?: i64, end_time?: i64, start: u64 = 0, count: u64 = 20)
            as ids_by_puuid
            path ["by-puuid", puuid, "ids"]
            query {
                queue,
                "type" = match_type,
                "startTime" = start_time,
                "endTime" = end_time,
                start,
                count
            }
            paginate OffsetLimitPagination {
                offset = start,
                limit = count
            }
            behavior match_v5_standard_read
            -> Json<Vec<String>>

            GET GetMatch(match_id: String)
            as by_id
            path [match_id]
            behavior match_v5_standard_read
            -> Json<models::MatchDto>

            GET GetTimeline(match_id: String)
            as timeline
            path [match_id, "timeline"]
            behavior match_v5_standard_read
            -> Json<models::TimelineDto>

            GET GetMatchReplaysByPuuid(puuid: String)
            as replays_by_puuid
            path ["by-puuid", puuid, "replays"]
            behavior match_v5_replays_read
            -> Json<serde_json::Value>
        }

        scope tournament_stub_v5 {
            path ["lol", "tournament-stub", "v5"]
            behavior tournament_stub_high_volume_read

            // Mutation endpoints are included for DSL/type coverage only. The
            // default live smoke path must not call them. Do not run mutation
            // calls against live Riot services unless explicitly testing
            // intended side effects with an owned key.
            POST CreateTournamentStubCodes(tournament_id: i64, count?: u32, body: Json<serde_json::Value>)
            as create_codes
            path ["codes"]
            query {
                "tournamentId" = tournament_id,
                count
            }
            -> Json<Vec<String>>

            GET GetTournamentStubLobbyEventsByCode(tournament_code: String)
            as lobby_events_by_code
            path ["lobby-events", "by-code", tournament_code]
            -> Json<serde_json::Value>

            GET GetTournamentStubCode(tournament_code: String)
            as code
            path ["codes", tournament_code]
            -> Json<serde_json::Value>

            POST RegisterTournamentStubProvider(body: Json<serde_json::Value>)
            as register_provider
            path ["providers"]
            -> Json<i32>

            POST RegisterTournamentStubTournament(body: Json<serde_json::Value>)
            as register_tournament
            path ["tournaments"]
            -> Json<i32>
        }
    }
}

pub use self::riot_client::{RiotClient, endpoints as riot_endpoints};

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
    TR1,
    RU,
    PH2,
    SG2,
    TH2,
    TW2,
    VN2,
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
            PlatformRoute::TR1 => "tr1",
            PlatformRoute::RU => "ru",
            PlatformRoute::PH2 => "ph2",
            PlatformRoute::SG2 => "sg2",
            PlatformRoute::TH2 => "th2",
            PlatformRoute::TW2 => "tw2",
            PlatformRoute::VN2 => "vn2",
        };
        f.write_str(s)
    }
}

#[derive(Clone, Copy, Debug)]
pub enum RegionalRoute {
    Americas,
    Asia,
    Europe,
    Sea,
}

impl core::fmt::Display for RegionalRoute {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let s = match self {
            RegionalRoute::Americas => "americas",
            RegionalRoute::Asia => "asia",
            RegionalRoute::Europe => "europe",
            RegionalRoute::Sea => "sea",
        };
        f.write_str(s)
    }
}

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

#[derive(Clone, Copy, Debug)]
pub enum LeagueTier {
    Iron,
    Bronze,
    Silver,
    Gold,
    Platinum,
    Emerald,
    Diamond,
}

impl core::fmt::Display for LeagueTier {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let s = match self {
            LeagueTier::Iron => "IRON",
            LeagueTier::Bronze => "BRONZE",
            LeagueTier::Silver => "SILVER",
            LeagueTier::Gold => "GOLD",
            LeagueTier::Platinum => "PLATINUM",
            LeagueTier::Emerald => "EMERALD",
            LeagueTier::Diamond => "DIAMOND",
        };
        f.write_str(s)
    }
}

#[derive(Clone, Copy, Debug)]
pub enum LeagueDivision {
    I,
    II,
    III,
    IV,
}

impl core::fmt::Display for LeagueDivision {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let s = match self {
            LeagueDivision::I => "I",
            LeagueDivision::II => "II",
            LeagueDivision::III => "III",
            LeagueDivision::IV => "IV",
        };
        f.write_str(s)
    }
}

pub mod models {
    use super::*;
    use serde_json::Value;
    use std::collections::HashMap;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct AccountDto {
        pub puuid: String,

        #[serde(default)]
        pub game_name: Option<String>,

        #[serde(default)]
        pub tag_line: Option<String>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct AccountRegionDto {
        #[serde(flatten)]
        pub raw: Value,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct ChampionRotationsDto {
        pub free_champion_ids: Vec<i64>,
        pub free_champion_ids_for_new_players: Vec<i64>,
        pub max_new_player_level: i64,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct SummonerDto {
        pub profile_icon_id: u32,
        pub revision_date: i64,
        pub puuid: String,
        pub summoner_level: u64,

        #[serde(default)]
        pub id: Option<String>,

        #[serde(default)]
        pub account_id: Option<String>,
    }

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
        pub puuid: Option<String>,
        #[serde(flatten)]
        pub extra: HashMap<String, Value>,
    }

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
        #[serde(default)]
        pub league_id: Option<String>,
        #[serde(default)]
        pub puuid: Option<String>,
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
        #[serde(default)]
        pub puuid: Option<String>,
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

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct CurrentGameInfo {
        #[serde(flatten)]
        pub raw: Value,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct PlatformDataDto {
        #[serde(flatten)]
        pub raw: Value,
    }

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
}

pub async fn riot_test() -> Result<(), ApiClientError> {
    dotenvy::dotenv().ok();

    let api_key = dotenvy::var("RIOT_API_KEY")
        .expect("RIOT_API_KEY missing; set it in the environment or .env before running riot_test");

    let riot = RiotClient::new(api_key);

    let account = riot
        .regional(RegionalRoute::Europe)
        .account_v1_accounts()
        .by_riot_id("DISC OF THE SUN".to_string(), "EUW".to_string())
        .await?;

    println!(
        "Riot account: {}#{} puuid={}",
        account.game_name.as_deref().unwrap_or("<missing-gameName>"),
        account.tag_line.as_deref().unwrap_or("<missing-tagLine>"),
        account.puuid
    );

    let account_by_puuid = riot
        .regional(RegionalRoute::Europe)
        .account_v1_accounts()
        .by_puuid(account.puuid.clone())
        .await?;

    println!(
        "Account by PUUID: gameName_present={} tagLine_present={}",
        account_by_puuid.game_name.is_some(),
        account_by_puuid.tag_line.is_some()
    );

    match riot
        .regional(RegionalRoute::Europe)
        .account_v1_region()
        .by_game_and_puuid("lol".to_string(), account.puuid.clone())
        .await
    {
        Ok(region) => {
            println!(
                "Active region lookup: available fields={:?}",
                region.raw.as_object().map(|object| object.len())
            );
        }
        Err(err) => {
            println!("Active region lookup: unavailable: {err}");
        }
    }

    let summoner = riot
        .platform(PlatformRoute::EUW1)
        .summoner_v4()
        .by_puuid(account.puuid.clone())
        .await?;

    println!(
        "Summoner: puuid={} level={} icon={} revision={}",
        summoner.puuid, summoner.summoner_level, summoner.profile_icon_id, summoner.revision_date
    );
    println!(
        "Summoner legacy ids: id={:?} account_id={:?}",
        summoner.id, summoner.account_id
    );

    match riot
        .platform(PlatformRoute::EUW1)
        .league_v4()
        .entries()
        .by_puuid(account.puuid.clone())
        .await
    {
        Ok(league_entries) => {
            println!("League entries: {}", league_entries.len());
            for entry in &league_entries {
                println!(
                    "Rank: queue={} {} {} {}LP wins={} losses={} hot_streak={} veteran={} inactive={}",
                    entry.queue_type,
                    entry.tier,
                    entry.rank,
                    entry.league_points,
                    entry.wins,
                    entry.losses,
                    entry.hot_streak,
                    entry.veteran,
                    entry.inactive
                );
            }
        }
        Err(err) => {
            println!("League entries: unavailable: {err}");
        }
    }

    match riot
        .platform(PlatformRoute::EUW1)
        .champion_mastery_v4()
        .by_puuid(account.puuid.clone())
        .await
    {
        Ok(masteries) => println!("Champion mastery entries: {}", masteries.len()),
        Err(err) => println!("Champion mastery entries: unavailable: {err}"),
    }

    match riot
        .platform(PlatformRoute::EUW1)
        .champion_mastery_v4()
        .score_by_puuid(account.puuid.clone())
        .await
    {
        Ok(score) => println!("Champion mastery score: {score}"),
        Err(err) => println!("Champion mastery score: unavailable: {err}"),
    }

    match riot
        .platform(PlatformRoute::EUW1)
        .champion_mastery_v4()
        .top_by_puuid(account.puuid.clone())
        .count(5)
        .await
    {
        Ok(top_masteries) => {
            println!("Top champion masteries: {}", top_masteries.len());
            if let Some(first) = top_masteries.first() {
                println!(
                    "Top mastery first: champion_id={} points={}",
                    first.champion_id, first.champion_points
                );
            }
        }
        Err(err) => println!("Top champion masteries: unavailable: {err}"),
    }

    match riot
        .platform(PlatformRoute::EUW1)
        .champion_v3()
        .rotations()
        .await
    {
        Ok(rotations) => {
            println!(
                "Champion rotations: free={} new_player_free={}",
                rotations.free_champion_ids.len(),
                rotations.free_champion_ids_for_new_players.len()
            );
        }
        Err(err) => println!("Champion rotations: unavailable: {err}"),
    }

    match riot
        .platform(PlatformRoute::EUW1)
        .status_v4()
        .platform_data()
        .await
    {
        Ok(status) => {
            println!(
                "Platform status: fields={:?}",
                status.raw.as_object().map(|object| object.len())
            );
        }
        Err(err) => println!("Platform status: unavailable: {err}"),
    }

    match riot
        .platform(PlatformRoute::EUW1)
        .challenges_v1()
        .player_data(account.puuid.clone())
        .await
    {
        Ok(challenge_player_data) => {
            println!(
                "Challenge player data: fields={:?}",
                challenge_player_data.as_object().map(|object| object.len())
            );
        }
        Err(err) => println!("Challenge player data: unavailable: {err}"),
    }

    match riot
        .platform(PlatformRoute::EUW1)
        .clash_v1()
        .players_by_puuid(account.puuid.clone())
        .await
    {
        Ok(clash_players) => println!("Clash player registrations: {}", clash_players.len()),
        Err(err) => println!("Clash player registrations: unavailable: {err}"),
    }

    let match_ids: Vec<String> = riot
        .regional(RegionalRoute::Europe)
        .match_v5_matches()
        .ids_by_puuid(account.puuid.clone())
        .count(100)
        .paginate(PaginationTermination::take_items(400))
        .collect()
        .await?;

    println!("Recent match ids: {}", match_ids.len());

    for match_id in match_ids.iter().take(2) {
        let match_detail = riot
            .regional(RegionalRoute::Europe)
            .match_v5_matches()
            .by_id(match_id.clone())
            .await;

        match match_detail {
            Ok(detail) => {
                println!(
                    "Match {match_id}: detail decoded metadata_fields={} info_fields={}",
                    detail.metadata.len(),
                    detail.info.len()
                );
            }
            Err(err) => println!("Match {match_id}: detail unavailable: {err}"),
        }

        let timeline = riot
            .regional(RegionalRoute::Europe)
            .match_v5_matches()
            .timeline(match_id.clone())
            .await;

        match timeline {
            Ok(timeline) => {
                println!(
                    "Match {match_id}: timeline decoded fields={:?}",
                    timeline.raw.as_object().map(|object| object.len())
                );
            }
            Err(err) => println!("Match {match_id}: timeline unavailable: {err}"),
        }
    }

    match riot
        .regional(RegionalRoute::Europe)
        .match_v5_matches()
        .replays_by_puuid(account.puuid.clone())
        .await
    {
        Ok(replays) => {
            println!(
                "Match replays: available fields={:?}",
                replays.as_object().map(|object| object.len())
            );
        }
        Err(err) => println!("Match replays: unavailable: {err}"),
    }

    match riot
        .platform(PlatformRoute::EUW1)
        .spectator_v5()
        .active_game_by_puuid(account.puuid.clone())
        .await
    {
        Ok(_) => {
            println!("Live game: active game found");
        }
        Err(err) => {
            println!("Live game: unavailable or not currently in game: {err}");
        }
    }

    Ok(())
}
