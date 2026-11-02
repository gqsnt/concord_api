//! Riot DTOs: documented projections of the official reference (see `SOURCES.md`).
//!
//! Riot omits empty values ("Zero is considered an empty value, as well as empty strings, empty
//! lists, and nulls", developer portal, Response Codes). Fields that can legitimately be zero or
//! empty therefore carry `#[serde(default)]`; identifiers that every documented response carries
//! (a PUUID, a match id) stay required, so a schema mistake fails to decode instead of becoming an
//! empty string. Unknown fields are ignored: these are projections, not the full schemas.
//!
//! Field names and meanings are Riot's (camelCase on the wire); `SOURCES.md` records each
//! table, so the fields are not documented again one by one.
#![allow(missing_docs)]

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt;

macro_rules! text_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub String);

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl From<&str> for $name {
            fn from(s: &str) -> Self {
                Self(s.to_owned())
            }
        }
    };
}

macro_rules! number_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub i64);

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Display::fmt(&self.0, f)
            }
        }
    };
}

text_id!(
    /// An encrypted PUUID ("Exact length of 78 characters"). Opaque; never parsed.
    Puuid
);
text_id!(
    /// A match id such as `EUW1_7000000001`.
    MatchId
);
text_id!(
    /// A tournament code string.
    TournamentCode
);
number_id!(
    /// A champion id (`long` in champion-mastery-v4).
    ChampionId
);
number_id!(
    /// A tournament id (`long` query parameter of createTournamentCode; `int` in the DTO).
    TournamentId
);
number_id!(
    /// A tournament provider id.
    ProviderId
);

/// account-v1 `AccountDto`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub puuid: Puuid,
    /// "May be excluded from the response if the account doesn't have a gameName."
    pub game_name: Option<String>,
    /// "May be excluded from the response if the account doesn't have a tagLine."
    pub tag_line: Option<String>,
}

/// summoner-v4 `SummonerDTO` (the current table: no summoner id, account id or name).
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Summoner {
    pub puuid: Puuid,
    #[serde(default)]
    pub profile_icon_id: i32,
    /// Epoch milliseconds.
    #[serde(default)]
    pub revision_date: i64,
    #[serde(default)]
    pub summoner_level: i64,
}

/// champion-mastery-v4 `ChampionMasteryDto` (projection).
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChampionMastery {
    pub puuid: Puuid,
    pub champion_id: ChampionId,
    #[serde(default)]
    pub champion_level: i32,
    #[serde(default)]
    pub champion_points: i32,
    /// Unix milliseconds.
    #[serde(default)]
    pub last_play_time: i64,
    #[serde(default)]
    pub tokens_earned: i32,
}

/// champion-v3 `ChampionInfo`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct ChampionRotation {
    /// Champions available to all players on Summoner's Rift (`sr`).
    #[serde(default, rename = "sr")]
    pub free: Vec<ChampionId>,
    /// Champions available to players under summoner level 11 (`newplayer`).
    #[serde(default, rename = "newplayer")]
    pub new_player: Vec<ChampionId>,
}

/// match-v5 `MatchDto` (projection: metadata and a summary of `info`).
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Match {
    pub metadata: MatchMetadata,
    pub info: MatchInfo,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchMetadata {
    pub match_id: MatchId,
    #[serde(default)]
    pub data_version: String,
    /// Participant PUUIDs.
    #[serde(default)]
    pub participants: Vec<Puuid>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchInfo {
    /// Unix milliseconds.
    #[serde(default)]
    pub game_creation: i64,
    /// Seconds for games after patch 11.20 (milliseconds before).
    #[serde(default)]
    pub game_duration: i64,
    #[serde(default)]
    pub game_mode: String,
    #[serde(default)]
    pub queue_id: i32,
    #[serde(default)]
    pub platform_id: String,
    #[serde(default)]
    pub participants: Vec<Participant>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Participant {
    pub puuid: Puuid,
    #[serde(default)]
    pub riot_id_game_name: String,
    #[serde(default)]
    pub riot_id_tagline: String,
    #[serde(default)]
    pub champion_name: String,
    #[serde(default)]
    pub team_id: i32,
    #[serde(default)]
    pub kills: i32,
    #[serde(default)]
    pub deaths: i32,
    #[serde(default)]
    pub assists: i32,
    #[serde(default)]
    pub win: bool,
}

/// tournament-stub-v5 `ProviderRegistrationParametersV5`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ProviderRegistration {
    pub region: TournamentRegion,
    /// Callback URL: "must be well-formed, use the http or https protocol, and use the default
    /// port for the protocol".
    pub url: String,
}

/// tournament-stub-v5 `TournamentRegistrationParametersV5`.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TournamentRegistration {
    pub provider_id: ProviderId,
    /// "The optional name of the tournament."
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// tournament-stub-v5 `TournamentCodeParametersV5`.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TournamentCodeParameters {
    /// "Optional list of encrypted puuids in order to validate the players eligible to join the
    /// lobby."
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_participants: Option<BTreeSet<Puuid>>,
    /// "Optional string that may contain any data in any format".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<String>,
    /// 1 to 5.
    pub team_size: u8,
    pub pick_type: PickType,
    pub map_type: MapType,
    pub spectator_type: SpectatorType,
    /// "Checks if allowed participants are enough to make full teams."
    pub enough_players: bool,
}

/// tournament-stub-v5 `TournamentCodeV5DTO` (projection).
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TournamentCodeInfo {
    pub code: TournamentCode,
    #[serde(default)]
    pub tournament_id: Option<TournamentId>,
    #[serde(default)]
    pub provider_id: Option<ProviderId>,
    #[serde(default)]
    pub team_size: i32,
    #[serde(default)]
    pub pick_type: String,
    #[serde(default)]
    pub map: String,
    #[serde(default)]
    pub lobby_name: String,
    #[serde(default)]
    pub participants: BTreeSet<Puuid>,
}

/// "The region in which the provider will be running tournaments."
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
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
    Ru,
    Tr,
    Kr,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PickType {
    BlindPick,
    DraftMode,
    AllRandom,
    TournamentDraft,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MapType {
    SummonersRift,
    HowlingAbyss,
    LeagueClassic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum SpectatorType {
    #[serde(rename = "NONE")]
    None,
    #[serde(rename = "LOBBYONLY")]
    LobbyOnly,
    #[serde(rename = "ALL")]
    All,
}

/// Queue type filter of match-v5 `getMatchIdsByPUUID` (`type`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchType {
    Ranked,
    Normal,
    Tourney,
    Tutorial,
}

impl fmt::Display for MatchType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Ranked => "ranked",
            Self::Normal => "normal",
            Self::Tourney => "tourney",
            Self::Tutorial => "tutorial",
        })
    }
}
