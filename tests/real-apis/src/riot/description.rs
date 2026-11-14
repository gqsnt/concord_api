use concord::{by_offset_limit, by_page, json, Json, Secret};
use std::collections::HashMap;

concord::api! {
    /// League of Legends, Teamfight Tactics and Riot accounts.
    ///
    /// A route is a host. A platform (`euw1`, `kr`) holds what lives on one server; a region
    /// (`europe`, `asia`) holds accounts and matches. `Platform::region()` gives the one of the other.
    /// Retry is off until set: `.with_retry(retry())` waits out a `429`, also on a `POST`.
    pub client Riot(
        /// The key of the developer portal. A development key lasts a day.
        key: Secret,
    ) {
        headers { "x-riot-token" = key; }
        read Json;
        problem RiotProblem;
        /// Riot's limits, per host and per method, shared by every call of this client.
        /// It waits before a request that the last answers say is over a limit, and
        /// holds the host or the method for `Retry-After` after a `429`.
        middleware Quota::new();

        /// A platform and its host: `euw1` in `https://euw1.api.riotgames.com`.
        part platform(route: Platform) {
            base "https://{route}.api.riotgames.com";
        }
        /// A region and its host: `europe` in `https://europe.api.riotgames.com`.
        part region(route: Region) {
            base "https://{route}.api.riotgames.com";
        }
        /// The signed-in player: the token of Riot Sign On, in place of the API key.
        part player(access_token: Secret) {
            headers { remove "x-riot-token"; "authorization" = "Bearer {access_token}"; }
        }
        /// A match history, newest first: `.start(…)`, `.count(…)` (1 to 100; Riot gives 20),
        /// `.start_time(…)`, `.end_time(…)` in Unix seconds. It ends on a page shorter than `count`.
        part history(start?: u32, count?: u32, start_time?: i64, end_time?: i64) {
            query { start; count; "startTime" = start_time; "endTime" = end_time; }
            paginate by_offset_limit(start, count);
        }
        /// The filters of a LoL history: `.queue(420)` (ranked solo), `.kind(MatchType::Ranked)`.
        part lol_filter(queue?: i32, kind?: MatchType) {
            query { queue; "type" = kind; }
        }
        /// A ladder by page number, from 1: `.page(…)`. It ends on an empty page.
        part ladder(page?: u32) {
            query { page; }
            paginate by_page(page, 1);
        }
        /// A ranked queue of TFT: `.queue(…)`. Riot reads `RANKED_TFT` when it is not set.
        part tft_queue(queue?: TftQueue) {
            query { queue; }
        }

        // ---- groups of one endpoint, on a platform's host

        /// champion-v3: the free champions of the week, and those for new players.
        GET champion_rotations(..platform) -> ChampionRotations {
            path "lol/platform/v3/champion-rotations";
        }
        /// lol-status-v4: the maintenances and incidents of a platform.
        GET lol_status(..platform) -> PlatformData {
            path "lol/status/v4/platform-data";
        }
        /// tft-status-v1.
        GET tft_status(..platform) -> PlatformData {
            path "tft/status/v1/platform-data";
        }
        /// third-party-code-v4: the code a player typed in the client, to prove an account is theirs.
        /// (unsure: still served)
        #[deprecated(note = "keyed by summoner id, which Riot replaced by the PUUID")]
        GET third_party_code(summoner_id: SummonerId, ..platform) -> Option<String> {
            path "lol/platform/v4/third-party-code/by-summoner/{summoner_id}";
            status { 404 => None }
        }

        // ---- regional hosts: americas, asia, europe, sea

        /// account-v1. An account can be found from any region; the nearest is the quickest.
        /// (unsure: whether `sea` serves it)
        scope account(..region) {
            path "riot/account/v1";

            /// `None` when there is no such account.
            GET by_puuid(puuid: Puuid) -> Option<Account> {
                path "accounts/by-puuid/{puuid}";
                status { 404 => None }
            }
            /// The account behind a Riot ID, `game_name#tag_line`. `None` when there is none.
            GET by_riot_id(game_name: String, tag_line: String) -> Option<Account> {
                path "accounts/by-riot-id/{game_name}/{tag_line}";
                status { 404 => None }
            }
            /// The signed-in player.
            GET me(..player) -> Account { path "accounts/me"; }
            /// The shard of a player, for a game that has shards (`val`, `lor`).
            GET active_shard(game: Game, puuid: Puuid) -> Option<ActiveShard> {
                path "active-shards/by-game/{game}/by-puuid/{puuid}";
                status { 404 => None }
            }
            /// The platform a player of `lol` or `tft` plays on. (unsure: path and answer)
            GET active_region(game: Game, puuid: Puuid) -> Option<ActiveRegion> {
                path "region/by-game/{game}/by-puuid/{puuid}";
                status { 404 => None }
            }
        }

        /// match-v5. A match lives in the region of its platform: `EUW1_…` in `europe`.
        scope matches(..region) {
            path "lol/match/v5/matches";

            /// The ids of a player's matches, newest first.
            GET ids(puuid: Puuid, ..history, ..lol_filter) -> Vec<MatchId> {
                path "by-puuid/{puuid}/ids";
            }
            /// `None` when there is no such match here: past Riot's retention, or in another region.
            GET get(id: MatchId) -> Option<Match> {
                path "{id}";
                status { 404 => None }
            }
            /// The match minute by minute: frames and events.
            GET timeline(id: MatchId) -> Option<Timeline> {
                path "{id}/timeline";
                status { 404 => None }
            }
            /// The replays of a player's recent matches. (unsure: the whole endpoint)
            GET replays(puuid: Puuid) -> Replays { path "by-puuid/{puuid}/replays"; }
        }

        /// tft-match-v1. Its ids are not LoL's.
        scope tft_matches(..region) {
            path "tft/match/v1/matches";

            GET ids(puuid: Puuid, ..history) -> Vec<TftMatchId> { path "by-puuid/{puuid}/ids"; }
            GET get(id: TftMatchId) -> Option<TftMatch> {
                path "{id}";
                status { 404 => None }
            }
        }

        /// lol-rso-match-v1: the matches of the signed-in player, custom games too.
        /// (unsure: the paths, and which filters it takes)
        scope rso_matches(..region, ..player) {
            path "lol/rso-match/v1/matches";

            GET ids(..history, ..lol_filter) -> Vec<MatchId> { path "ids"; }
            GET get(id: MatchId) -> Option<Match> {
                path "{id}";
                status { 404 => None }
            }
            GET timeline(id: MatchId) -> Option<Timeline> {
                path "{id}/timeline";
                status { 404 => None }
            }
        }

        /// tournament-v5, or its stub: the same calls with no real game behind them, to build
        /// and test against. Needs a key that Riot enabled for tournaments.
        /// (unsure: served on `americas` only)
        scope tournament(api: TournamentApi) {
            base "https://americas.api.riotgames.com";
            path "lol/{api}/v5";

            /// Where Riot posts the result of each game: once for a region and a callback URL.
            POST register_provider(provider: NewProvider) -> ProviderId {
                path "providers";
                body json(provider);
            }
            POST create(registration: NewTournament) -> TournamentId {
                path "tournaments";
                body json(registration);
            }
            /// `count` codes, 1 to 1000. (unsure: Riot makes one when it is not set)
            POST create_codes(tournament_id: TournamentId, settings: CodeSettings, count?: u32)
                -> Vec<TournamentCode>
            {
                path "codes";
                query { "tournamentId" = tournament_id; count; }
                body json(settings);
            }
            GET code(code: TournamentCode) -> Option<CodeDetails> {
                path "codes/{code}";
                status { 404 => None }
            }
            GET lobby_events(code: TournamentCode) -> LobbyEvents { path "lobby-events/by-code/{code}"; }
            /// Not on the stub.
            PUT update_code(code: TournamentCode, update: CodeUpdate) {
                path "codes/{code}";
                body json(update);
            }
            /// The games played with a code. Not on the stub.
            GET games(code: TournamentCode) -> Vec<TournamentGame> { path "games/by-code/{code}"; }
        }

        // ---- platform hosts: br1, eun1, euw1, jp1, kr, la1, la2, me1, na1, oc1, ru, sg2, tr1, tw2, vn2

        /// champion-mastery-v4.
        scope champion_mastery(..platform) {
            path "lol/champion-mastery/v4";

            /// Every champion the player has played.
            GET by_puuid(puuid: Puuid) -> Vec<ChampionMastery> {
                path "champion-masteries/by-puuid/{puuid}";
            }
            /// `None` when the player has never played this champion.
            GET by_champion(puuid: Puuid, champion: ChampionId) -> Option<ChampionMastery> {
                path "champion-masteries/by-puuid/{puuid}/by-champion/{champion}";
                status { 404 => None }
            }
            /// The best champions. Riot gives 3 when `count` is not set.
            GET top(puuid: Puuid, count?: u32) -> Vec<ChampionMastery> {
                path "champion-masteries/by-puuid/{puuid}/top";
                query { count; }
            }
            /// The sum of the mastery levels of every champion.
            GET score(puuid: Puuid) -> i32 { path "scores/by-puuid/{puuid}"; }

        }

        /// clash-v1.
        scope clash(..platform) {
            path "lol/clash/v1";

            /// The player's places in Clash teams; empty when there is none.
            GET players(puuid: Puuid) -> Vec<ClashPlayer> { path "players/by-puuid/{puuid}"; }
            GET team(id: ClashTeamId) -> Option<ClashTeam> {
                path "teams/{id}";
                status { 404 => None }
            }
            /// The tournaments that are open or coming.
            GET tournaments() -> Vec<ClashTournament> { path "tournaments"; }
            GET tournament_by_team(team: ClashTeamId) -> Option<ClashTournament> {
                path "tournaments/by-team/{team}";
                status { 404 => None }
            }
            GET tournament(id: ClashTournamentId) -> Option<ClashTournament> {
                path "tournaments/{id}";
                status { 404 => None }
            }
        }

        /// league-v4: LoL's ranked ladders.
        scope league(..platform) {
            path "lol/league/v4";

            GET challenger(queue: Queue) -> LeagueList { path "challengerleagues/by-queue/{queue}"; }
            GET grandmaster(queue: Queue) -> LeagueList { path "grandmasterleagues/by-queue/{queue}"; }
            GET master(queue: Queue) -> LeagueList { path "masterleagues/by-queue/{queue}"; }
            GET get(id: LeagueId) -> Option<LeagueList> {
                path "leagues/{id}";
                status { 404 => None }
            }
            /// One entry for each queue the player is ranked in; empty when unranked.
            GET by_puuid(puuid: Puuid) -> Vec<LeagueEntry> { path "entries/by-puuid/{puuid}"; }
            /// A tier and a division below master. (unsure: 205 players a page)
            GET entries(queue: Queue, tier: Tier, division: Division, ..ladder) -> Vec<LeagueEntry> {
                path "entries/{queue}/{tier}/{division}";
            }
        }

        /// league-exp-v4: the same ladder, with master and above.
        scope league_exp(..platform) {
            path "lol/league-exp/v4";

            GET entries(queue: Queue, tier: Tier, division: Division, ..ladder) -> Vec<LeagueEntry> {
                path "entries/{queue}/{tier}/{division}";
            }
        }

        /// lol-challenges-v1.
        scope challenges(..platform) {
            path "lol/challenges/v1";

            GET config() -> Vec<ChallengeConfig> { path "challenges/config"; }
            /// For each challenge, the share of players at each level.
            GET percentiles() -> HashMap<ChallengeId, Percentiles> { path "challenges/percentiles"; }
            GET challenge_config(id: ChallengeId) -> Option<ChallengeConfig> {
                path "challenges/{id}/config";
                status { 404 => None }
            }
            GET challenge_percentiles(id: ChallengeId) -> Percentiles {
                path "challenges/{id}/percentiles";
            }
            /// The best players at an apex level. Not paged: `limit` is all there is.
            GET leaderboard(id: ChallengeId, level: ApexLevel, limit?: u32) -> Vec<ApexPlayer> {
                path "challenges/{id}/leaderboards/by-level/{level}";
                query { limit; }
            }
            GET player(puuid: Puuid) -> Option<PlayerChallenges> {
                path "player-data/{puuid}";
                status { 404 => None }
            }
        }

        /// spectator-v5. The path says `by-summoner`; it takes a PUUID.
        scope spectator(..platform) {
            path "lol/spectator/v5";

            /// `None` when the player is not in a game now.
            GET active_game(puuid: Puuid) -> Option<CurrentGame> {
                path "active-games/by-summoner/{puuid}";
                status { 404 => None }
            }
            GET featured_games() -> FeaturedGames { path "featured-games"; }
        }

        /// spectator-tft-v5: TFT, under a path that starts with `lol`.
        scope spectator_tft(..platform) {
            path "lol/spectator/tft/v5";

            /// `None` when the player is not in a game now.
            GET active_game(puuid: Puuid) -> Option<CurrentGame> {
                path "active-games/by-puuid/{puuid}";
                status { 404 => None }
            }
            /// (unsure: the whole endpoint)
            GET featured_games() -> FeaturedGames { path "featured-games"; }
        }

        /// summoner-v4: a player's LoL profile on one platform.
        scope summoner(..platform) {
            path "lol/summoner/v4/summoners";

            /// `None` when this account has never played LoL on this platform.
            GET by_puuid(puuid: Puuid) -> Option<Summoner> {
                path "by-puuid/{puuid}";
                status { 404 => None }
            }
            GET me(..player) -> Summoner { path "me"; }
        }

        /// tft-league-v1.
        scope tft_league(..platform) {
            path "tft/league/v1";

            GET challenger(..tft_queue) -> TftLeagueList { path "challenger"; }
            GET grandmaster(..tft_queue) -> TftLeagueList { path "grandmaster"; }
            GET master(..tft_queue) -> TftLeagueList { path "master"; }
            GET get(id: LeagueId) -> Option<TftLeagueList> {
                path "leagues/{id}";
                status { 404 => None }
            }
            /// (unsure: the path)
            GET by_puuid(puuid: Puuid) -> Vec<TftLeagueEntry> { path "by-puuid/{puuid}"; }
            GET entries(tier: Tier, division: Division, ..tft_queue, ..ladder) -> Vec<TftLeagueEntry> {
                path "entries/{tier}/{division}";
            }
            /// The top of a rated ladder (Hyper Roll). Not paged.
            GET top_rated(queue: RatedQueue) -> Vec<TopRated> { path "rated-ladders/{queue}/top"; }
        }

        /// tft-summoner-v1: the same profile, for TFT.
        scope tft_summoner(..platform) {
            path "tft/summoner/v1/summoners";

            GET by_puuid(puuid: Puuid) -> Option<Summoner> {
                path "by-puuid/{puuid}";
                status { 404 => None }
            }
            GET me(..player) -> Summoner { path "me"; }
        }
    }
}
