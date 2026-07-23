//! Account, summoner, and champion-rotation response models.
//!
//! Required features: the Riot bundle. Mode: compile/serde support.
//! Documentation owner: `docs/recipes/riot.md`.
//! Proving test: `tests/riot_contract.rs`.

use serde::{Deserialize, Serialize};

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
