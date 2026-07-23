//! League, mastery, clash, and challenge values and response models.
//!
//! Required features: the Riot bundle. Mode: compile/serde support.
//! Documentation owner: `docs/recipes/riot.md`.
//! Proving test: `tests/riot_contract.rs`.

use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;

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
