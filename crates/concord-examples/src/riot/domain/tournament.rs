//! Tournament request and response models.
//!
//! Required features: the Riot bundle. Mode: compile/serde support.
//! Documentation owner: `docs/recipes/riot.md`.
//! Proving test: `tests/riot_contract.rs`.

use serde::{Deserialize, Serialize};

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
