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
        rate_limit { id "concord.riot.1"; cost 1;
            key [origin];
            allow 500 per 10s;
            allow 30000 per 10m;
        }

        scope champion_mastery_v4() {
            path ["champion-mastery", "v4"];
            rate_limit { id "concord.riot.2"; key [origin]; cost 1;
                allow 20000 per 10s;
                allow 1200000 per 10m;
            }

            GET masteries_by_puuid(puuid: String) -> Vec<ChampionMasteryDto> via Json {
                path ["champion-masteries", "by-puuid", endpoint.puuid];
            }

            GET mastery_by_champion(puuid: String, champion_id: i64) -> ChampionMasteryDto via Json {
                path ["champion-masteries", "by-puuid", endpoint.puuid, "by-champion", endpoint.champion_id];
            }

            GET top_masteries_by_puuid(puuid: String, count?: u32) -> Vec<ChampionMasteryDto> via Json {
                path ["champion-masteries", "by-puuid", endpoint.puuid, "top"];
                query { "count" = endpoint.count; }
            }

            GET mastery_score_by_puuid(puuid: String) -> i32 via Json {
                path ["scores", "by-puuid", endpoint.puuid];
            }
        }

        scope champion_v3() {
            path ["platform", "v3"];

            GET champion_rotations() -> ChampionRotationsDto via Json {
                path ["champion-rotations"];
                rate_limit { id "concord.riot.3"; key [origin]; cost 1;
                    allow 20000 per 10s;
                    allow 1200000 per 10m;
                }
            }
        }

        scope clash_v1() {
            path ["clash", "v1"];

            GET clash_players_by_puuid(puuid: String) -> Vec<ClashPlayerDto> via Json {
                path ["players", "by-puuid", endpoint.puuid];
                rate_limit { id "concord.riot.4"; key [origin]; cost 1;
                    allow 20000 per 10s;
                    allow 1200000 per 10m;
                }
            }

            GET clash_team(team_id: String) -> ClashTeamDto via Json {
                path ["teams", endpoint.team_id];
                rate_limit { id "concord.riot.5"; key [origin]; cost 1;  allow 200 per 1m; }
            }

            GET clash_tournament_by_team(team_id: String) -> ClashTournamentDto via Json {
                path ["tournaments", "by-team", endpoint.team_id];
                rate_limit { id "concord.riot.6"; key [origin]; cost 1;  allow 200 per 1m; }
            }

            GET clash_tournament(tournament_id: i64) -> ClashTournamentDto via Json {
                path ["tournaments", endpoint.tournament_id];
                rate_limit { id "concord.riot.7"; key [origin]; cost 1;  allow 10 per 1m; }
            }

            GET clash_tournaments() -> Vec<ClashTournamentDto> via Json {
                path ["tournaments"];
                rate_limit { id "concord.riot.8"; key [origin]; cost 1;  allow 10 per 1m; }
            }
        }

        scope league_v4() {
            path ["league", "v4"];

            GET challenger_by_queue(queue: crate::riot::domain::LeagueQueue) -> LeagueListDto via Json {
                path ["challengerleagues", "by-queue", endpoint.queue];
                rate_limit { id "concord.riot.9"; key [origin]; cost 1;
                    allow 30 per 10s;
                    allow 500 per 10m;
                }
            }

            GET grandmaster_by_queue(queue: crate::riot::domain::LeagueQueue) -> LeagueListDto via Json {
                path ["grandmasterleagues", "by-queue", endpoint.queue];
                rate_limit { id "concord.riot.10"; key [origin]; cost 1;
                    allow 30 per 10s;
                    allow 500 per 10m;
                }
            }

            GET master_by_queue(queue: crate::riot::domain::LeagueQueue) -> LeagueListDto via Json {
                path ["masterleagues", "by-queue", endpoint.queue];
                rate_limit { id "concord.riot.11"; key [origin]; cost 1;
                    allow 30 per 10s;
                    allow 500 per 10m;
                }
            }

            GET league_entries(
                queue: crate::riot::domain::LeagueQueue,
                tier: crate::riot::domain::LeagueTier,
                division: crate::riot::domain::LeagueDivision,
                page: u32 = 1
            ) -> Vec<LeagueEntryDto> via Json {
                path ["entries", endpoint.queue, endpoint.tier, endpoint.division];
                query { "page" = endpoint.page; }
                rate_limit { id "concord.riot.12"; key [origin]; cost 1;  allow 50 per 10s; }
            }

            GET league_entries_by_puuid(puuid: String) -> Vec<LeagueEntryDto> via Json {
                path ["entries", "by-puuid", endpoint.puuid];
                rate_limit { id "concord.riot.13"; key [origin]; cost 1;
                    allow 20000 per 10s;
                    allow 1200000 per 10m;
                }
            }

            GET league_by_id(league_id: String) -> LeagueListDto via Json {
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
            ) -> Vec<LeagueEntryDto> via Json {
                path ["entries", endpoint.queue, endpoint.tier, endpoint.division];
                query { "page" = endpoint.page; }
                rate_limit { id "concord.riot.14"; key [origin]; cost 1;  allow 50 per 10s; }
            }
        }

        scope challenges_v1() {
            path ["challenges", "v1"];
            rate_limit { id "concord.riot.15"; key [origin]; cost 1;
                allow 20000 per 10s;
                allow 1200000 per 10m;
            }

            GET challenge_configs() -> Vec<ChallengeConfigInfoDto> via Json {
                path ["challenges", "config"];
            }

            GET challenge_percentiles() -> HashMap<String, HashMap<String, f64>> via Json {
                path ["challenges", "percentiles"];
            }

            GET challenge_config(challenge_id: i64) -> ChallengeConfigInfoDto via Json {
                path ["challenges", endpoint.challenge_id, "config"];
            }

            GET challenge_leaderboard(
                challenge_id: i64,
                level: crate::riot::domain::ChallengeLevel,
                limit?: u32
            ) -> Vec<ApexPlayerInfoDto> via Json {
                path ["challenges", endpoint.challenge_id, "leaderboards", "by-level", endpoint.level];
                query { "limit" = endpoint.limit; }
            }

            GET challenge_percentiles_by_id(challenge_id: i64) -> HashMap<String, f64> via Json {
                path ["challenges", endpoint.challenge_id, "percentiles"];
            }

            GET challenge_player_data(puuid: String) -> PlayerInfoDto via Json {
                path ["player-data", endpoint.puuid];
            }
        }

        scope status_v4() {
            path ["status", "v4"];

            GET platform_data() -> PlatformDataDto via Json {
                path ["platform-data"];
                rate_limit { id "concord.riot.16"; key [origin]; cost 1;
                    allow 20000 per 10s;
                    allow 1200000 per 10m;
                }
            }
        }

        scope spectator_v5() {
            path ["spectator", "v5"];

            GET active_game_by_puuid(puuid: String) -> CurrentGameInfo via Json {
                path ["active-games", "by-summoner", endpoint.puuid];
                rate_limit { id "concord.riot.17"; key [origin]; cost 1;
                    allow 3000 per 10s;
                    allow 180000 per 10m;
                }
            }
        }

        scope summoner_v4() {
            path ["summoner", "v4", "summoners"];

            GET summoner_by_puuid(puuid: String) -> SummonerDto via Json {
                path ["by-puuid", endpoint.puuid];
                rate_limit { id "concord.riot.18"; key [origin]; cost 1;  allow 1600 per 1m; }
            }

            GET summoner_by_id(summoner_id: String) -> SummonerDto via Json {
                path [endpoint.summoner_id];
                rate_limit { id "concord.riot.19"; key [origin]; cost 1;  allow 1600 per 1m; }
            }
        }
    }


    scope regional(regional: crate::riot::domain::RegionalRoute) {
        subdomain replace [regional.regional, "api"];
        rate_limit { id "concord.riot.20"; cost 1;
            key [origin];
            allow 500 per 10s;
            allow 30000 per 10m;
        }

        scope account_v1() {
            path ["riot", "account", "v1"];

            GET account_by_riot_id(game_name: String, tag_line: String) -> AccountDto via Json {
                path ["accounts", "by-riot-id", endpoint.game_name, endpoint.tag_line];
                rate_limit { id "concord.riot.21"; key [origin]; cost 1;  allow 1000 per 1m; }
            }

            GET account_by_puuid(puuid: String) -> AccountDto via Json {
                path ["accounts", "by-puuid", endpoint.puuid];
                rate_limit { id "concord.riot.22"; key [origin]; cost 1;  allow 1000 per 1m; }
            }

            GET active_region(game: crate::riot::domain::RiotGame, puuid: String) -> AccountRegionDto via Json {
                path ["region", "by-game", endpoint.game, "by-puuid", endpoint.puuid];
                rate_limit { id "concord.riot.23"; key [origin]; cost 1;
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
            ) -> Vec<String> via Json {
                path ["by-puuid", endpoint.puuid, "ids"];
                query {
                    "startTime" = endpoint.start_time;
                    "endTime" = endpoint.end_time;
                    "queue" = endpoint.queue;
                    "type" = endpoint.match_type;
                    "start" = endpoint.start;
                    "count" = endpoint.count;
                }
                rate_limit { id "concord.riot.24"; key [origin]; cost 1;  allow 2000 per 10s; }
                paginate via concord::pagination::OffsetPagination {
                    offset -> endpoint.start;
                    page_size -> endpoint.count;
                }
            }
            // ANCHOR_END: riot_match_ids_endpoint

            GET replays_by_puuid(puuid: String) -> ReplayDto via Json {
                path ["by-puuid", endpoint.puuid, "replays"];
                rate_limit { id "concord.riot.25"; key [origin]; cost 1;
                    allow 20000 per 10s;
                    allow 1200000 per 10m;
                }
            }

            GET match_by_id(match_id: String) -> MatchDto via Json {
                path [endpoint.match_id];
                rate_limit { id "concord.riot.26"; key [origin]; cost 1;  allow 2000 per 10s; }
            }

            GET timeline_by_id(match_id: String) -> TimelineDto via Json {
                path [endpoint.match_id, "timeline"];
                rate_limit { id "concord.riot.27"; key [origin]; cost 1;  allow 2000 per 10s; }
            }
        }

        scope tournament_stub_v5() {
            path ["lol", "tournament-stub", "v5"];
            rate_limit { id "concord.riot.28"; key [origin]; cost 1;
                allow 20000 per 10s;
                allow 1200000 per 10m;
            }

            POST create_stub_codes(
                tournament_id: i64,
                count?: u32,
                body body: TournamentCodeParameters via Json
            ) -> Vec<String> via Json {
                path ["codes"];
                query {
                    "tournamentId" = endpoint.tournament_id;
                    "count" = endpoint.count;
                }
            }

            GET stub_code(tournament_code: String) -> TournamentCodeDto via Json {
                path ["codes", endpoint.tournament_code];
            }

            GET stub_lobby_events(tournament_code: String) -> LobbyEventListDto via Json {
                path ["lobby-events", "by-code", endpoint.tournament_code];
            }

            POST register_stub_provider(body body: ProviderRegistrationParameters via Json) -> i32 via Json {
                path ["providers"];
            }

            POST register_stub_tournament(body body: TournamentRegistrationParameters via Json) -> i32 via Json {
                path ["tournaments"];
            }
        }

        scope tournament_v5() {
            path ["lol", "tournament", "v5"];

            POST create_tournament_codes(
                tournament_id: i64,
                count?: u32,
                body body: TournamentCodeParameters via Json
            ) -> Vec<String> via Json {
                path ["codes"];
                query {
                    "tournamentId" = endpoint.tournament_id;
                    "count" = endpoint.count;
                }
            }

            GET tournament_code(tournament_code: String) -> TournamentCodeDto via Json {
                path ["codes", endpoint.tournament_code];
            }

            PUT update_tournament_code(
                tournament_code: String,
                body body: TournamentCodeUpdateParameters via Json
            ) -> () via Empty {
                path ["codes", endpoint.tournament_code];
            }

            GET tournament_game_by_code(tournament_code: String) -> TournamentGameDto via Json {
                path ["games", "by-code", endpoint.tournament_code];
            }

            GET tournament_lobby_events(tournament_code: String) -> LobbyEventListDto via Json {
                path ["lobby-events", "by-code", endpoint.tournament_code];
            }

            POST register_tournament_provider(body body: ProviderRegistrationParameters via Json) -> i32 via Json {
                path ["providers"];
            }

            POST register_tournament(body body: TournamentRegistrationParameters via Json) -> i32 via Json {
                path ["tournaments"];
            }
        }
    }
}}
