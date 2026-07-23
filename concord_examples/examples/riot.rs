#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::num::NonZeroU32;

use concord_core::advanced::{
    Cooldown, CooldownScope, DefaultRateLimitObserver, HeaderName, RateLimitObserver,
    RateLimitResponseHead, SanitizedHeaderValue,
};
use concord_core::prelude::*;
use concord_macros::api;
use serde::{Deserialize, Serialize};

use serde_json::Value;

/// Riot's documented starting application limit for a production API key.
///
/// Riot applies the application limit per routing host. The unified client
/// declares separate lexical rules for its platform and regional route trees.
const DEFAULT_MATCH_PAGE_SIZE: NonZeroU32 = match NonZeroU32::new(20) {
    Some(value) => value,
    None => unreachable!(),
};

#[derive(Clone, Copy, Debug)]
pub enum PlatformRoute {
    Br1,
    Eun1,
    Euw1,
    Jp1,
    Kr,
    La1,
    La2,
    Me1,
    Na1,
    Oc1,
    Ru,
    Sg2,
    Tr1,
    Tw2,
    Vn2,
}

impl PlatformRoute {
    #[must_use]
    pub const fn regional(self) -> RegionalRoute {
        match self {
            Self::Br1 | Self::La1 | Self::La2 | Self::Na1 => RegionalRoute::Americas,
            Self::Jp1 | Self::Kr => RegionalRoute::Asia,
            Self::Eun1 | Self::Euw1 | Self::Me1 | Self::Ru | Self::Tr1 => RegionalRoute::Europe,
            Self::Oc1 | Self::Sg2 | Self::Tw2 | Self::Vn2 => RegionalRoute::Sea,
        }
    }
}

impl fmt::Display for PlatformRoute {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Br1 => "br1",
            Self::Eun1 => "eun1",
            Self::Euw1 => "euw1",
            Self::Jp1 => "jp1",
            Self::Kr => "kr",
            Self::La1 => "la1",
            Self::La2 => "la2",
            Self::Me1 => "me1",
            Self::Na1 => "na1",
            Self::Oc1 => "oc1",
            Self::Ru => "ru",
            Self::Sg2 => "sg2",
            Self::Tr1 => "tr1",
            Self::Tw2 => "tw2",
            Self::Vn2 => "vn2",
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub enum RegionalRoute {
    Americas,
    Asia,
    Europe,
    Sea,
}

#[derive(Clone, Copy, Debug)]
pub enum DDragonLocale {
    EnUs,
    FrFr,
}

impl fmt::Display for DDragonLocale {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::EnUs => "en_US",
            Self::FrFr => "fr_FR",
        })
    }
}

impl fmt::Display for RegionalRoute {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Americas => "americas",
            Self::Asia => "asia",
            Self::Europe => "europe",
            Self::Sea => "sea",
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub enum RiotGame {
    LeagueOfLegends,
    TeamfightTactics,
}

impl fmt::Display for RiotGame {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::LeagueOfLegends => "lol",
            Self::TeamfightTactics => "tft",
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub enum MatchType {
    Ranked,
    Normal,
    Tourney,
    Tutorial,
}

impl fmt::Display for MatchType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Ranked => "ranked",
            Self::Normal => "normal",
            Self::Tourney => "tourney",
            Self::Tutorial => "tutorial",
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub enum LeagueQueue {
    RankedSolo5x5,
    RankedFlexSr,
}

impl fmt::Display for LeagueQueue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::RankedSolo5x5 => "RANKED_SOLO_5x5",
            Self::RankedFlexSr => "RANKED_FLEX_SR",
        })
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

impl fmt::Display for LeagueTier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Iron => "IRON",
            Self::Bronze => "BRONZE",
            Self::Silver => "SILVER",
            Self::Gold => "GOLD",
            Self::Platinum => "PLATINUM",
            Self::Emerald => "EMERALD",
            Self::Diamond => "DIAMOND",
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub enum LeagueDivision {
    I,
    Ii,
    Iii,
    Iv,
}

impl fmt::Display for LeagueDivision {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::I => "I",
            Self::Ii => "II",
            Self::Iii => "III",
            Self::Iv => "IV",
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub enum ChallengeLevel {
    None,
    Iron,
    Bronze,
    Silver,
    Gold,
    Platinum,
    Diamond,
    Master,
    Grandmaster,
    Challenger,
}

impl fmt::Display for ChallengeLevel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::None => "NONE",
            Self::Iron => "IRON",
            Self::Bronze => "BRONZE",
            Self::Silver => "SILVER",
            Self::Gold => "GOLD",
            Self::Platinum => "PLATINUM",
            Self::Diamond => "DIAMOND",
            Self::Master => "MASTER",
            Self::Grandmaster => "GRANDMASTER",
            Self::Challenger => "CHALLENGER",
        })
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AccountDto {
    pub puuid: String,
    pub game_name: Option<String>,
    pub tag_line: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AccountRegionDto {
    pub puuid: String,
    pub game: String,
    pub region: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SummonerDto {
    pub id: Option<String>,
    pub account_id: Option<String>,
    pub puuid: String,
    pub profile_icon_id: u32,
    pub revision_date: i64,
    pub summoner_level: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ChampionRotationsDto {
    pub free_champion_ids: Vec<i64>,
    pub free_champion_ids_for_new_players: Vec<i64>,
    pub max_new_player_level: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ChampionMasteryDto {
    pub puuid: String,
    pub champion_id: i64,
    pub champion_level: i32,
    pub champion_points: i64,
    pub last_play_time: i64,
    pub champion_points_since_last_level: i64,
    pub champion_points_until_next_level: i64,
    pub chest_granted: bool,
    pub mark_required_for_next_level: i32,
    pub tokens_earned: i32,
    pub milestone_grades: Vec<String>,
    pub next_season_milestone: Option<Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MiniSeriesDto {
    pub losses: i32,
    pub progress: String,
    pub target: i32,
    pub wins: i32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct LeagueEntryDto {
    pub league_id: Option<String>,
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct LeagueItemDto {
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct LeagueListDto {
    pub league_id: String,
    pub tier: String,
    pub name: String,
    pub queue: String,
    pub entries: Vec<LeagueItemDto>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ClashPlayerDto {
    pub puuid: String,
    pub team_id: String,
    pub position: String,
    pub role: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ClashTeamDto {
    pub id: String,
    pub tournament_id: i32,
    pub name: String,
    pub icon_id: i32,
    pub tier: i32,
    pub captain: String,
    pub abbreviation: String,
    pub players: Vec<ClashPlayerDto>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ClashTournamentPhaseDto {
    pub id: i32,
    pub registration_time: i64,
    pub start_time: i64,
    pub cancelled: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ClashTournamentDto {
    pub id: i32,
    pub theme_id: i32,
    pub name_key: String,
    pub name_key_secondary: String,
    pub schedule: Vec<ClashTournamentPhaseDto>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ChallengeConfigInfoDto {
    pub id: i64,
    pub localized_names: HashMap<String, HashMap<String, String>>,
    pub state: String,
    pub tracking: String,
    pub start_timestamp: i64,
    pub end_timestamp: i64,
    pub leaderboard: bool,
    pub thresholds: HashMap<String, f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ApexPlayerInfoDto {
    pub puuid: String,
    pub value: f64,
    pub position: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ChallengePoints {
    pub level: String,
    pub current: i64,
    pub max: i64,
    pub percentile: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ChallengeInfo {
    pub challenge_id: i64,
    pub percentile: f64,
    pub level: String,
    pub value: f64,
    pub achieved_time: i64,
    pub position: i64,
    pub players_in_level: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PlayerClientPreferences {
    pub banner_accent: String,
    pub title: String,
    pub challenge_ids: Vec<i64>,
    pub crest_border: String,
    pub prestige_crest_border_level: i32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PlayerInfoDto {
    pub challenges: Vec<ChallengeInfo>,
    pub preferences: PlayerClientPreferences,
    pub total_points: ChallengePoints,
    pub category_points: HashMap<String, ChallengePoints>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct StatusContentDto {
    pub locale: String,
    pub content: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct StatusUpdateDto {
    pub id: i32,
    pub author: String,
    pub publish: bool,
    pub publish_locations: Vec<String>,
    pub translations: Vec<StatusContentDto>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct StatusDto {
    pub id: i32,
    pub maintenance_status: String,
    pub incident_severity: String,
    pub titles: Vec<StatusContentDto>,
    pub updates: Vec<StatusUpdateDto>,
    pub created_at: String,
    pub archive_at: String,
    pub updated_at: String,
    pub platforms: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PlatformDataDto {
    pub id: String,
    pub name: String,
    pub locales: Vec<String>,
    pub maintenances: Vec<StatusDto>,
    pub incidents: Vec<StatusDto>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CurrentGameParticipant {
    pub puuid: String,
    pub champion_id: i64,
    pub team_id: i64,
    pub spell1_id: i64,
    pub spell2_id: i64,
    pub profile_icon_id: i64,
    pub bot: bool,
    pub game_customization_objects: Vec<Value>,
    pub perks: Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct BannedChampion {
    pub pick_turn: i32,
    pub champion_id: i64,
    pub team_id: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Observer {
    pub encryption_key: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CurrentGameInfo {
    pub game_id: i64,
    pub game_type: String,
    pub game_start_time: i64,
    pub map_id: i64,
    pub game_length: i64,
    pub platform_id: String,
    pub game_mode: String,
    pub banned_champions: Vec<BannedChampion>,
    pub game_queue_config_id: i64,
    pub observer: Observer,
    pub participants: Vec<CurrentGameParticipant>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ReplayDto {
    pub total: i32,
    #[serde(rename = "matchFileURLs")]
    pub match_file_urls: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MatchMetadataDto {
    pub data_version: String,
    pub match_id: String,
    pub participants: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MatchParticipantDto {
    pub puuid: String,
    pub riot_id_game_name: String,
    pub riot_id_tagline: String,
    pub champion_id: i32,
    pub champion_name: String,
    pub team_id: i32,
    pub team_position: String,
    pub individual_position: String,
    pub kills: i32,
    pub deaths: i32,
    pub assists: i32,
    pub win: bool,
    pub gold_earned: i32,
    pub total_minions_killed: i32,
    pub neutral_minions_killed: i32,
    pub vision_score: i32,
    pub total_damage_dealt_to_champions: i32,
    pub total_damage_taken: i32,
    pub time_played: i32,
    pub items: Option<Vec<i32>>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MatchTeamDto {
    pub team_id: i32,
    pub win: bool,
    pub bans: Vec<Value>,
    pub objectives: Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MatchInfoDto {
    pub game_creation: i64,
    pub game_duration: i64,
    pub game_end_timestamp: i64,
    pub game_id: i64,
    pub game_mode: String,
    pub game_name: String,
    pub game_start_timestamp: i64,
    pub game_type: String,
    pub game_version: String,
    pub map_id: i32,
    pub platform_id: String,
    pub queue_id: i32,
    pub tournament_code: String,
    pub participants: Vec<MatchParticipantDto>,
    pub teams: Vec<MatchTeamDto>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MatchDto {
    pub metadata: MatchMetadataDto,
    pub info: MatchInfoDto,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TimelineParticipantDto {
    pub participant_id: i32,
    pub puuid: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TimelineInfoDto {
    pub frame_interval: i64,
    pub frames: Vec<Value>,
    pub participants: Vec<TimelineParticipantDto>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TimelineDto {
    pub metadata: MatchMetadataDto,
    pub info: TimelineInfoDto,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum TournamentRegion {
    Br,
    Eune,
    Euw,
    Jp,
    Lan,
    Las,
    Na,
    Oce,
    Pbe,
    Tr,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TournamentMap {
    SummonersRift,
    HowlingAbyss,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TournamentPickType {
    BlindPick,
    DraftMode,
    AllRandom,
    TournamentDraft,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum TournamentSpectatorType {
    None,
    Lobbyonly,
    All,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TournamentCodeParameters {
    pub allowed_summoner_ids: Vec<String>,
    pub metadata: String,
    pub team_size: i32,
    pub enough_players: bool,
    pub map_type: Option<TournamentMap>,
    pub pick_type: Option<TournamentPickType>,
    pub spectator_type: Option<TournamentSpectatorType>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TournamentCodeUpdateParameters {
    pub allowed_summoner_ids: Vec<String>,
    pub map_type: Option<TournamentMap>,
    pub pick_type: Option<TournamentPickType>,
    pub spectator_type: Option<TournamentSpectatorType>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ProviderRegistrationParameters {
    pub region: Option<TournamentRegion>,
    pub url: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TournamentRegistrationParameters {
    pub provider_id: i32,
    pub name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TournamentCodeDto {
    pub tournament_code: String,
    pub spectators: String,
    pub lobby_name: String,
    pub metadata: String,
    pub password: String,
    pub team_size: i32,
    pub provider_id: i32,
    pub pick_type: String,
    pub tournament_id: i32,
    pub id: i32,
    pub region: String,
    pub map: String,
    pub participants: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct LobbyEventDto {
    pub timestamp: String,
    pub event_type: String,
    pub summoner_id: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct LobbyEventListDto {
    pub event_list: Vec<LobbyEventDto>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TournamentGameDto {
    pub game_id: i64,
    pub game_name: String,
    pub game_type: String,
    pub game_map: i32,
    pub game_mode: String,
    pub region: String,
    pub start_time: i64,
    pub short_code: String,
    pub metadata: String,
}

/// Interprets Riot's final `429` response headers for future acquisitions.
///
/// Static Concord buckets prevent ordinary bursts. Riot's `Retry-After` and
/// `X-Rate-Limit-Type` headers refine the cooldown after the platform rejects a
/// request. Method cooldowns conservatively cool the current lexical plan.
#[derive(Default)]
pub struct RiotRateLimitHeaders;

impl RateLimitObserver for RiotRateLimitHeaders {
    fn observe(&self, response: &RateLimitResponseHead<'_>) -> Option<Cooldown> {
        let default = DefaultRateLimitObserver.observe(response)?;
        let rate_limit_type = HeaderName::from_static("x-rate-limit-type");

        let scope = match response.headers.get(&rate_limit_type) {
            Some(SanitizedHeaderValue::Text(value))
                if value.eq_ignore_ascii_case("application") =>
            {
                CooldownScope::Host
            }
            Some(SanitizedHeaderValue::Text(value)) if value.eq_ignore_ascii_case("service") => {
                CooldownScope::Host
            }
            Some(SanitizedHeaderValue::Text(value)) if value.eq_ignore_ascii_case("method") => {
                CooldownScope::CurrentBuckets
            }
            _ => return Some(default),
        };

        Some(Cooldown {
            duration: default.duration,
            scope,
        })
    }
}

api! {
    client RiotApi {
        base "https://riotgames.com/";

        vars {
            platform: crate::PlatformRoute = crate::PlatformRoute::Euw1;
            regional: crate::RegionalRoute = crate::RegionalRoute::Europe;
        }

        credentials {
            secret api_key: String;
            credential riot_api_key = api_key(secret.api_key);
        }

        headers { "User-Agent" = "ConcordRiotExample/1.0"; }
        auth header "X-Riot-Token" = riot_api_key challenge never;
        timeout 20s;
        rate_limit_observer crate::RiotRateLimitHeaders;
    }

    scope Platform() {
        host [vars.platform, "api"];
        path ["lol"];
        rate_limit {
            500 / 10s;
            30000 / 10m;
        }

        scope champion_mastery_v4() {
            path ["champion-mastery", "v4"];
            rate_limit {
                20000 / 10s;
                1200000 / 10m;
            }

            GET MasteriesByPuuid(puuid: String) as masteries_by_puuid -> Json<Vec<ChampionMasteryDto>> {
                path ["champion-masteries", "by-puuid", ep.puuid];
            }

            GET MasteryByChampion(puuid: String, champion_id: i64) as mastery_by_champion -> Json<ChampionMasteryDto> {
                path ["champion-masteries", "by-puuid", ep.puuid, "by-champion", ep.champion_id];
            }

            GET TopMasteriesByPuuid(puuid: String, count: Option<u32>) as top_masteries_by_puuid -> Json<Vec<ChampionMasteryDto>> {
                path ["champion-masteries", "by-puuid", ep.puuid, "top"];
                query { "count" = ep.count; }
            }

            GET MasteryScoreByPuuid(puuid: String) as mastery_score_by_puuid -> Json<i32> {
                path ["scores", "by-puuid", ep.puuid];
            }
        }

        scope champion_v3() {
            path ["platform", "v3"];

            GET ChampionRotations() as champion_rotations -> Json<ChampionRotationsDto> {
                path ["champion-rotations"];
                rate_limit {
                    20000 / 10s;
                    1200000 / 10m;
                }
            }
        }

        scope clash_v1() {
            path ["clash", "v1"];

            GET PlayersByPuuid(puuid: String) as clash_players_by_puuid -> Json<Vec<ClashPlayerDto>> {
                path ["players", "by-puuid", ep.puuid];
                rate_limit {
                    20000 / 10s;
                    1200000 / 10m;
                }
            }

            GET Team(team_id: String) as clash_team -> Json<ClashTeamDto> {
                path ["teams", ep.team_id];
                rate_limit { 200 / 1m; }
            }

            GET TournamentByTeam(team_id: String) as clash_tournament_by_team -> Json<ClashTournamentDto> {
                path ["tournaments", "by-team", ep.team_id];
                rate_limit { 200 / 1m; }
            }

            GET Tournament(tournament_id: i64) as clash_tournament -> Json<ClashTournamentDto> {
                path ["tournaments", ep.tournament_id];
                rate_limit { 10 / 1m; }
            }

            GET Tournaments() as clash_tournaments -> Json<Vec<ClashTournamentDto>> {
                path ["tournaments"];
                rate_limit { 10 / 1m; }
            }
        }

        scope league_v4() {
            path ["league", "v4"];

            GET ChallengerByQueue(queue: crate::LeagueQueue) as challenger_by_queue -> Json<LeagueListDto> {
                path ["challengerleagues", "by-queue", ep.queue];
                rate_limit {
                    30 / 10s;
                    500 / 10m;
                }
            }

            GET GrandmasterByQueue(queue: crate::LeagueQueue) as grandmaster_by_queue -> Json<LeagueListDto> {
                path ["grandmasterleagues", "by-queue", ep.queue];
                rate_limit {
                    30 / 10s;
                    500 / 10m;
                }
            }

            GET MasterByQueue(queue: crate::LeagueQueue) as master_by_queue -> Json<LeagueListDto> {
                path ["masterleagues", "by-queue", ep.queue];
                rate_limit {
                    30 / 10s;
                    500 / 10m;
                }
            }

            GET Entries(
                queue: crate::LeagueQueue,
                tier: crate::LeagueTier,
                division: crate::LeagueDivision,
                page: u32 = 1
            ) as league_entries -> Json<Vec<LeagueEntryDto>> {
                path ["entries", ep.queue, ep.tier, ep.division];
                query { "page" = ep.page; }
                rate_limit { 50 / 10s; }
            }

            GET EntriesByPuuid(puuid: String) as league_entries_by_puuid -> Json<Vec<LeagueEntryDto>> {
                path ["entries", "by-puuid", ep.puuid];
                rate_limit {
                    20000 / 10s;
                    1200000 / 10m;
                }
            }

            GET LeagueById(league_id: String) as league_by_id -> Json<LeagueListDto> {
                path ["leagues", ep.league_id];
            }
        }

        scope league_exp_v4() {
            path ["league-exp", "v4"];

            GET Entries(
                queue: crate::LeagueQueue,
                tier: crate::LeagueTier,
                division: crate::LeagueDivision,
                page: u32 = 1
            ) as league_exp_entries -> Json<Vec<LeagueEntryDto>> {
                path ["entries", ep.queue, ep.tier, ep.division];
                query { "page" = ep.page; }
                rate_limit { 50 / 10s; }
            }
        }

        scope challenges_v1() {
            path ["challenges", "v1"];
            rate_limit {
                20000 / 10s;
                1200000 / 10m;
            }

            GET Configs() as challenge_configs -> Json<Vec<ChallengeConfigInfoDto>> {
                path ["challenges", "config"];
            }

            GET Percentiles() as challenge_percentiles -> Json<HashMap<String, HashMap<String, f64>>> {
                path ["challenges", "percentiles"];
            }

            GET Config(challenge_id: i64) as challenge_config -> Json<ChallengeConfigInfoDto> {
                path ["challenges", ep.challenge_id, "config"];
            }

            GET Leaderboard(
                challenge_id: i64,
                level: crate::ChallengeLevel,
                limit: Option<u32>
            ) as challenge_leaderboard -> Json<Vec<ApexPlayerInfoDto>> {
                path ["challenges", ep.challenge_id, "leaderboards", "by-level", ep.level];
                query { "limit" = ep.limit; }
            }

            GET ChallengePercentiles(challenge_id: i64) as challenge_percentiles_by_id -> Json<HashMap<String, f64>> {
                path ["challenges", ep.challenge_id, "percentiles"];
            }

            GET PlayerData(puuid: String) as challenge_player_data -> Json<PlayerInfoDto> {
                path ["player-data", ep.puuid];
            }
        }

        scope status_v4() {
            path ["status", "v4"];

            GET PlatformData() as platform_data -> Json<PlatformDataDto> {
                path ["platform-data"];
                rate_limit {
                    20000 / 10s;
                    1200000 / 10m;
                }
            }
        }

        scope spectator_v5() {
            path ["spectator", "v5"];

            GET ActiveGameByPuuid(puuid: String) as active_game_by_puuid -> Json<CurrentGameInfo> {
                path ["active-games", "by-summoner", ep.puuid];
                rate_limit {
                    3000 / 10s;
                    180000 / 10m;
                }
            }
        }

        scope summoner_v4() {
            path ["summoner", "v4", "summoners"];

            GET SummonerByPuuid(puuid: String) as summoner_by_puuid -> Json<SummonerDto> {
                path ["by-puuid", ep.puuid];
                rate_limit { 1600 / 1m; }
            }

            GET SummonerById(summoner_id: String) as summoner_by_id -> Json<SummonerDto> {
                path [ep.summoner_id];
                rate_limit { 1600 / 1m; }
            }
        }
    }


    scope Regional() {
        host [vars.regional, "api"];
        rate_limit {
            500 / 10s;
            30000 / 10m;
        }

        scope account_v1() {
            path ["riot", "account", "v1"];

            GET AccountByRiotId(game_name: String, tag_line: String) as account_by_riot_id -> Json<AccountDto> {
                path ["accounts", "by-riot-id", ep.game_name, ep.tag_line];
                rate_limit { 1000 / 1m; }
            }

            GET AccountByPuuid(puuid: String) as account_by_puuid -> Json<AccountDto> {
                path ["accounts", "by-puuid", ep.puuid];
                rate_limit { 1000 / 1m; }
            }

            GET ActiveRegion(game: crate::RiotGame, puuid: String) as active_region -> Json<AccountRegionDto> {
                path ["region", "by-game", ep.game, "by-puuid", ep.puuid];
                rate_limit {
                    20000 / 10s;
                    1200000 / 10m;
                }
            }
        }

        scope match_v5() {
            path ["lol", "match", "v5", "matches"];

            GET MatchIdsByPuuid(
                puuid: String,
                start_time: Option<i64>,
                end_time: Option<i64>,
                queue: Option<u32>,
                match_type: Option<crate::MatchType>,
                start: u64 = 0,
                count: std::num::NonZeroU32 = crate::DEFAULT_MATCH_PAGE_SIZE
            ) as match_ids_by_puuid -> Json<Vec<String>> {
                path ["by-puuid", ep.puuid, "ids"];
                query {
                    "startTime" = ep.start_time;
                    "endTime" = ep.end_time;
                    "queue" = ep.queue;
                    "type" = ep.match_type;
                    "start" = ep.start;
                    "count" = ep.count;
                }
                rate_limit { 2000 / 10s; }
                paginate concord_core::advanced::OffsetPagination {
                    offset: bind ep.start,
                    page_size: bind ep.count,
                }
            }

            GET ReplaysByPuuid(puuid: String) as replays_by_puuid -> Json<ReplayDto> {
                path ["by-puuid", ep.puuid, "replays"];
                rate_limit {
                    20000 / 10s;
                    1200000 / 10m;
                }
            }

            GET MatchById(match_id: String) as match_by_id -> Json<MatchDto> {
                path [ep.match_id];
                rate_limit { 2000 / 10s; }
            }

            GET TimelineById(match_id: String) as timeline_by_id -> Json<TimelineDto> {
                path [ep.match_id, "timeline"];
                rate_limit { 2000 / 10s; }
            }
        }

        scope tournament_stub_v5() {
            path ["lol", "tournament-stub", "v5"];
            rate_limit {
                20000 / 10s;
                1200000 / 10m;
            }

            POST CreateCodes(
                tournament_id: i64,
                count: Option<u32>,
                body: Json<TournamentCodeParameters>
            ) as create_stub_codes -> Json<Vec<String>> {
                path ["codes"];
                query {
                    "tournamentId" = ep.tournament_id;
                    "count" = ep.count;
                }
            }

            GET Code(tournament_code: String) as stub_code -> Json<TournamentCodeDto> {
                path ["codes", ep.tournament_code];
            }

            GET LobbyEvents(tournament_code: String) as stub_lobby_events -> Json<LobbyEventListDto> {
                path ["lobby-events", "by-code", ep.tournament_code];
            }

            POST RegisterProvider(body: Json<ProviderRegistrationParameters>) as register_stub_provider -> Json<i32> {
                path ["providers"];
            }

            POST RegisterTournament(body: Json<TournamentRegistrationParameters>) as register_stub_tournament -> Json<i32> {
                path ["tournaments"];
            }
        }

        scope tournament_v5() {
            path ["lol", "tournament", "v5"];

            POST CreateCodes(
                tournament_id: i64,
                count: Option<u32>,
                body: Json<TournamentCodeParameters>
            ) as create_tournament_codes -> Json<Vec<String>> {
                path ["codes"];
                query {
                    "tournamentId" = ep.tournament_id;
                    "count" = ep.count;
                }
            }

            GET Code(tournament_code: String) as tournament_code -> Json<TournamentCodeDto> {
                path ["codes", ep.tournament_code];
            }

            PUT UpdateCode(
                tournament_code: String,
                body: Json<TournamentCodeUpdateParameters>
            ) as update_tournament_code -> NoContent {
                path ["codes", ep.tournament_code];
            }

            GET GameByCode(tournament_code: String) as tournament_game_by_code -> Json<TournamentGameDto> {
                path ["games", "by-code", ep.tournament_code];
            }

            GET LobbyEvents(tournament_code: String) as tournament_lobby_events -> Json<LobbyEventListDto> {
                path ["lobby-events", "by-code", ep.tournament_code];
            }

            POST RegisterProvider(body: Json<ProviderRegistrationParameters>) as register_tournament_provider -> Json<i32> {
                path ["providers"];
            }

            POST RegisterTournament(body: Json<TournamentRegistrationParameters>) as register_tournament -> Json<i32> {
                path ["tournaments"];
            }
        }
    }
}

api! {
    client DDragonApi {
        base "https://ddragon.leagueoflegends.com/";

        vars {
            locale: crate::DDragonLocale = crate::DDragonLocale::EnUs;
        }

        headers { "User-Agent" = "ConcordRiotExample/1.0"; }
        timeout 20s;
    }

    GET Versions() as versions -> Json<Vec<String>> {
        path ["api", "versions.json"];
    }

    GET Realm(region: String) as realm -> Json<serde_json::Value> {
        path ["realms", fmt[ep.region, ".json"]];
    }

    scope Version(version: String) {
        path ["cdn", scope.version];

        scope Data() {
            path ["data", vars.locale];

            GET Champions() as champions -> Json<serde_json::Value> {
                path ["champion.json"];
            }

            GET Items() as items -> Json<serde_json::Value> {
                path ["item.json"];
            }
        }

        scope Assets() {
            path ["img"];

            GET Champion(filename: String) as champion_image -> Bytes {
                path ["champion", ep.filename];
            }

            GET Item(filename: String) as item_image -> Bytes {
                path ["item", ep.filename];
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    use riot_api::prelude::*;

    let api_key = std::env::var("RIOT_API_KEY")?;
    let game_name = std::env::var("RIOT_GAME_NAME")?;
    let tag_line = std::env::var("RIOT_TAG_LINE")?;

    let riot = RiotApi::new(api_key)?;

    let account = riot
        .regional()
        .account_v1()
        .account_by_riot_id(game_name, tag_line)
        .await?;

    println!(
        "Riot ID: {}#{}\nPUUID: {}",
        account.game_name.as_deref().unwrap_or("<unknown>"),
        account.tag_line.as_deref().unwrap_or("<unknown>"),
        account.puuid
    );

    let summoner = riot
        .platform()
        .summoner_v4()
        .summoner_by_puuid(account.puuid.clone())
        .await?;

    println!(
        "Summoner level: {} | profile icon: {}",
        summoner.summoner_level, summoner.profile_icon_id
    );

    let league_entries = riot
        .platform()
        .league_v4()
        .league_entries_by_puuid(account.puuid.clone())
        .await?;

    for entry in &league_entries {
        println!(
            "{}: {} {} — {} LP ({}W / {}L)",
            entry.queue_type, entry.tier, entry.rank, entry.league_points, entry.wins, entry.losses
        );
    }

    let top_masteries = riot
        .platform()
        .champion_mastery_v4()
        .top_masteries_by_puuid(account.puuid.clone())
        .count(5)
        .await?;

    println!("Top champion masteries: {}", top_masteries.len());

    let match_ids = riot
        .regional()
        .match_v5()
        .match_ids_by_puuid(account.puuid.clone())
        .match_type(MatchType::Ranked)
        .count(NonZeroU32::new(20).expect("20 is non-zero"))
        .paginate(PageLimit::Take(2))
        .await?;

    println!("Collected {} recent ranked match IDs", match_ids.len());

    if let Some(match_id) = match_ids.first() {
        let response = riot
            .regional()
            .match_v5()
            .match_by_id(match_id.clone())
            .response()
            .await?;

        println!(
            "Match {}: HTTP {} | queue={} | duration={}s | participants={}",
            response.value().metadata.match_id,
            response.status(),
            response.value().info.queue_id,
            response.value().info.game_duration,
            response.value().info.participants.len()
        );

        let timeline = riot
            .regional()
            .match_v5()
            .timeline_by_id(match_id.clone())
            .await?;

        println!("Timeline frames: {}", timeline.info.frames.len());
    }

    match riot
        .platform()
        .spectator_v5()
        .active_game_by_puuid(account.puuid.clone())
        .await
    {
        Ok(game) => println!(
            "Live game found: id={} mode={} participants={}",
            game.game_id,
            game.game_mode,
            game.participants.len()
        ),
        Err(error) => println!("No accessible live game: {error}"),
    }

    let status = riot
        .platform()
        .status_v4()
        .platform_data()
        .response()
        .await?;
    println!(
        "{} status: HTTP {}, incidents={}, maintenances={}",
        status.value().name,
        status.status(),
        status.value().incidents.len(),
        status.value().maintenances.len()
    );

    Ok(())
}
