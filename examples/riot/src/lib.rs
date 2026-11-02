//! A typed Riot Games client built with Concord, and two application workflows.
//!
//! - [`api`] — the declarations: [`Riot`] (API key, regional and platform routing) and
//!   [`DataDragon`] (credential-free static data on its own host).
//! - [`types`] — documented projections of Riot's DTOs; [`routing`] — one type per published
//!   route list.
//! - [`player_summary`] — Riot ID → PUUID → summoner, top masteries, a bounded list of recent
//!   matches; [`tournament_codes`] — the body-bearing tournament-stub call.
//!
//! The workflows are ordinary application code: request budgets, stopping on `429`, and choosing
//! routes are decisions made here, not in Concord. Sources for every operation and field are in
//! `SOURCES.md`; the tests run offline against synthetic fixtures.

pub mod api;
pub mod routing;
pub mod types;

pub use api::{DataDragon, Riot, RiotConfig, X_RIOT_TOKEN};
pub use routing::{AccountRoute, MatchRoute, Platform};
pub use types::*;

use concord::reqwest::StatusCode;
use concord::reqwest::header::RETRY_AFTER;

/// At most this many match details are fetched by [`player_summary`].
pub const MAX_MATCHES: u32 = 5;

/// A player's profile and recent games.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerSummary {
    /// The account of the Riot ID.
    pub account: Account,
    /// The summoner on the requested platform.
    pub summoner: Summoner,
    /// The three highest champion masteries.
    pub top_masteries: Vec<ChampionMastery>,
    /// Recent games, newest first.
    pub recent: Vec<MatchLine>,
}

/// One recent game, from the player's side.
#[derive(Debug, Clone, PartialEq)]
pub struct MatchLine {
    /// The match.
    pub id: MatchId,
    /// Riot's game mode (`CLASSIC`, `ARAM`, ...).
    pub mode: String,
    /// The player's champion.
    pub champion: String,
    /// The player's kills.
    pub kills: i32,
    /// The player's deaths.
    pub deaths: i32,
    /// The player's assists.
    pub assists: i32,
    /// Whether the player's team won.
    pub win: bool,
}

/// How a workflow stopped early.
#[derive(Debug)]
pub enum Stop {
    /// Riot answered `429`: stop, and wait `retry_after` seconds before any further call (Riot's
    /// policy). The workflow does not retry.
    RateLimited {
        /// Seconds from Riot's `Retry-After` header, when present and numeric.
        retry_after: Option<u64>,
    },
    /// Any other error (status, transport, decoding), as Concord reported it.
    Failed(concord::Error),
}

impl From<concord::Error> for Stop {
    fn from(error: concord::Error) -> Self {
        // A 429's response is unread; its headers are borrowed from the error.
        if error.status() != Some(StatusCode::TOO_MANY_REQUESTS) {
            return Stop::Failed(error);
        }
        Stop::RateLimited {
            retry_after: error
                .response()
                .and_then(|r| r.headers().get(RETRY_AFTER))
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse().ok()),
        }
    }
}

/// Resolves a Riot ID and reads its summoner, top masteries and up to `matches` recent games
/// ([`MAX_MATCHES`] at most): `4 + matches` requests in all.
///
/// The account route only locates the account; the platform and match routes are the player's
/// own (an account's route does not tell which platform a player plays on), so the caller
/// supplies them.
pub async fn player_summary(
    riot: &Riot,
    game_name: &str,
    tag_line: &str,
    account_route: AccountRoute,
    platform: Platform,
    match_route: MatchRoute,
    matches: u32,
) -> Result<PlayerSummary, Stop> {
    let account = riot
        .account(account_route)
        .by_riot_id(game_name, tag_line)
        .send()
        .await?;
    let on_platform = riot.platform(platform);
    let summoner = on_platform.summoner(account.puuid.clone()).send().await?;
    let top_masteries = on_platform
        .top_masteries(account.puuid.clone())
        .set_count(3)
        .send()
        .await?;
    let history = riot.matches(match_route);
    let ids = history
        .ids(account.puuid.clone())
        .set_count(matches.min(MAX_MATCHES))
        .send()
        .await?;
    let mut recent = Vec::new();
    for id in ids.into_iter().take(MAX_MATCHES as usize) {
        let game = history.get(id.clone()).send().await?;
        if let Some(me) = game
            .info
            .participants
            .iter()
            .find(|p| p.puuid == account.puuid)
        {
            recent.push(MatchLine {
                id,
                mode: game.info.game_mode.clone(),
                champion: me.champion_name.clone(),
                kills: me.kills,
                deaths: me.deaths,
                assists: me.assists,
                win: me.win,
            });
        }
    }
    Ok(PlayerSummary {
        account,
        summoner,
        top_masteries,
        recent,
    })
}

/// Creates `count` tournament codes with the tournament stub (a JSON body plus query
/// parameters). Only ever called against local fixtures in this example.
pub async fn tournament_codes(
    riot: &Riot,
    route: AccountRoute,
    tournament: TournamentId,
    parameters: TournamentCodeParameters,
    count: u32,
) -> Result<Vec<TournamentCode>, Stop> {
    Ok(riot
        .tournament_stub(route)
        .create_codes(tournament, parameters)
        .set_count(count)
        .send()
        .await?)
}
