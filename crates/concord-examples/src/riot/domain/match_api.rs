//! Match routing values and match/timeline response models.
//!
//! Required features: the Riot bundle. Mode: deterministic planning and
//! manual live use. Documentation owner: `docs/spec.md`.
//! Proving test: `tests/riot_contract.rs`.

use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;

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
