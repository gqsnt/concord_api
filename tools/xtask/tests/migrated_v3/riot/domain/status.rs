//! Platform status, spectator, and replay response models.
//!
//! Required features: the Riot bundle. Mode: compile/serde support.
//! Documentation owner: `docs/spec.md`.
//! Proving test: `tests/riot_contract.rs`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

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
