//! The complete Riot platform and regional API declaration.
//!
//! Required features: `json` and `rate-limit`. Mode: deterministic
//! planning plus optional manual live execution. Documentation owner:
//! `docs/recipes/riot.md`. Proving test: `tests/riot_contract.rs`.

use std::collections::HashMap;

use concord::api;
use concord::prelude::*;

use crate::riot::domain::*;

api! {
    client RiotApi {
        base "https://riotgames.com/";

        vars {
            platform: crate::riot::domain::PlatformRoute = crate::riot::domain::PlatformRoute::Euw1;
            regional: crate::riot::domain::RegionalRoute = crate::riot::domain::RegionalRoute::Europe;
        }

        credentials {
            secret api_key: String;
            credential riot_api_key = api_key(secret.api_key);
        }

        headers { "User-Agent" = "ConcordRiotExample/1.0"; }
        auth header "X-Riot-Token" = riot_api_key challenge never;
        timeout 20s;
    }

    scope platform() {
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

            GET masteries_by_puuid(puuid: String) -> Json<Vec<ChampionMasteryDto>> {
                path ["champion-masteries", "by-puuid", ep.puuid];
            }

            GET mastery_by_champion(puuid: String, champion_id: i64) -> Json<ChampionMasteryDto> {
                path ["champion-masteries", "by-puuid", ep.puuid, "by-champion", ep.champion_id];
            }

            GET top_masteries_by_puuid(puuid: String, count: Option<u32>) -> Json<Vec<ChampionMasteryDto>> {
                path ["champion-masteries", "by-puuid", ep.puuid, "top"];
                query { "count" = ep.count; }
            }

            GET mastery_score_by_puuid(puuid: String) -> Json<i32> {
                path ["scores", "by-puuid", ep.puuid];
            }
        }

        scope champion_v3() {
            path ["platform", "v3"];

            GET champion_rotations() -> Json<ChampionRotationsDto> {
                path ["champion-rotations"];
                rate_limit {
                    20000 / 10s;
                    1200000 / 10m;
                }
            }
        }

        scope clash_v1() {
            path ["clash", "v1"];

            GET clash_players_by_puuid(puuid: String) -> Json<Vec<ClashPlayerDto>> {
                path ["players", "by-puuid", ep.puuid];
                rate_limit {
                    20000 / 10s;
                    1200000 / 10m;
                }
            }

            GET clash_team(team_id: String) -> Json<ClashTeamDto> {
                path ["teams", ep.team_id];
                rate_limit { 200 / 1m; }
            }

            GET clash_tournament_by_team(team_id: String) -> Json<ClashTournamentDto> {
                path ["tournaments", "by-team", ep.team_id];
                rate_limit { 200 / 1m; }
            }

            GET clash_tournament(tournament_id: i64) -> Json<ClashTournamentDto> {
                path ["tournaments", ep.tournament_id];
                rate_limit { 10 / 1m; }
            }

            GET clash_tournaments() -> Json<Vec<ClashTournamentDto>> {
                path ["tournaments"];
                rate_limit { 10 / 1m; }
            }
        }

        scope league_v4() {
            path ["league", "v4"];

            GET challenger_by_queue(queue: crate::riot::domain::LeagueQueue) -> Json<LeagueListDto> {
                path ["challengerleagues", "by-queue", ep.queue];
                rate_limit {
                    30 / 10s;
                    500 / 10m;
                }
            }

            GET grandmaster_by_queue(queue: crate::riot::domain::LeagueQueue) -> Json<LeagueListDto> {
                path ["grandmasterleagues", "by-queue", ep.queue];
                rate_limit {
                    30 / 10s;
                    500 / 10m;
                }
            }

            GET master_by_queue(queue: crate::riot::domain::LeagueQueue) -> Json<LeagueListDto> {
                path ["masterleagues", "by-queue", ep.queue];
                rate_limit {
                    30 / 10s;
                    500 / 10m;
                }
            }

            GET league_entries(
                queue: crate::riot::domain::LeagueQueue,
                tier: crate::riot::domain::LeagueTier,
                division: crate::riot::domain::LeagueDivision,
                page: u32 = 1
            ) -> Json<Vec<LeagueEntryDto>> {
                path ["entries", ep.queue, ep.tier, ep.division];
                query { "page" = ep.page; }
                rate_limit { 50 / 10s; }
            }

            GET league_entries_by_puuid(puuid: String) -> Json<Vec<LeagueEntryDto>> {
                path ["entries", "by-puuid", ep.puuid];
                rate_limit {
                    20000 / 10s;
                    1200000 / 10m;
                }
            }

            GET league_by_id(league_id: String) -> Json<LeagueListDto> {
                path ["leagues", ep.league_id];
            }
        }

        scope league_exp_v4() {
            path ["league-exp", "v4"];

            GET league_exp_entries(
                queue: crate::riot::domain::LeagueQueue,
                tier: crate::riot::domain::LeagueTier,
                division: crate::riot::domain::LeagueDivision,
                page: u32 = 1
            ) -> Json<Vec<LeagueEntryDto>> {
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

            GET challenge_configs() -> Json<Vec<ChallengeConfigInfoDto>> {
                path ["challenges", "config"];
            }

            GET challenge_percentiles() -> Json<HashMap<String, HashMap<String, f64>>> {
                path ["challenges", "percentiles"];
            }

            GET challenge_config(challenge_id: i64) -> Json<ChallengeConfigInfoDto> {
                path ["challenges", ep.challenge_id, "config"];
            }

            GET challenge_leaderboard(
                challenge_id: i64,
                level: crate::riot::domain::ChallengeLevel,
                limit: Option<u32>
            ) -> Json<Vec<ApexPlayerInfoDto>> {
                path ["challenges", ep.challenge_id, "leaderboards", "by-level", ep.level];
                query { "limit" = ep.limit; }
            }

            GET challenge_percentiles_by_id(challenge_id: i64) -> Json<HashMap<String, f64>> {
                path ["challenges", ep.challenge_id, "percentiles"];
            }

            GET challenge_player_data(puuid: String) -> Json<PlayerInfoDto> {
                path ["player-data", ep.puuid];
            }
        }

        scope status_v4() {
            path ["status", "v4"];

            GET platform_data() -> Json<PlatformDataDto> {
                path ["platform-data"];
                rate_limit {
                    20000 / 10s;
                    1200000 / 10m;
                }
            }
        }

        scope spectator_v5() {
            path ["spectator", "v5"];

            GET active_game_by_puuid(puuid: String) -> Json<CurrentGameInfo> {
                path ["active-games", "by-summoner", ep.puuid];
                rate_limit {
                    3000 / 10s;
                    180000 / 10m;
                }
            }
        }

        scope summoner_v4() {
            path ["summoner", "v4", "summoners"];

            GET summoner_by_puuid(puuid: String) -> Json<SummonerDto> {
                path ["by-puuid", ep.puuid];
                rate_limit { 1600 / 1m; }
            }

            GET summoner_by_id(summoner_id: String) -> Json<SummonerDto> {
                path [ep.summoner_id];
                rate_limit { 1600 / 1m; }
            }
        }
    }


    scope regional() {
        host [vars.regional, "api"];
        rate_limit {
            500 / 10s;
            30000 / 10m;
        }

        scope account_v1() {
            path ["riot", "account", "v1"];

            GET account_by_riot_id(game_name: String, tag_line: String) -> Json<AccountDto> {
                path ["accounts", "by-riot-id", ep.game_name, ep.tag_line];
                rate_limit { 1000 / 1m; }
            }

            GET account_by_puuid(puuid: String) -> Json<AccountDto> {
                path ["accounts", "by-puuid", ep.puuid];
                rate_limit { 1000 / 1m; }
            }

            GET active_region(game: crate::riot::domain::RiotGame, puuid: String) -> Json<AccountRegionDto> {
                path ["region", "by-game", ep.game, "by-puuid", ep.puuid];
                rate_limit {
                    20000 / 10s;
                    1200000 / 10m;
                }
            }
        }

        scope match_v5() {
            path ["lol", "match", "v5", "matches"];

            GET match_ids_by_puuid(
                puuid: String,
                start_time: Option<i64>,
                end_time: Option<i64>,
                queue: Option<u32>,
                match_type: Option<crate::riot::domain::MatchType>,
                start: u64 = 0,
                count: std::num::NonZeroU32 = crate::riot::domain::DEFAULT_MATCH_PAGE_SIZE
            ) -> Json<Vec<String>> {
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
                paginate concord::pagination::OffsetPagination {
                    offset: bind ep.start,
                    page_size: bind ep.count,
                }
            }

            GET replays_by_puuid(puuid: String) -> Json<ReplayDto> {
                path ["by-puuid", ep.puuid, "replays"];
                rate_limit {
                    20000 / 10s;
                    1200000 / 10m;
                }
            }

            GET match_by_id(match_id: String) -> Json<MatchDto> {
                path [ep.match_id];
                rate_limit { 2000 / 10s; }
            }

            GET timeline_by_id(match_id: String) -> Json<TimelineDto> {
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

            POST create_stub_codes(
                tournament_id: i64,
                count: Option<u32>,
                #[body] body: Json<TournamentCodeParameters>
            ) -> Json<Vec<String>> {
                path ["codes"];
                query {
                    "tournamentId" = ep.tournament_id;
                    "count" = ep.count;
                }
            }

            GET stub_code(tournament_code: String) -> Json<TournamentCodeDto> {
                path ["codes", ep.tournament_code];
            }

            GET stub_lobby_events(tournament_code: String) -> Json<LobbyEventListDto> {
                path ["lobby-events", "by-code", ep.tournament_code];
            }

            POST register_stub_provider(#[body] body: Json<ProviderRegistrationParameters>) -> Json<i32> {
                path ["providers"];
            }

            POST register_stub_tournament(#[body] body: Json<TournamentRegistrationParameters>) -> Json<i32> {
                path ["tournaments"];
            }
        }

        scope tournament_v5() {
            path ["lol", "tournament", "v5"];

            POST create_tournament_codes(
                tournament_id: i64,
                count: Option<u32>,
                #[body] body: Json<TournamentCodeParameters>
            ) -> Json<Vec<String>> {
                path ["codes"];
                query {
                    "tournamentId" = ep.tournament_id;
                    "count" = ep.count;
                }
            }

            GET tournament_code(tournament_code: String) -> Json<TournamentCodeDto> {
                path ["codes", ep.tournament_code];
            }

            PUT update_tournament_code(
                tournament_code: String,
                #[body] body: Json<TournamentCodeUpdateParameters>
            ) -> NoContent {
                path ["codes", ep.tournament_code];
            }

            GET tournament_game_by_code(tournament_code: String) -> Json<TournamentGameDto> {
                path ["games", "by-code", ep.tournament_code];
            }

            GET tournament_lobby_events(tournament_code: String) -> Json<LobbyEventListDto> {
                path ["lobby-events", "by-code", ep.tournament_code];
            }

            POST register_tournament_provider(#[body] body: Json<ProviderRegistrationParameters>) -> Json<i32> {
                path ["providers"];
            }

            POST register_tournament(#[body] body: Json<TournamentRegistrationParameters>) -> Json<i32> {
                path ["tournaments"];
            }
        }
    }
}
