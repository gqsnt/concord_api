//! The complete Riot platform and regional API declaration.
//!
//! Required features: `json` and `rate-limit`. Mode: deterministic
//! planning plus optional manual live execution. Documentation owners:
//! `docs/spec.md` and `docs/riot_walkthrough.md`. Proving tests:
//! `tests/riot_contract.rs` and `tests/baseline_recording.rs`.
//! Rate windows and partitioning are illustrative Concord policy, not a
//! verified statement of Riot's current service quotas.
//!
//! The declaration was first converted from the pinned v2.1 source without
//! any meaning change (spec §24.1) and then given the separate showcase
//! cleanup of §24.3: numeric windows are factored into reusable window sets,
//! single-owner query values use direct placement, and domain types use their
//! imported names. Every endpoint still keeps its method, route, query/header
//! programs in order, private API-key placement, timeout, representation
//! defaults and fences, derived endpoint identity, and every rate policy ID,
//! key, window, cost and original sharing group; `tests/baseline_recording.rs`
//! proves state-equivalence against the captured v2.1 recording.

use std::collections::HashMap;
use std::num::NonZeroU32;

use concord::api;
use concord::prelude::*;

use crate::riot::domain::*;

api! {
    // ANCHOR: riot_client
    client RiotApi {
        base "https://riotgames.com/";

        credentials {
            secret api_key;
            credential riot_api_key = api_key(secret.api_key);
        }

        headers { "User-Agent" = "ConcordRiotExample/2.1"; }
        auth { header "X-Riot-Token" = riot_api_key; }
        timeout 20s;
        request Json;
        response Json;

        // Window sets hold numbers only. They share no state: every
        // attachment below names its own policy ID, and the ID with the
        // ordered key alone selects quota state.
        rate_limits {
            windows application { allow 500 per 10s; allow 30000 per 10m; }
            windows standard { allow 20000 per 10s; allow 1200000 per 10m; }
            windows apex_leagues { allow 30 per 10s; allow 500 per 10m; }
            windows league_entries { allow 50 per 10s; }
            windows clash_teams { allow 200 per 1m; }
            windows clash_tournaments { allow 10 per 1m; }
            windows spectator { allow 3000 per 10s; allow 180000 per 10m; }
            windows summoners { allow 1600 per 1m; }
            windows accounts { allow 1000 per 1m; }
            windows match_reads { allow 2000 per 10s; }
        }
    // ANCHOR_END: riot_client

    scope platform(platform: PlatformRoute) {
        subdomain replace [platform, "api"];
        path "lol";
        // The platform application bucket, one per routing host.
        rate_limit "concord.riot.1" key [origin] using application;

        scope champion_mastery_v4() {
            path "champion-mastery/v4";
            // Scope-shared method bucket (one of the three original groups).
            rate_limit "concord.riot.2" key [origin] using standard;

            GET masteries_by_puuid(puuid: String) -> Vec<ChampionMasteryDto> {
                path "champion-masteries/by-puuid/{puuid}";
            }

            GET mastery_by_champion(puuid: String, champion_id: i64) -> ChampionMasteryDto {
                path "champion-masteries/by-puuid/{puuid}/by-champion/{champion_id}";
            }

            GET top_masteries_by_puuid(puuid: String, query count?: u32)
                -> Vec<ChampionMasteryDto>
            {
                path "champion-masteries/by-puuid/{puuid}/top";
            }

            GET mastery_score_by_puuid(puuid: String) -> i32 {
                path "scores/by-puuid/{puuid}";
            }
        }

        scope champion_v3() {
            path "platform/v3";

            GET champion_rotations() -> ChampionRotationsDto {
                path "champion-rotations";
                rate_limit "concord.riot.3" key [origin] using standard;
            }
        }

        scope clash_v1() {
            path "clash/v1";

            GET clash_players_by_puuid(puuid: String) -> Vec<ClashPlayerDto> {
                path "players/by-puuid/{puuid}";
                rate_limit "concord.riot.4" key [origin] using standard;
            }

            GET clash_team(team_id: String) -> ClashTeamDto {
                path "teams/{team_id}";
                rate_limit "concord.riot.5" key [origin] using clash_teams;
            }

            GET clash_tournament_by_team(team_id: String) -> ClashTournamentDto {
                path "tournaments/by-team/{team_id}";
                rate_limit "concord.riot.6" key [origin] using clash_teams;
            }

            GET clash_tournament(tournament_id: i64) -> ClashTournamentDto {
                path "tournaments/{tournament_id}";
                rate_limit "concord.riot.7" key [origin] using clash_tournaments;
            }

            GET clash_tournaments() -> Vec<ClashTournamentDto> {
                path "tournaments";
                rate_limit "concord.riot.8" key [origin] using clash_tournaments;
            }
        }

        scope league_v4() {
            path "league/v4";

            GET challenger_by_queue(queue: LeagueQueue) -> LeagueListDto {
                path "challengerleagues/by-queue/{queue}";
                rate_limit "concord.riot.9" key [origin] using apex_leagues;
            }

            GET grandmaster_by_queue(queue: LeagueQueue) -> LeagueListDto {
                path "grandmasterleagues/by-queue/{queue}";
                rate_limit "concord.riot.10" key [origin] using apex_leagues;
            }

            GET master_by_queue(queue: LeagueQueue) -> LeagueListDto {
                path "masterleagues/by-queue/{queue}";
                rate_limit "concord.riot.11" key [origin] using apex_leagues;
            }

            GET league_entries(
                queue: LeagueQueue,
                tier: LeagueTier,
                division: LeagueDivision,
                query page: u32 = 1,
            ) -> Vec<LeagueEntryDto> {
                path "entries/{queue}/{tier}/{division}";
                rate_limit "concord.riot.12" key [origin] using league_entries;
            }

            GET league_entries_by_puuid(puuid: String) -> Vec<LeagueEntryDto> {
                path "entries/by-puuid/{puuid}";
                rate_limit "concord.riot.13" key [origin] using standard;
            }

            GET league_by_id(league_id: String) -> LeagueListDto {
                path "leagues/{league_id}";
            }
        }

        scope league_exp_v4() {
            path "league-exp/v4";

            GET league_exp_entries(
                queue: LeagueQueue,
                tier: LeagueTier,
                division: LeagueDivision,
                query page: u32 = 1,
            ) -> Vec<LeagueEntryDto> {
                path "entries/{queue}/{tier}/{division}";
                rate_limit "concord.riot.14" key [origin] using league_entries;
            }
        }

        scope challenges_v1() {
            path "challenges/v1";
            // Scope-shared method bucket (one of the three original groups).
            rate_limit "concord.riot.15" key [origin] using standard;

            GET challenge_configs() -> Vec<ChallengeConfigInfoDto> {
                path "challenges/config";
            }

            GET challenge_percentiles() -> HashMap<String, HashMap<String, f64>> {
                path "challenges/percentiles";
            }

            GET challenge_config(challenge_id: i64) -> ChallengeConfigInfoDto {
                path "challenges/{challenge_id}/config";
            }

            GET challenge_leaderboard(
                challenge_id: i64,
                level: ChallengeLevel,
                query limit?: u32,
            ) -> Vec<ApexPlayerInfoDto> {
                path "challenges/{challenge_id}/leaderboards/by-level/{level}";
            }

            GET challenge_percentiles_by_id(challenge_id: i64) -> HashMap<String, f64> {
                path "challenges/{challenge_id}/percentiles";
            }

            GET challenge_player_data(puuid: String) -> PlayerInfoDto {
                path "player-data/{puuid}";
            }
        }

        scope status_v4() {
            path "status/v4";
            // A deliberately repeated default: its provenance is this scope.
            response Json;

            GET platform_data() -> PlatformDataDto {
                path "platform-data";
                rate_limit "concord.riot.16" key [origin] using standard;
            }
        }

        scope spectator_v5() {
            path "spectator/v5";

            GET active_game_by_puuid(puuid: String) -> CurrentGameInfo {
                path "active-games/by-summoner/{puuid}";
                rate_limit "concord.riot.17" key [origin] using spectator;
            }
        }

        scope summoner_v4() {
            path "summoner/v4/summoners";

            GET summoner_by_puuid(puuid: String) -> SummonerDto {
                path "by-puuid/{puuid}";
                rate_limit "concord.riot.18" key [origin] using summoners;
            }

            GET summoner_by_id(summoner_id: String) -> SummonerDto {
                path "{summoner_id}";
                rate_limit "concord.riot.19" key [origin] using summoners;
            }
        }
    }

    scope regional(regional: RegionalRoute) {
        subdomain replace [regional, "api"];
        // The regional application bucket, one per routing host.
        rate_limit "concord.riot.20" key [origin] using application;

        scope account_v1() {
            path "riot/account/v1";

            GET account_by_riot_id(game_name: String, tag_line: String) -> AccountDto {
                path "accounts/by-riot-id/{game_name}/{tag_line}";
                rate_limit "concord.riot.21" key [origin] using accounts;
            }

            GET account_by_puuid(puuid: String) -> AccountDto {
                path "accounts/by-puuid/{puuid}";
                rate_limit "concord.riot.22" key [origin] using accounts;
            }

            GET active_region(game: RiotGame, puuid: String) -> AccountRegionDto {
                path "region/by-game/{game}/by-puuid/{puuid}";
                rate_limit "concord.riot.23" key [origin] using standard;
            }
        }

        scope match_v5() {
            path "lol/match/v5/matches";

            // ANCHOR: riot_match_ids_endpoint
            GET match_ids_by_puuid(
                puuid: String,
                query("startTime") start_time?: i64,
                query("endTime") end_time?: i64,
                query queue?: u32,
                query("type") match_type?: MatchType,
                query start: u64 = 0,
                query count: NonZeroU32 = DEFAULT_MATCH_PAGE_SIZE,
            ) -> Vec<String> {
                path "by-puuid/{puuid}/ids";
                rate_limit "concord.riot.24" key [origin] using match_reads;
                paginate via concord::pagination::OffsetPagination {
                    init concord::pagination::OffsetInit { offset: start, page_size: count };
                    cursor -> start;
                }
            }
            // ANCHOR_END: riot_match_ids_endpoint

            GET replays_by_puuid(puuid: String) -> ReplayDto {
                path "by-puuid/{puuid}/replays";
                rate_limit "concord.riot.25" key [origin] using standard;
            }

            GET match_by_id(match_id: String) -> MatchDto {
                path "{match_id}";
                rate_limit "concord.riot.26" key [origin] using match_reads;
            }

            GET timeline_by_id(match_id: String) -> TimelineDto {
                path "{match_id}/timeline";
                rate_limit "concord.riot.27" key [origin] using match_reads;
            }
        }

        scope tournament_stub_v5() {
            path "lol/tournament-stub/v5";
            // A deliberately repeated default: its provenance is this scope.
            request Json;
            // Scope-shared method bucket (one of the three original groups).
            rate_limit "concord.riot.28" key [origin] using standard;

            POST create_stub_codes(
                query("tournamentId") tournament_id: i64,
                query count?: u32,
                body body: TournamentCodeParameters,
            ) -> Vec<String> {
                path "codes";
            }

            GET stub_code(tournament_code: String) -> TournamentCodeDto {
                path "codes/{tournament_code}";
            }

            GET stub_lobby_events(tournament_code: String) -> LobbyEventListDto {
                path "lobby-events/by-code/{tournament_code}";
            }

            POST register_stub_provider(body body: ProviderRegistrationParameters) -> i32 {
                path "providers";
            }

            POST register_stub_tournament(body body: TournamentRegistrationParameters) -> i32 {
                path "tournaments";
            }
        }

        scope tournament_v5() {
            path "lol/tournament/v5";
            // Representation fences: nothing is inherited below this scope.
            request explicit;
            response explicit;

            POST create_tournament_codes(
                query("tournamentId") tournament_id: i64,
                query count?: u32,
                body body: TournamentCodeParameters via Json,
            ) -> Vec<String> via Json {
                path "codes";
            }

            GET tournament_code(tournament_code: String) -> TournamentCodeDto via Json {
                path "codes/{tournament_code}";
            }

            PUT update_tournament_code(
                tournament_code: String,
                body body: TournamentCodeUpdateParameters via Json,
            ) -> () via Empty {
                path "codes/{tournament_code}";
            }

            GET tournament_game_by_code(tournament_code: String) -> TournamentGameDto via Json {
                path "games/by-code/{tournament_code}";
            }

            GET tournament_lobby_events(tournament_code: String) -> LobbyEventListDto via Json {
                path "lobby-events/by-code/{tournament_code}";
            }

            POST register_tournament_provider(
                body body: ProviderRegistrationParameters via Json,
            ) -> i32 via Json {
                path "providers";
            }

            POST register_tournament(
                body body: TournamentRegistrationParameters via Json,
            ) -> i32 via Json {
                path "tournaments";
            }
        }
    }
}}
