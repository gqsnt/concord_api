//! The complete Riot platform and regional API declaration.
//!
//! Required features: `json` and `rate-limit`. Mode: deterministic
//! planning plus optional manual live execution. Documentation owner:
//! `docs/spec.md`. Proving test: `tests/riot_contract.rs`.
//! Rate windows and partitioning are illustrative Concord policy, not a
//! verified statement of Riot's current service quotas.

use std::collections::HashMap;

use concord::api;
use concord::prelude::*;

use crate::riot::domain::*;

api! {
    // ANCHOR: riot_client
    client RiotApi {
        base "https://riotgames.com/";

        values {

        }

        credentials {
            secret api_key;
            credential riot_api_key = api_key(secret.api_key);
        }

        headers { "User-Agent" = "ConcordRiotExample/1.0"; }
        auth { header "X-Riot-Token" = riot_api_key; }
        timeout 20s;

    // ANCHOR_END: riot_client

    scope platform( platform: crate::riot::domain::PlatformRoute) {
        subdomain replace [platform.platform, "api"];
        path ["lol"];
        rate_limit {
            key [host];
            allow 500 per 10s;
            allow 30000 per 10m;
        }

        scope champion_mastery_v4() {
            path ["champion-mastery", "v4"];
            rate_limit {
                allow 20000 per 10s;
                allow 1200000 per 10m;
            }

            GET masteries_by_puuid(puuid: String) -> Vec<ChampionMasteryDto> using Json<Vec<ChampionMasteryDto>> {
                path ["champion-masteries", "by-puuid", endpoint.puuid];
            }

            GET mastery_by_champion(puuid: String, champion_id: i64) -> ChampionMasteryDto using Json<ChampionMasteryDto> {
                path ["champion-masteries", "by-puuid", endpoint.puuid, "by-champion", endpoint.champion_id];
            }

            GET top_masteries_by_puuid(puuid: String, count?: u32) -> Vec<ChampionMasteryDto> using Json<Vec<ChampionMasteryDto>> {
                path ["champion-masteries", "by-puuid", endpoint.puuid, "top"];
                query { "count" = endpoint.count; }
            }

            GET mastery_score_by_puuid(puuid: String) -> i32 using Json<i32> {
                path ["scores", "by-puuid", endpoint.puuid];
            }
        }

        scope champion_v3() {
            path ["platform", "v3"];

            GET champion_rotations() -> ChampionRotationsDto using Json<ChampionRotationsDto> {
                path ["champion-rotations"];
                rate_limit {
                    allow 20000 per 10s;
                    allow 1200000 per 10m;
                }
            }
        }

        scope clash_v1() {
            path ["clash", "v1"];

            GET clash_players_by_puuid(puuid: String) -> Vec<ClashPlayerDto> using Json<Vec<ClashPlayerDto>> {
                path ["players", "by-puuid", endpoint.puuid];
                rate_limit {
                    allow 20000 per 10s;
                    allow 1200000 per 10m;
                }
            }

            GET clash_team(team_id: String) -> ClashTeamDto using Json<ClashTeamDto> {
                path ["teams", endpoint.team_id];
                rate_limit { allow 200 per 1m; }
            }

            GET clash_tournament_by_team(team_id: String) -> ClashTournamentDto using Json<ClashTournamentDto> {
                path ["tournaments", "by-team", endpoint.team_id];
                rate_limit { allow 200 per 1m; }
            }

            GET clash_tournament(tournament_id: i64) -> ClashTournamentDto using Json<ClashTournamentDto> {
                path ["tournaments", endpoint.tournament_id];
                rate_limit { allow 10 per 1m; }
            }

            GET clash_tournaments() -> Vec<ClashTournamentDto> using Json<Vec<ClashTournamentDto>> {
                path ["tournaments"];
                rate_limit { allow 10 per 1m; }
            }
        }

        scope league_v4() {
            path ["league", "v4"];

            GET challenger_by_queue(queue: crate::riot::domain::LeagueQueue) -> LeagueListDto using Json<LeagueListDto> {
                path ["challengerleagues", "by-queue", endpoint.queue];
                rate_limit {
                    allow 30 per 10s;
                    allow 500 per 10m;
                }
            }

            GET grandmaster_by_queue(queue: crate::riot::domain::LeagueQueue) -> LeagueListDto using Json<LeagueListDto> {
                path ["grandmasterleagues", "by-queue", endpoint.queue];
                rate_limit {
                    allow 30 per 10s;
                    allow 500 per 10m;
                }
            }

            GET master_by_queue(queue: crate::riot::domain::LeagueQueue) -> LeagueListDto using Json<LeagueListDto> {
                path ["masterleagues", "by-queue", endpoint.queue];
                rate_limit {
                    allow 30 per 10s;
                    allow 500 per 10m;
                }
            }

            GET league_entries(
                queue: crate::riot::domain::LeagueQueue,
                tier: crate::riot::domain::LeagueTier,
                division: crate::riot::domain::LeagueDivision,
                page: u32 = 1
            ) -> Vec<LeagueEntryDto> using Json<Vec<LeagueEntryDto>> {
                path ["entries", endpoint.queue, endpoint.tier, endpoint.division];
                query { "page" = endpoint.page; }
                rate_limit { allow 50 per 10s; }
            }

            GET league_entries_by_puuid(puuid: String) -> Vec<LeagueEntryDto> using Json<Vec<LeagueEntryDto>> {
                path ["entries", "by-puuid", endpoint.puuid];
                rate_limit {
                    allow 20000 per 10s;
                    allow 1200000 per 10m;
                }
            }

            GET league_by_id(league_id: String) -> LeagueListDto using Json<LeagueListDto> {
                path ["leagues", endpoint.league_id];
            }
        }

        scope league_exp_v4() {
            path ["league-exp", "v4"];

            GET league_exp_entries(
                queue: crate::riot::domain::LeagueQueue,
                tier: crate::riot::domain::LeagueTier,
                division: crate::riot::domain::LeagueDivision,
                page: u32 = 1
            ) -> Vec<LeagueEntryDto> using Json<Vec<LeagueEntryDto>> {
                path ["entries", endpoint.queue, endpoint.tier, endpoint.division];
                query { "page" = endpoint.page; }
                rate_limit { allow 50 per 10s; }
            }
        }

        scope challenges_v1() {
            path ["challenges", "v1"];
            rate_limit {
                allow 20000 per 10s;
                allow 1200000 per 10m;
            }

            GET challenge_configs() -> Vec<ChallengeConfigInfoDto> using Json<Vec<ChallengeConfigInfoDto>> {
                path ["challenges", "config"];
            }

            GET challenge_percentiles() -> HashMap<String, HashMap<String, f64>> using Json<HashMap<String, HashMap<String, f64>>> {
                path ["challenges", "percentiles"];
            }

            GET challenge_config(challenge_id: i64) -> ChallengeConfigInfoDto using Json<ChallengeConfigInfoDto> {
                path ["challenges", endpoint.challenge_id, "config"];
            }

            GET challenge_leaderboard(
                challenge_id: i64,
                level: crate::riot::domain::ChallengeLevel,
                limit?: u32
            ) -> Vec<ApexPlayerInfoDto> using Json<Vec<ApexPlayerInfoDto>> {
                path ["challenges", endpoint.challenge_id, "leaderboards", "by-level", endpoint.level];
                query { "limit" = endpoint.limit; }
            }

            GET challenge_percentiles_by_id(challenge_id: i64) -> HashMap<String, f64> using Json<HashMap<String, f64>> {
                path ["challenges", endpoint.challenge_id, "percentiles"];
            }

            GET challenge_player_data(puuid: String) -> PlayerInfoDto using Json<PlayerInfoDto> {
                path ["player-data", endpoint.puuid];
            }
        }

        scope status_v4() {
            path ["status", "v4"];

            GET platform_data() -> PlatformDataDto using Json<PlatformDataDto> {
                path ["platform-data"];
                rate_limit {
                    allow 20000 per 10s;
                    allow 1200000 per 10m;
                }
            }
        }

        scope spectator_v5() {
            path ["spectator", "v5"];

            GET active_game_by_puuid(puuid: String) -> CurrentGameInfo using Json<CurrentGameInfo> {
                path ["active-games", "by-summoner", endpoint.puuid];
                rate_limit {
                    allow 3000 per 10s;
                    allow 180000 per 10m;
                }
            }
        }

        scope summoner_v4() {
            path ["summoner", "v4", "summoners"];

            GET summoner_by_puuid(puuid: String) -> SummonerDto using Json<SummonerDto> {
                path ["by-puuid", endpoint.puuid];
                rate_limit { allow 1600 per 1m; }
            }

            GET summoner_by_id(summoner_id: String) -> SummonerDto using Json<SummonerDto> {
                path [endpoint.summoner_id];
                rate_limit { allow 1600 per 1m; }
            }
        }
    }


    scope regional(regional: crate::riot::domain::RegionalRoute) {
        subdomain replace [regional.regional, "api"];
        rate_limit {
            key [host];
            allow 500 per 10s;
            allow 30000 per 10m;
        }

        scope account_v1() {
            path ["riot", "account", "v1"];

            GET account_by_riot_id(game_name: String, tag_line: String) -> AccountDto using Json<AccountDto> {
                path ["accounts", "by-riot-id", endpoint.game_name, endpoint.tag_line];
                rate_limit { allow 1000 per 1m; }
            }

            GET account_by_puuid(puuid: String) -> AccountDto using Json<AccountDto> {
                path ["accounts", "by-puuid", endpoint.puuid];
                rate_limit { allow 1000 per 1m; }
            }

            GET active_region(game: crate::riot::domain::RiotGame, puuid: String) -> AccountRegionDto using Json<AccountRegionDto> {
                path ["region", "by-game", endpoint.game, "by-puuid", endpoint.puuid];
                rate_limit {
                    allow 20000 per 10s;
                    allow 1200000 per 10m;
                }
            }
        }

        scope match_v5() {
            path ["lol", "match", "v5", "matches"];

            // ANCHOR: riot_match_ids_endpoint
            GET match_ids_by_puuid(
                puuid: String,
                start_time?: i64,
                end_time?: i64,
                queue?: u32,
                match_type?: crate::riot::domain::MatchType,
                start: u64 = 0,
                count: std::num::NonZeroU32 = crate::riot::domain::DEFAULT_MATCH_PAGE_SIZE
            ) -> Vec<String> using Json<Vec<String>> {
                path ["by-puuid", endpoint.puuid, "ids"];
                query {
                    "startTime" = endpoint.start_time;
                    "endTime" = endpoint.end_time;
                    "queue" = endpoint.queue;
                    "type" = endpoint.match_type;
                    "start" = endpoint.start;
                    "count" = endpoint.count;
                }
                rate_limit { allow 2000 per 10s; }
                paginate with concord::pagination::OffsetPagination {
                    offset -> endpoint.start;
                    page_size -> endpoint.count;
                }
            }
            // ANCHOR_END: riot_match_ids_endpoint

            GET replays_by_puuid(puuid: String) -> ReplayDto using Json<ReplayDto> {
                path ["by-puuid", endpoint.puuid, "replays"];
                rate_limit {
                    allow 20000 per 10s;
                    allow 1200000 per 10m;
                }
            }

            GET match_by_id(match_id: String) -> MatchDto using Json<MatchDto> {
                path [endpoint.match_id];
                rate_limit { allow 2000 per 10s; }
            }

            GET timeline_by_id(match_id: String) -> TimelineDto using Json<TimelineDto> {
                path [endpoint.match_id, "timeline"];
                rate_limit { allow 2000 per 10s; }
            }
        }

        scope tournament_stub_v5() {
            path ["lol", "tournament-stub", "v5"];
            rate_limit {
                allow 20000 per 10s;
                allow 1200000 per 10m;
            }

            POST create_stub_codes(
                tournament_id: i64,
                count?: u32,
                body body: TournamentCodeParameters using Json<TournamentCodeParameters>
            ) -> Vec<String> using Json<Vec<String>> {
                path ["codes"];
                query {
                    "tournamentId" = endpoint.tournament_id;
                    "count" = endpoint.count;
                }
            }

            GET stub_code(tournament_code: String) -> TournamentCodeDto using Json<TournamentCodeDto> {
                path ["codes", endpoint.tournament_code];
            }

            GET stub_lobby_events(tournament_code: String) -> LobbyEventListDto using Json<LobbyEventListDto> {
                path ["lobby-events", "by-code", endpoint.tournament_code];
            }

            POST register_stub_provider(body body: ProviderRegistrationParameters using Json<ProviderRegistrationParameters>) -> i32 using Json<i32> {
                path ["providers"];
            }

            POST register_stub_tournament(body body: TournamentRegistrationParameters using Json<TournamentRegistrationParameters>) -> i32 using Json<i32> {
                path ["tournaments"];
            }
        }

        scope tournament_v5() {
            path ["lol", "tournament", "v5"];

            POST create_tournament_codes(
                tournament_id: i64,
                count?: u32,
                body body: TournamentCodeParameters using Json<TournamentCodeParameters>
            ) -> Vec<String> using Json<Vec<String>> {
                path ["codes"];
                query {
                    "tournamentId" = endpoint.tournament_id;
                    "count" = endpoint.count;
                }
            }

            GET tournament_code(tournament_code: String) -> TournamentCodeDto using Json<TournamentCodeDto> {
                path ["codes", endpoint.tournament_code];
            }

            PUT update_tournament_code(
                tournament_code: String,
                body body: TournamentCodeUpdateParameters using Json<TournamentCodeUpdateParameters>
            ) -> () using NoContent {
                path ["codes", endpoint.tournament_code];
            }

            GET tournament_game_by_code(tournament_code: String) -> TournamentGameDto using Json<TournamentGameDto> {
                path ["games", "by-code", endpoint.tournament_code];
            }

            GET tournament_lobby_events(tournament_code: String) -> LobbyEventListDto using Json<LobbyEventListDto> {
                path ["lobby-events", "by-code", endpoint.tournament_code];
            }

            POST register_tournament_provider(body body: ProviderRegistrationParameters using Json<ProviderRegistrationParameters>) -> i32 using Json<i32> {
                path ["providers"];
            }

            POST register_tournament(body body: TournamentRegistrationParameters using Json<TournamentRegistrationParameters>) -> i32 using Json<i32> {
                path ["tournaments"];
            }
        }
    }
}}
