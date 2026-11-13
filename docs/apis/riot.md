# the Riot Games API, in full in Concord

Written by Claude Opus 5.5 (`claude-opus-5-5`, as the harness names it), from the guide (the tour, chapters 3 to 6, the grammar, the rules, the tables of chapter 8), the endpoint list of [`first/riot.md`](first/riot.md) and remembered Riot documentation, on 2026-11-12. No API was called; nothing was compiled.

## The description

How the tree was chosen. Each of Riot's groups (`league-v4`, `match-v5`, …) is a scope with its own `route`, as in the tour: `riot.league(Euw1)`, `riot.matches(Europe)`. So each group's route has its own type (`Platform` or `Region`), and a group of one endpoint is an endpoint on the client with its own `base` (`riot.lol_status(Euw1)`). The tournament API and its stub are one scope: the stub is an input of the path. `(unsure)` marks a fact of Riot's that I cannot vouch for.

```rust
use std::collections::HashMap;

use concord::{by_offset_limit, by_page, json, Json, Secret};

use crate::model::*;            // the data types and the routes: beside the macro
use crate::quota::Quota;        // the middleware: beside the macro

concord::api! {
    /// League of Legends, Teamfight Tactics and Riot accounts.
    ///
    /// A route is a host. A platform (`euw1`, `kr`) holds what lives on one server; a region
    /// (`europe`, `asia`) holds accounts and matches. `Platform::region()` gives the one of the other.
    /// Retry is off: `.with_retry(Retry::standard())` waits out a `429` on every `GET`.
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

        // ---- groups of one endpoint: the route is the endpoint's own input

        /// champion-v3: the free champions of the week, and those for new players.
        GET champion_rotations(route: Platform) -> ChampionRotations {
            base "https://{route}.api.riotgames.com";
            path "lol/platform/v3/champion-rotations";
        }
        /// lol-status-v4: the maintenances and incidents of a platform.
        GET lol_status(route: Platform) -> PlatformData {
            base "https://{route}.api.riotgames.com";
            path "lol/status/v4/platform-data";
        }
        /// tft-status-v1.
        GET tft_status(route: Platform) -> PlatformData {
            base "https://{route}.api.riotgames.com";
            path "tft/status/v1/platform-data";
        }
        /// third-party-code-v4: the code a player typed in the client, to prove an account is theirs.
        #[deprecated(note = "keyed by summoner id (unsure: still served)")]
        GET third_party_code(route: Platform, summoner_id: SummonerId) -> Option<String> {
            base "https://{route}.api.riotgames.com";
            path "lol/platform/v4/third-party-code/by-summoner/{summoner_id}";
            status { 404 => None }
        }

        // ---- regional hosts: americas, asia, europe, sea

        /// account-v1. An account can be found from any region; the nearest is the quickest.
        /// (unsure: whether `sea` serves it)
        scope account(route: Region) {
            base "https://{route}.api.riotgames.com";
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
        scope matches(route: Region) {
            base "https://{route}.api.riotgames.com";
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
        scope tft_matches(route: Region) {
            base "https://{route}.api.riotgames.com";
            path "tft/match/v1/matches";

            GET ids(puuid: Puuid, ..history) -> Vec<TftMatchId> { path "by-puuid/{puuid}/ids"; }
            GET get(id: TftMatchId) -> Option<TftMatch> {
                path "{id}";
                status { 404 => None }
            }
        }

        /// lol-rso-match-v1: the matches of the signed-in player, custom games too.
        /// (unsure: the paths, and which filters it takes)
        scope rso_matches(route: Region, ..player) {
            base "https://{route}.api.riotgames.com";
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
        scope champion_mastery(route: Platform) {
            base "https://{route}.api.riotgames.com";
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

            // Keyed by summoner id, which Riot retired for the PUUID. (unsure: still served)
            #[deprecated(note = "use by_puuid")]
            GET by_summoner(summoner_id: SummonerId) -> Vec<ChampionMastery> {
                path "champion-masteries/by-summoner/{summoner_id}";
            }
            #[deprecated(note = "use by_champion")]
            GET by_summoner_champion(summoner_id: SummonerId, champion: ChampionId)
                -> Option<ChampionMastery>
            {
                path "champion-masteries/by-summoner/{summoner_id}/by-champion/{champion}";
                status { 404 => None }
            }
            #[deprecated(note = "use top")]
            GET top_by_summoner(summoner_id: SummonerId, count?: u32) -> Vec<ChampionMastery> {
                path "champion-masteries/by-summoner/{summoner_id}/top";
                query { count; }
            }
            #[deprecated(note = "use score")]
            GET score_by_summoner(summoner_id: SummonerId) -> i32 {
                path "scores/by-summoner/{summoner_id}";
            }
        }

        /// clash-v1.
        scope clash(route: Platform) {
            base "https://{route}.api.riotgames.com";
            path "lol/clash/v1";

            /// The player's places in Clash teams; empty when there is none.
            GET players(puuid: Puuid) -> Vec<ClashPlayer> { path "players/by-puuid/{puuid}"; }
            #[deprecated(note = "use players")]
            GET players_by_summoner(summoner_id: SummonerId) -> Vec<ClashPlayer> {
                path "players/by-summoner/{summoner_id}";
            }
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
        scope league(route: Platform) {
            base "https://{route}.api.riotgames.com";
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
            #[deprecated(note = "use by_puuid")]
            GET by_summoner(summoner_id: SummonerId) -> Vec<LeagueEntry> {
                path "entries/by-summoner/{summoner_id}";
            }
            /// A tier and a division below master. (unsure: 205 players a page)
            GET entries(queue: Queue, tier: Tier, division: Division, ..ladder) -> Vec<LeagueEntry> {
                path "entries/{queue}/{tier}/{division}";
            }
        }

        /// league-exp-v4: the same ladder, with master and above.
        scope league_exp(route: Platform) {
            base "https://{route}.api.riotgames.com";
            path "lol/league-exp/v4";

            GET entries(queue: Queue, tier: Tier, division: Division, ..ladder) -> Vec<LeagueEntry> {
                path "entries/{queue}/{tier}/{division}";
            }
        }

        /// lol-challenges-v1.
        scope challenges(route: Platform) {
            base "https://{route}.api.riotgames.com";
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
        scope spectator(route: Platform) {
            base "https://{route}.api.riotgames.com";
            path "lol/spectator/v5";

            /// `None` when the player is not in a game now.
            GET active_game(puuid: Puuid) -> Option<CurrentGame> {
                path "active-games/by-summoner/{puuid}";
                status { 404 => None }
            }
            GET featured_games() -> FeaturedGames { path "featured-games"; }
        }

        /// spectator-tft-v5: TFT, under a path that starts with `lol`.
        scope spectator_tft(route: Platform) {
            base "https://{route}.api.riotgames.com";
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
        scope summoner(route: Platform) {
            base "https://{route}.api.riotgames.com";
            path "lol/summoner/v4/summoners";

            /// `None` when this account has never played LoL on this platform.
            GET by_puuid(puuid: Puuid) -> Option<Summoner> {
                path "by-puuid/{puuid}";
                status { 404 => None }
            }
            GET me(..player) -> Summoner { path "me"; }
            #[deprecated(note = "use by_puuid")]
            GET by_id(id: SummonerId) -> Option<Summoner> {
                path "{id}";
                status { 404 => None }
            }
            #[deprecated(note = "use by_puuid")]
            GET by_account(account_id: AccountId) -> Option<Summoner> {
                path "by-account/{account_id}";
                status { 404 => None }
            }
            #[deprecated(note = "names became Riot IDs: use account().by_riot_id (unsure: still served)")]
            GET by_name(name: String) -> Option<Summoner> {
                path "by-name/{name}";
                status { 404 => None }
            }
        }

        /// tft-league-v1.
        scope tft_league(route: Platform) {
            base "https://{route}.api.riotgames.com";
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
            #[deprecated(note = "use by_puuid")]
            GET by_summoner(summoner_id: SummonerId) -> Vec<TftLeagueEntry> {
                path "entries/by-summoner/{summoner_id}";
            }
            GET entries(tier: Tier, division: Division, ..tft_queue, ..ladder) -> Vec<TftLeagueEntry> {
                path "entries/{tier}/{division}";
            }
            /// The top of a rated ladder (Hyper Roll). Not paged.
            GET top_rated(queue: RatedQueue) -> Vec<TopRated> { path "rated-ladders/{queue}/top"; }
        }

        /// tft-summoner-v1: the same profile, for TFT.
        scope tft_summoner(route: Platform) {
            base "https://{route}.api.riotgames.com";
            path "tft/summoner/v1/summoners";

            GET by_puuid(puuid: Puuid) -> Option<Summoner> {
                path "by-puuid/{puuid}";
                status { 404 => None }
            }
            GET me(..player) -> Summoner { path "me"; }
            #[deprecated(note = "use by_puuid")]
            GET by_id(id: SummonerId) -> Option<Summoner> {
                path "{id}";
                status { 404 => None }
            }
            #[deprecated(note = "use by_puuid")]
            GET by_account(account_id: AccountId) -> Option<Summoner> {
                path "by-account/{account_id}";
                status { 404 => None }
            }
            #[deprecated(note = "names became Riot IDs: use account().by_riot_id (unsure: still served)")]
            GET by_name(name: String) -> Option<Summoner> {
                path "by-name/{name}";
                status { 404 => None }
            }
        }
    }
}
```

## Beside the macro

Two files: `model.rs`, the data types, and `quota.rs`, the middleware. Nothing else: `Json` reads the answers and the problem, `by_offset_limit` and `by_page` are the pagers, and no arm or entry calls a function.

**The data types** (`model.rs`). Each derives `Deserialize` (an answer) or `Serialize` (an input or a body), with Riot's names through `serde`.

| Kind | Types |
|---|---|
| Routes, written by `serde` in a host or a path | `Platform` (below), `Region` (`americas`, `asia`, `europe`, `sea`), `TournamentApi` (`Live` is `tournament`, `Stub` is `tournament-stub`), `Game` (`lol`, `tft`, `val`, `lor`) |
| Ids, `#[serde(transparent)]` with `From` | text: `Puuid`, `MatchId`, `TftMatchId`, `SummonerId`, `AccountId`, `LeagueId`, `ClashTeamId`, `TournamentCode`; numbers: `ChampionId`, `ChallengeId`, `ClashTournamentId`, `TournamentId`, `ProviderId` |
| Riot's capitals, read in answers too, so each has `#[serde(other)] Unknown` | `Queue` (`RANKED_SOLO_5x5`, `RANKED_FLEX_SR`: a `rename` on each case, for the small `x`), `Tier`, `Division`, `TftQueue`, `RatedQueue`, `ApexLevel`; `MatchType` (`ranked`, `normal`, `tourney`, `tutorial`) |
| Answers | `Account`, `ActiveShard`, `ActiveRegion`, `Match`, `Timeline`, `Replays`, `TftMatch`, `ChampionRotations`, `PlatformData`, `ChampionMastery`, `ClashPlayer`, `ClashTeam`, `ClashTournament`, `LeagueList`, `LeagueEntry`, `TftLeagueList`, `TftLeagueEntry`, `TopRated`, `ChallengeConfig`, `Percentiles` (a map from a level to a share), `ApexPlayer`, `PlayerChallenges`, `CurrentGame` (LoL and TFT), `FeaturedGames`, `Summoner` (LoL and TFT), `CodeDetails`, `LobbyEvents`, `TournamentGame` |
| Bodies | `NewProvider`, `NewTournament`, `CodeSettings`, `CodeUpdate` |
| How Riot says no | `RiotProblem` (below) |

Two in full:

```rust
/// One server of LoL and TFT, and its host: `euw1` in `https://euw1.api.riotgames.com`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Br1, Eun1, Euw1, Jp1, Kr, La1, La2, Me1, Na1, Oc1, Ru, Sg2, Tr1, Tw2, Vn2,
    // (unsure: ph2 and th2, which Riot folded into sg2)
}

impl Platform {
    /// The region that holds this platform's accounts and matches.
    pub fn region(self) -> Region {
        use Platform::*;
        match self {
            Br1 | La1 | La2 | Na1 => Region::Americas,
            Jp1 | Kr => Region::Asia,
            Eun1 | Euw1 | Me1 | Ru | Tr1 => Region::Europe,
            Oc1 | Sg2 | Tw2 | Vn2 => Region::Sea,          // (unsure: oc1 moved to sea for matches)
        }
    }
}

/// How Riot says no: `{ "status": { "message": "Data not found", "status_code": 404 } }`.
#[derive(Clone, Debug, Deserialize)]
pub struct RiotProblem { pub status: RiotStatus }

#[derive(Clone, Debug, Deserialize)]
pub struct RiotStatus { pub message: String, pub status_code: u16 }
```

**The middleware** (`quota.rs`), in full: it is the one piece that is not obvious. Riot counts requests per key in two kinds of bucket: the application's, per host, and a method's, per host and endpoint. Every answer says how full each window of its buckets is (`x-app-rate-limit: 20:1,100:120` and `x-app-rate-limit-count: 20:1,31:120`), and a `429` says how long to stay away. `Quota` holds a bucket while it is full, and every request of the client waits for its two buckets before it goes out. Its wait is not silence (the clock for silence starts at the transport); only `with_timeout` bounds it.

```rust
use concord::{Middleware, Next, TransportError, TryInfo};     // paths guessed: finding 9
use reqwest::header::HeaderMap;
use std::{collections::HashMap, sync::{Arc, Mutex}, time::Duration};
use tokio::time::{sleep_until, Instant};

#[derive(Clone, Default)]
pub struct Quota(Arc<Mutex<HashMap<String, Instant>>>);         // a bucket, and until when it is held

impl Quota {
    pub fn new() -> Quota { Quota::default() }

    fn held(&self, bucket: &str) -> Option<Instant> {
        let now = Instant::now();
        self.0.lock().unwrap().get(bucket).copied().filter(|until| *until > now)
    }

    fn hold(&self, bucket: &str, wait: Duration) {
        let until = Instant::now() + wait;
        let mut buckets = self.0.lock().unwrap();
        let slot = buckets.entry(bucket.to_owned()).or_insert(until);
        *slot = (*slot).max(until);
    }
}

impl Middleware for Quota {
    async fn handle(&self, request: reqwest::Request, info: TryInfo, next: Next<'_>)
        -> Result<reqwest::Response, TransportError>
    {
        let app = request.url().host_str().unwrap_or_default().to_owned();  // euw1.api.riotgames.com
        let method = format!("{app} {}", info.endpoint());                   // … riot.league.entries
        while let Some(until) = self.held(&app).max(self.held(&method)) {
            sleep_until(until).await;
        }
        let answer = next.send(request, info).await?;
        let headers = answer.headers();
        for (bucket, limits, counts) in [
            (&app, "x-app-rate-limit", "x-app-rate-limit-count"),
            (&method, "x-method-rate-limit", "x-method-rate-limit-count"),
        ] {
            if let Some(window) = full_window(headers, limits, counts) {
                self.hold(bucket, window);          // a whole window: Riot does not say when it began
            }
        }
        if answer.status() == 429 {
            let wait = seconds(headers, "retry-after").unwrap_or(Duration::from_secs(1));
            match headers.get("x-rate-limit-type").and_then(|v| v.to_str().ok()) {
                Some("application") => self.hold(&app, wait),
                _ => self.hold(&method, wait),     // `method`, or `service` (unsure: per method)
            }
        }
        Ok(answer)
    }
}

/// The longest window whose count has reached its limit.
fn full_window(headers: &HeaderMap, limits: &str, counts: &str) -> Option<Duration> {
    let pairs = |name: &str| -> Vec<(u64, u64)> {
        let text = headers.get(name).and_then(|v| v.to_str().ok()).unwrap_or("");
        text.split(',').filter_map(|pair| {
            let (n, seconds) = pair.trim().split_once(':')?;
            Some((n.parse().ok()?, seconds.parse().ok()?))
        }).collect()
    };
    let counts = pairs(counts);
    pairs(limits).into_iter()
        .filter(|(limit, window)| counts.iter().any(|(n, w)| w == window && n >= limit))
        .map(|(_, window)| Duration::from_secs(window))
        .max()
}

fn seconds(headers: &HeaderMap, name: &str) -> Option<Duration> {
    headers.get(name)?.to_str().ok()?.trim().parse().ok().map(Duration::from_secs)
}
```

Two requests that leave together can both pass a bucket that the next answer shows full: the `429` that may follow holds the bucket for its `Retry-After`, and `Retry::standard()` sends a `GET` again after it, through this same wait. The live tournament API and its stub share one method bucket here (they are one endpoint of the description): stricter than Riot.

## Calling it

`try_next` is `futures::TryStreamExt`'s. Each piece is in an `async fn` that returns a `Result`.

1. A player by Riot ID, then their profile and their ranks on their platform.

```rust
use riot_api::{model::*, Riot};

let riot = Riot::new(std::env::var("RIOT_API_KEY")?).with_retry(concord::Retry::standard());

let Some(account) = riot.account(Region::Asia).by_riot_id("Hide on bush", "KR1").await? else {
    println!("no such Riot ID");
    return Ok(());
};
let profile = riot.summoner(Platform::Kr).by_puuid(&account.puuid).await?;   // None: never played LoL on KR
let ranks = riot.league(Platform::Kr).by_puuid(&account.puuid).await?;       // empty when unranked
```

2. Every ranked solo game of a season, from the region of the player's platform.

```rust
let matches = riot.matches(Platform::Euw1.region());                         // europe
let mut ids = matches.ids(&puuid).queue(420).start_time(season_start).count(100).pages().items();
while let Some(id) = ids.try_next().await? {
    if let Some(game) = matches.get(&id).await? {                            // None: past retention
        totals.add(&game);
    }
}
```

3. A whole division of the ladder, page after page.

```rust
let mut players = riot.league(Platform::Euw1)
    .entries(Queue::RankedSolo, Tier::Diamond, Division::I)
    .pages()
    .items();
while let Some(entry) = players.try_next().await? {
    println!("{} {} LP", entry.puuid, entry.league_points);
}
```

4. Is the player in a game now? A `404` is an answer; a dead key is not.

```rust
match riot.spectator(Platform::Kr).active_game(&puuid).await {
    Ok(Some(game)) => println!("in game for {} s", game.game_length),
    Ok(None) => println!("not in a game"),
    Err(e) if e.is_status(403) => eprintln!("key refused: a development key lasts a day (unsure: 403)"),
    Err(e) => return Err(e.into()),
}
```

5. The signed-in player, with the token of Riot Sign On: no API key goes on these requests.

```rust
let me = riot.account(Region::Americas).me(&token).await?;
let profile = riot.summoner(Platform::Na1).me(&token).await?;
let recent = riot.rso_matches(Region::Americas, &token).ids().count(20).send().await?;   // one page
```

6. A tournament: built against the stub, run against the live API; a `429` on a `POST`.

```rust
let tournaments = riot.tournament(if live { TournamentApi::Live } else { TournamentApi::Stub });
let provider = tournaments.register_provider(NewProvider { region: "EUW".into(), url: callback }).await?;
let id = tournaments.create(NewTournament { provider_id: provider, name: "Friday cup".into() }).await?;

// Retry never sends a POST again. Riot turns a request away with 429 before any work (unsure),
// so the caller may; the quota holds the next try until `Retry-After`.
let mut tries = 0;
let codes = loop {
    tries += 1;
    match tournaments.create_codes(id, &settings).count(8).await {
        Err(e) if e.is_status(429) && tries < 3 => continue,
        codes => break codes?,
    }
};
```

## Findings

| # | Where | What got in the way | What I wrote instead | Kind |
|---|---|---|---|---|
| 1 | 14 scopes and 4 endpoints on a routed host | The host `https://{route}.api.riotgames.com` is one fact of Riot's. A part may not hold `base`, and the client cannot name the `route` of a scope below it. | The same `base` line 18 times, one in 24 lines of the block. The other tree, `scope platform(route) { scope league { … } }`, writes it twice, and puts one more level on every call. | said with pain |
| 2 | `tournament`: `register_provider`, `create`, `create_codes` | Riot turns a request away with `429` before doing anything (unsure), so a `POST` may go out again after one. The language can say that an endpoint may be repeated (`idempotent`), not that a status proves nothing was done; and a `when` rule of `Retry` answers only "is it worth it". | Nothing in the description. The caller loops on `429` (piece 6), and the quota holds the next try. | cannot be said |
| 3 | `tournament` and its stub | The stub is five of the same endpoints under `lol/tournament-stub/v5`. The tree has no "the same children under another path". | One scope with the API as an input of its path, `path "lol/{api}/v5";`. It reads well and switches with one value; `update_code` and `games` can then be called on the stub, which has neither. | small |
| 4 | 30 lookups | `status { 404 => None }` on each one. An arm on a scope would reach its `me` and its lists, which are not an `Option`; a part with no inputs could carry it, and would put a status in the signature line. | The arm on each endpoint, as chapter 5 asks ("a statement about one endpoint"). | small |
| 5 | `..history`, `..tft_queue`, `..ladder`, `..lol_filter`, `..player` | A part takes its inputs out of the signature: `GET challenger(..tft_queue)` does not show `.queue(…)`. The "hide the blocks" view of section 3.2 now needs the parts beside it. | Five parts anyway, each with a doc comment that names its setters. | small |
| 6 | match-v5, tft-match-v1, lol-rso-match-v1 | The provider's word `match` is a Rust keyword, so it cannot name the scope. | `matches`, and `tft_matches`, `rso_matches` to keep one rule. | small |
| 7 | the history filter `type` | A Rust keyword. | `kind?: MatchType` and `"type" = kind;`: the door of section 3.3. | small |
| 8 | `middleware Quota::new();` | A quota that two clients of one key share must be a required input of the client (an optional one is refused there), so every caller would pass one. And that a clone of a client shares its middleware is said for handles and calls, not for the client. | One quota per client; a program shares it by cloning the client. I read rules 26 and U7 twice for it. | small |
| 9 | `quota.rs` | The guide names `concord::Middleware`, not where `Next`, `TryInfo` and `TransportError` live. | `use concord::{Middleware, Next, TransportError, TryInfo};`, a guess. | small |
| 10 | a `429` with a long `Retry-After` | Rule 18 says a wait is "never more than 60, and never less than a `Retry-After`"; Riot's 120-second window can ask for more, and which wins is not said. | Nothing: the quota holds the bucket, so both readings are safe here. | small |
| 11 | `x-riot-token` on the client | Rule 16: a secret "written under one `base`" does not follow another. This client has no `base`, and 19 nodes have each their own. Read twice; the tour's section 2 is the same case, and works. | The header on the client, as in the tour. | small |
| 12 | `..history` with no `.count(…)` | `by_offset_limit` ends on a page shorter than the `count` that the caller gave; it cannot know Riot's 20. | Nothing: such a history costs one request more, the empty page. | small |

What I wished I could write:

- **1** (said with pain): `part platform(route: Platform) { base "https://{route}.api.riotgames.com"; }`, then `scope league(..platform) { path "lol/league/v4"; … }`.
- **2** (cannot be said): on the client, `untouched 429;`: after this status, nothing was done, so retry may send any request again.

## Measures

| | |
|---|---|
| Endpoints | **75** in the block. They cover 80 operations of Riot's: the 5 of the tournament stub are 5 of the 7 tournament endpoints, with another input. 14 are legacy (`#[deprecated]`). |
| Lines of the `api!` block | **431**, from `concord::api! {` to its `}`: 67 of them doc comments, 14 `#[deprecated]`, 19 `base` (18 of them the same line). |
| An input's name written a second time only to place it | **106**: every input is placed once. 87 are holes of a `path` or a `base` (`{puuid}`, `{route}`); 19 are entries or bodies (`count;`, `"startTime" = start_time;`, `json(settings)`). Not counted: 3 names in `paginate` (`start`, `count`, `page`), which give a place to a pager, not to the wire. |
| Inputs in all | **106** written (1 of the client, 9 in five parts, 96 on scopes and endpoints). With the parts unfolded, the nodes have 124: the parts saved 18 inputs and 18 placements. |
| Lines of Rust beside the macro only because the language could not say something | **0** beside the macro. The quota (73 lines with its blank lines) is the door that the guide gives for a quota on purpose (chapter 6, section 6.6); `Platform::region` (12 lines) is ordinary Rust. In calling code, the 8 lines of the loop in piece 6 exist because of finding 2. |

## Scores

- **Writing this description: 7/10.** Riot's reference became the tree almost line by line, and the RSO token as a part was a pleasure; the 18 identical `base` lines, and choosing where a part helps and where it hides, took the rest.
- **Reading it back a month later: 7/10.** Each endpoint reads like Riot's own page, but the same `base` on every scope is noise, and five parts send a reader to the top of the block.
- **The calling code: 8/10.** `riot.matches(Europe).ids(&puuid).count(100).pages().items()` is the API itself; the platform and the region of one player, and a loop for a `POST` after a `429`, are the only ceremony.
