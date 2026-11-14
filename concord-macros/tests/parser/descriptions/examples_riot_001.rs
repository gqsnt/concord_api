concord::api! {
    /// League of Legends, Riot accounts, and Teamfight Tactics.
    /// Retry is opt-in; use crate::retry(). For shared 429 pauses use crate::with_cooldown().
    pub client Riot(key: Secret) {
        headers { "x-riot-token" = key; }
        read Json;
        problem RiotProblem;

        /// Regional services. Choose the player's match cluster explicitly.
        /// For account-v1 use Americas, Asia or Europe; support for Sea is (unsure).
        scope regional(route: Region) {
            base "https://{route}.api.riotgames.com";

            /// account-v1. A regional route is not the player's platform shard.
            scope account {
                path "riot/account/v1";
                /// None means no account; it does not establish whether a route was correct.
                GET by_puuid(puuid: Puuid) -> Option<Account> {
                    path "accounts/by-puuid/{puuid}";
                    status { 404 => None }
                }
                GET by_riot_id(game_name: GameName, tag_line: TagLine) -> Option<Account> {
                    path "accounts/by-riot-id/{game_name}/{tag_line}";
                    status { 404 => None }
                }
                /// RSO token obtained by the application; no API key is sent here.
                GET me(access_token: Secret) -> Account {
                    path "accounts/me";
                    headers { remove "x-riot-token"; "authorization" = "Bearer {access_token}"; }
                }
                /// Primarily other Riot games; usable game values for LoL/TFT are (unsure).
                GET active_shard(game: Game, puuid: Puuid) -> Option<ActiveShard> {
                    path "active-shards/by-game/{game}/by-puuid/{puuid}";
                    status { 404 => None }
                }
                /// game is lol or tft. Returned region-to-Platform mapping is (unsure).
                GET active_region(game: Game, puuid: Puuid) -> Option<AccountRegion> {
                    path "region/by-game/{game}/by-puuid/{puuid}";
                    status { 404 => None }
                }
            }

            /// match-v5, including the event timeline.
            scope matches {
                path "lol/match/v5/matches";
                /// Newest first. Times are Unix seconds; count is 0..=100 (unsure: zero).
                /// Omitted start/count use server defaults. Paging is not a stable snapshot.
                GET ids(puuid: Puuid, start?: u32, count?: u32,
                    queue?: i32, match_type?: MatchType,
                    start_time?: i64, end_time?: i64) -> Vec<MatchId> {
                    path "by-puuid/{puuid}/ids";
                    query { start; count; queue; "type" = match_type;
                        "startTime" = start_time; "endTime" = end_time; }
                    paginate offset_limit(start, count);
                }
                /// None for an unavailable match, including data outside retention.
                GET get(id: MatchId) -> Option<Match> {
                    path "{id}";
                    status { 200 { body: game } => Some(game), 404 => None }
                }
                /// A complete JSON timeline; the 64 MiB cap is this crate's choice.
                GET timeline(id: MatchId) -> Option<MatchTimeline> {
                    path "{id}/timeline";
                    read Json.limit(64 * concord::MIB);
                    status { 404 => None }
                }
                /// (unsure) Newer replay listing: response schema, filters and availability.
                GET replays(puuid: Puuid) -> PlayerReplays {
                    path "by-puuid/{puuid}/replays";
                }
            }

            /// tft-match-v1. TFT match IDs are distinct from LoL match IDs.
            scope tft_matches {
                path "tft/match/v1/matches";
                /// count max 100; start/end time support is (unsure). Times: Unix seconds.
                GET ids(puuid: Puuid, start?: u32, count?: u32,
                    start_time?: i64, end_time?: i64) -> Vec<TftMatchId> {
                    path "by-puuid/{puuid}/ids";
                    query { start; count; "startTime" = start_time; "endTime" = end_time; }
                    paginate offset_limit(start, count);
                }
                GET get(id: TftMatchId) -> Option<TftMatch> {
                    path "{id}";
                    status { 404 => None }
                }
            }

            /// lol-rso-match-v1: (unsure) exact operations, paths and filters.
            /// Token acquisition/refresh belongs to the application, not this scope.
            scope rso_matches(access_token: Secret) {
                path "lol/rso-match/v1/matches";
                headers { remove "x-riot-token"; "authorization" = "Bearer {access_token}"; }
                GET ids(puuid: Puuid, start?: u32, count?: u32) -> Vec<MatchId> {
                    path "by-puuid/{puuid}/ids";
                    query { start; count; }
                    // No automatic pager: the termination contract is (unsure).
                }
                GET get(id: MatchId) -> Option<Match> {
                    path "{id}";
                    status { 404 => None }
                }
            }

            /// tournament-v5. Requires a tournament-enabled key; routing is (unsure).
            scope tournaments {
                path "lol/tournament/v5";
                /// Creates codes, not a paged read. count is at most 1000 (unsure).
                POST create_codes(tournament_id: TournamentId, parameters: CodeParameters,
                    count?: u32) -> Vec<TournamentCode> {
                    path "codes";
                    query { "tournamentId" = tournament_id; count; }
                    body concord::json(parameters);
                }
                GET code(code: TournamentCode) -> Option<CodeDetails> {
                    path "codes/{code}";
                    status { 404 => None }
                }
                PUT update_code(code: TournamentCode, parameters: CodeUpdate) {
                    path "codes/{code}";
                    body concord::json(parameters);
                }
                GET lobby_events(code: TournamentCode) -> LobbyEvents {
                    path "lobby-events/by-code/{code}";
                }
                /// (unsure) v5 game result collection and response schema.
                GET games(code: TournamentCode) -> Vec<TournamentGame> {
                    path "games/by-code/{code}";
                }
                POST register_provider(parameters: ProviderRegistration) -> ProviderId {
                    path "providers";
                    body concord::json(parameters);
                }
                POST create(parameters: TournamentRegistration) -> TournamentId {
                    path "tournaments";
                    body concord::json(parameters);
                }
            }

            /// tournament-stub-v5. Stub results do not establish that a real lobby exists.
            scope tournament_stub {
                path "lol/tournament-stub/v5";
                POST create_codes(tournament_id: TournamentId, parameters: CodeParameters,
                    count?: u32) -> Vec<TournamentCode> {
                    path "codes";
                    query { "tournamentId" = tournament_id; count; }
                    body concord::json(parameters);
                }
                GET code(code: TournamentCode) -> Option<CodeDetails> {
                    path "codes/{code}";
                    status { 404 => None }
                }
                GET lobby_events(code: TournamentCode) -> LobbyEvents {
                    path "lobby-events/by-code/{code}";
                }
                POST register_provider(parameters: ProviderRegistration) -> ProviderId {
                    path "providers";
                    body concord::json(parameters);
                }
                POST create(parameters: TournamentRegistration) -> TournamentId {
                    path "tournaments";
                    body concord::json(parameters);
                }
            }
        }

        /// Platform services. Euw1 is a platform; Europe is a regional cluster.
        scope platform(route: Platform) {
            base "https://{route}.api.riotgames.com";

            /// champion-v3: the current free champion rotation, including new-player rules.
            GET champion_rotations() -> ChampionRotations { path "lol/platform/v3/champion-rotations"; }
            /// lol-status-v4: maintenance and incidents for this platform.
            GET status() -> PlatformStatus { path "lol/status/v4/platform-data"; }
            /// tft-status-v1.
            GET tft_status() -> PlatformStatus { path "tft/status/v1/platform-data"; }
            /// third-party-code-v4: legacy verification code; availability is (unsure).
            #[deprecated(note = "Legacy summoner-ID verification; availability (unsure)")]
            GET third_party_code(summoner_id: SummonerId) -> Option<String> {
                path "lol/platform/v4/third-party-code/by-summoner/{summoner_id}";
                status { 404 => None }
            }

            /// champion-mastery-v4. Top lists have a count, not a next page.
            scope mastery {
                path "lol/champion-mastery/v4";
                GET by_puuid(puuid: Puuid) -> Vec<ChampionMastery> {
                    path "champion-masteries/by-puuid/{puuid}";
                }
                GET by_champion(puuid: Puuid, champion_id: ChampionId) -> Option<ChampionMastery> {
                    path "champion-masteries/by-puuid/{puuid}/by-champion/{champion_id}";
                    status { 404 => None }
                }
                GET top(puuid: Puuid, count?: u32) -> Vec<ChampionMastery> {
                    path "champion-masteries/by-puuid/{puuid}/top";
                    query { count; }
                }
                GET score(puuid: Puuid) -> i32 { path "scores/by-puuid/{puuid}"; }
                /// Legacy summoner-ID variants below: continued availability is (unsure).
                #[deprecated(note = "Use by_puuid; legacy availability (unsure)")]
                GET by_summoner(summoner_id: SummonerId) -> Vec<ChampionMastery> {
                    path "champion-masteries/by-summoner/{summoner_id}";
                }
                #[deprecated(note = "Use by_champion; legacy availability (unsure)")]
                GET champion_by_summoner(summoner_id: SummonerId, champion_id: ChampionId) -> Option<ChampionMastery> {
                    path "champion-masteries/by-summoner/{summoner_id}/by-champion/{champion_id}";
                    status { 404 => None }
                }
                #[deprecated(note = "Use top; legacy availability (unsure)")]
                GET top_by_summoner(summoner_id: SummonerId, count?: u32) -> Vec<ChampionMastery> {
                    path "champion-masteries/by-summoner/{summoner_id}/top";
                    query { count; }
                }
                #[deprecated(note = "Use score; legacy availability (unsure)")]
                GET score_by_summoner(summoner_id: SummonerId) -> i32 { path "scores/by-summoner/{summoner_id}"; }
            }

            /// clash-v1.
            scope clash {
                path "lol/clash/v1";
                /// (unsure) PUUID successor to players/by-summoner.
                GET players(puuid: Puuid) -> Vec<ClashPlayer> { path "players/by-puuid/{puuid}"; }
                /// Legacy route; current availability is (unsure).
                GET players_by_summoner(summoner_id: SummonerId) -> Vec<ClashPlayer> {
                    path "players/by-summoner/{summoner_id}";
                }
                GET team(team_id: ClashTeamId) -> Option<ClashTeam> {
                    path "teams/{team_id}";
                    status { 404 => None }
                }
                GET tournaments() -> Vec<ClashTournament> { path "tournaments"; }
                GET tournament_by_team(team_id: ClashTeamId) -> Option<ClashTournament> {
                    path "tournaments/by-team/{team_id}";
                    status { 404 => None }
                }
                GET tournament(tournament_id: ClashTournamentId) -> Option<ClashTournament> {
                    path "tournaments/{tournament_id}";
                    status { 404 => None }
                }
            }

            /// league-v4. Queue/tier/division use Riot's uppercase wire names.
            scope league {
                path "lol/league/v4";
                GET challenger(queue: Queue) -> LeagueList { path "challengerleagues/by-queue/{queue}"; }
                GET grandmaster(queue: Queue) -> LeagueList { path "grandmasterleagues/by-queue/{queue}"; }
                GET master(queue: Queue) -> LeagueList { path "masterleagues/by-queue/{queue}"; }
                GET get(league_id: LeagueId) -> Option<LeagueList> {
                    path "leagues/{league_id}";
                    status { 404 => None }
                }
                /// Empty when unranked; a 404 remains an error rather than an empty ranking.
                GET by_puuid(puuid: Puuid) -> Vec<LeagueEntry> { path "entries/by-puuid/{puuid}"; }
                /// Legacy summoner-ID route; availability is (unsure).
                GET by_summoner(summoner_id: SummonerId) -> Vec<LeagueEntry> { path "entries/by-summoner/{summoner_id}"; }
                /// Page starts at 1. Empty page ends iteration; a short page does not.
                GET entries(queue: Queue, tier: Tier, division: Division, page?: u32) -> Vec<LeagueEntry> {
                    path "entries/{queue}/{tier}/{division}";
                    query { page; }
                    paginate crate::league_pages!();
                }
            }
            /// league-exp-v4: also covers apex tiers (accepted division there is (unsure)).
            scope league_exp {
                path "lol/league-exp/v4";
                GET entries(queue: Queue, tier: Tier, division: Division, page?: u32) -> Vec<LeagueEntry> {
                    path "entries/{queue}/{tier}/{division}";
                    query { page; }
                    paginate crate::league_pages!();
                }
            }

            /// lol-challenges-v1. Percentiles are keyed JSON objects, not pages.
            scope challenges {
                path "lol/challenges/v1";
                GET configurations() -> Vec<ChallengeConfig> { path "challenges/config"; }
                GET percentiles() -> ChallengePercentiles { path "challenges/percentiles"; }
                GET configuration(challenge_id: ChallengeId) -> Option<ChallengeConfig> {
                    path "challenges/{challenge_id}/config";
                    status { 404 => None }
                }
                /// Top players at level; limit has no offset/cursor companion.
                GET leaders(challenge_id: ChallengeId, level: ChallengeLevel, limit?: u32) -> Vec<ChallengeLeader> {
                    path "challenges/{challenge_id}/leaderboards/by-level/{level}";
                    query { limit; }
                }
                GET challenge_percentiles(challenge_id: ChallengeId) -> LevelPercentiles {
                    path "challenges/{challenge_id}/percentiles";
                }
                GET player(puuid: Puuid) -> Option<PlayerChallenges> {
                    path "player-data/{puuid}";
                    status { 404 => None }
                }
            }

            /// spectator-v5. The path says by-summoner, but the input is a PUUID.
            scope spectator {
                path "lol/spectator/v5";
                /// None means no currently observable game, not a nonexistent account.
                GET current_game(puuid: Puuid) -> Option<CurrentGame> {
                    path "active-games/by-summoner/{puuid}";
                    status { 404 => None }
                }
                /// (unsure) Continued availability in v5.
                GET featured_games() -> FeaturedGames { path "featured-games"; }
            }
            /// spectator-tft-v5 is a TFT family despite its /lol URL prefix.
            scope tft_spectator {
                path "lol/spectator/tft/v5";
                GET current_game(puuid: Puuid) -> Option<TftCurrentGame> {
                    path "active-games/by-puuid/{puuid}";
                    status { 404 => None }
                }
            }

            /// summoner-v4. Use Riot account lookup for player-facing names.
            scope summoner {
                path "lol/summoner/v4/summoners";
                GET by_puuid(puuid: Puuid) -> Option<Summoner> {
                    path "by-puuid/{puuid}";
                    status { 404 => None }
                }
                GET me(access_token: Secret) -> Summoner {
                    path "me";
                    headers { remove "x-riot-token"; "authorization" = "Bearer {access_token}"; }
                }
                /// Legacy identifiers below; current availability is (unsure).
                #[deprecated(note = "Use by_puuid; legacy availability (unsure)")]
                GET by_id(summoner_id: SummonerId) -> Option<Summoner> {
                    path "{summoner_id}";
                    status { 404 => None }
                }
                #[deprecated(note = "Use by_puuid; legacy availability (unsure)")]
                GET by_account(account_id: EncryptedAccountId) -> Option<Summoner> {
                    path "by-account/{account_id}";
                    status { 404 => None }
                }
                #[deprecated(note = "Use account.by_riot_id; availability (unsure)")]
                GET by_name(name: String) -> Option<Summoner> {
                    path "by-name/{name}";
                    status { 404 => None }
                }
            }

            /// tft-league-v1. TFT queue names differ from LoL queue names.
            scope tft_league {
                path "tft/league/v1";
                GET challenger(queue?: TftQueue) -> TftLeagueList {
                    path "challenger"; query { queue; }
                }
                GET grandmaster(queue?: TftQueue) -> TftLeagueList {
                    path "grandmaster"; query { queue; }
                }
                GET master(queue?: TftQueue) -> TftLeagueList {
                    path "master"; query { queue; }
                }
                /// (unsure) Continued availability of lookup by league ID.
                GET get(league_id: LeagueId) -> Option<TftLeagueList> {
                    path "leagues/{league_id}";
                    status { 404 => None }
                }
                GET by_puuid(puuid: Puuid) -> Vec<TftLeagueEntry> { path "by-puuid/{puuid}"; }
                /// Legacy summoner-ID route; availability is (unsure).
                GET by_summoner(summoner_id: SummonerId) -> Vec<TftLeagueEntry> { path "entries/by-summoner/{summoner_id}"; }
                GET entries(tier: Tier, division: Division, queue?: TftQueue, page?: u32) -> Vec<TftLeagueEntry> {
                    path "entries/{tier}/{division}";
                    query { queue; page; }
                    paginate crate::league_pages!();
                }
                /// e.g. RANKED_TFT_TURBO; this top ladder is not paged.
                GET top_rated(queue: TftQueue) -> Vec<TftTopRated> { path "rated-ladders/{queue}/top"; }
            }

            /// tft-summoner-v1. me uses RSO; the other calls use the API key.
            scope tft_summoner {
                path "tft/summoner/v1/summoners";
                GET by_puuid(puuid: Puuid) -> Option<TftSummoner> {
                    path "by-puuid/{puuid}";
                    status { 404 => None }
                }
                GET me(access_token: Secret) -> TftSummoner {
                    path "me";
                    headers { remove "x-riot-token"; "authorization" = "Bearer {access_token}"; }
                }
                /// Legacy identifiers below; current availability is (unsure).
                #[deprecated(note = "Use by_puuid; legacy availability (unsure)")]
                GET by_id(summoner_id: SummonerId) -> Option<TftSummoner> {
                    path "{summoner_id}";
                    status { 404 => None }
                }
                #[deprecated(note = "Use by_puuid; legacy availability (unsure)")]
                GET by_account(account_id: EncryptedAccountId) -> Option<TftSummoner> {
                    path "by-account/{account_id}";
                    status { 404 => None }
                }
                #[deprecated(note = "Use account.by_riot_id; availability (unsure)")]
                GET by_name(name: String) -> Option<TftSummoner> {
                    path "by-name/{name}";
                    status { 404 => None }
                }
            }
        }
    }
}
