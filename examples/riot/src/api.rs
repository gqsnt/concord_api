//! The declarations: the Riot API (API key, regional and platform routing) and Data Dragon (a
//! separate, credential-free host). Every operation is sourced in `SOURCES.md`.

use concord::bytes::Bytes;
use concord::reqwest::header::{ACCEPT, AUTHORIZATION, HeaderName};
use concord::{Binary, Json};

use crate::routing::{AccountRoute, MatchRoute, Platform};
use crate::types::*;

/// The header carrying a Riot API key. Riot's portal recommends sending the key as a header
/// rather than a query parameter; the key is not an `Authorization` credential.
pub const X_RIOT_TOKEN: HeaderName = HeaderName::from_static("x-riot-token");

concord::api! {
    /// The Riot Games API, League of Legends subset (see `SOURCES.md`).
    ///
    /// The key is `#[sensitive]`: it is sent on every request in `x-riot-token` and withheld
    /// from Concord's `Debug` output and errors. Each scope takes the routing value its
    /// operations list (they differ per operation); the value becomes the host's first label,
    /// `{route}.api.riotgames.com`.
    pub client Riot(#[sensitive] header(X_RIOT_TOKEN) key: String) {
        base "https://api.riotgames.com";
        headers { ACCEPT = "application/json"; }
        format Json;

        /// account-v1, regional routing (americas, asia, europe; any account from any).
        scope account(route: AccountRoute) {
            subdomain "{route}";
            path "riot/account/v1/accounts";

            /// The account of a Riot ID (`gameName#tagLine`).
            GET by_riot_id(game_name: String, tag_line: String) -> Account {
                path "by-riot-id/{game_name}/{tag_line}";
            }

            /// The account of a PUUID.
            GET by_puuid(puuid: Puuid) -> Account { path "by-puuid/{puuid}"; }

            /// The account of a Riot Sign On access token (a separate access mode: the
            /// application already holds the token; Concord does not obtain or refresh it).
            /// Sent with `Authorization: Bearer …` instead of the API key.
            GET me(#[sensitive] access_token: String) -> Account {
                path "me";
                headers {
                    remove X_RIOT_TOKEN;
                    AUTHORIZATION = "Bearer {access_token}";
                }
            }
        }

        /// Platform routing (summoner-v4, champion-mastery-v4, champion-v3).
        scope platform(route: Platform) {
            subdomain "{route}";
            path "lol";

            /// summoner-v4: the summoner of a PUUID on this platform.
            GET summoner(puuid: Puuid) -> Summoner {
                path "summoner/v4/summoners/by-puuid/{puuid}";
            }

            /// champion-mastery-v4: the highest masteries (Riot's default count is 3).
            GET top_masteries(puuid: Puuid, query count?: u32) -> Vec<ChampionMastery> {
                path "champion-mastery/v4/champion-masteries/by-puuid/{puuid}/top";
            }

            /// champion-v3: the free champion rotation.
            GET rotation() -> ChampionRotation { path "platform/v3/champion-rotations"; }
        }

        /// match-v5, regional routing (americas, asia, europe, sea).
        scope matches(route: MatchRoute) {
            subdomain "{route}";
            path "lol/match/v5/matches";

            /// Match ids of a PUUID, newest first. Riot's defaults: `start` 0, `count` 20
            /// (0 to 100); unset filters are not sent.
            GET ids(
                puuid: Puuid,
                query("startTime") start_time?: i64,
                query("endTime") end_time?: i64,
                query queue?: i32,
                query("type") kind?: MatchType,
                query start?: u32,
                query count?: u32,
            ) -> Vec<MatchId> {
                path "by-puuid/{puuid}/ids";
            }

            /// A match.
            GET get(id: MatchId) -> Match { path "{id}"; }
        }

        /// tournament-stub-v5: the test double of the tournament API. Its routing value is not
        /// stated by the reference (see `SOURCES.md`); the caller supplies it. Only exercised
        /// against local fixtures here.
        scope tournament_stub(route: AccountRoute) {
            subdomain "{route}";
            path "lol/tournament-stub/v5";

            /// Registers a provider (its callback URL) and returns its id.
            POST register_provider(body provider: ProviderRegistration) -> ProviderId {
                path "providers";
            }

            /// Registers a tournament for a provider and returns its id.
            POST register_tournament(body tournament: TournamentRegistration) -> TournamentId {
                path "tournaments";
            }

            /// Creates tournament codes (at most 1000 per call).
            POST create_codes(
                query("tournamentId") tournament: TournamentId,
                query count?: u32,
                body parameters: TournamentCodeParameters,
            ) -> Vec<TournamentCode> {
                path "codes";
            }

            /// The settings of a tournament code.
            GET code(code: TournamentCode) -> TournamentCodeInfo { path "codes/{code}"; }
        }
    }
}

concord::api! {
    /// Data Dragon: Riot's static data and assets. A different host with no credentials: a
    /// separate client, so the API key can never be sent there.
    pub client DataDragon {
        base "https://ddragon.leagueoflegends.com";
        response Json;

        /// Every Data Dragon version, newest first.
        GET versions() -> Vec<String> { path "api/versions.json"; }

        /// A champion's square image (PNG bytes).
        GET champion_square(version: String, champion: String) -> Bytes {
            path "cdn/{version}/img/champion/{champion}.png";
            response Binary;
        }
    }
}
