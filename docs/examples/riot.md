# Riot Games

*[Concord](../../README.md) · [docs](../README.md) · examples · previous: [GitHub](github.md) · next: [an object store like Amazon S3](s3.md)*

This page is for a Rust developer who calls the Riot Games API (League of Legends, Teamfight Tactics, Riot accounts). It gives a client with 62 endpoints that you can copy, and shows many hosts chosen by an input, and a quota that every call shares.

Here is Riot as Concord sees it. A route is a host: a platform (`euw1`, `kr`) serves what lives on one game server, and a region (`europe`, `asia`) serves accounts and matches. Two parts, `platform` and `region`, each contain the statement `base "https://{route}.api.riotgames.com";`, and each of Riot's API groups (`league-v4`, `match-v5`, …) is a scope that includes one of them: `riot.league(Platform::Euw1)`. Authentication is the developer portal key, sent in the header `x-riot-token` to every host; for a signed-in player, the Riot Sign On token replaces it (the part `player`). A match history is paginated by `start` and `count` (`by_offset_limit`), and a ladder by page number (`by_page`). Riot rejects a request with an error status and `{"status": {"message": …, "status_code": …}}`, read into `RiotProblem`; "no such player" is a `404`, which the lookups turn into `None`. Every response reports how full the rate limits are, per host and per method, and a middleware layer reads it. Riot's own reference is [developer.riotgames.com/apis](https://developer.riotgames.com/apis). `(unsure)` marks a fact about Riot that has not been checked.

## The description

```rust
use concord::{by_offset_limit, by_page, json, Json, Secret};
use std::collections::HashMap;

concord::api! {
    /// League of Legends, Teamfight Tactics and Riot accounts.
    ///
    /// A route is a host. A platform (`euw1`, `kr`) holds what lives on one server; a region
    /// (`europe`, `asia`) holds accounts and matches. `Platform::region()` maps a platform to its region.
    /// Retries are off until you turn them on: `.with_retry(retry())` waits out a `429`, also on a `POST`.
    pub client Riot(
        /// The key of the developer portal. A development key lasts a day.
        key: Secret,
    ) {
        headers { "x-riot-token" = key; }
        read Json;
        problem RiotProblem;
        /// Riot's limits, per host and per method, shared by every call of this client.
        /// It waits before a request that the last responses say is over a limit, and
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
        /// A match history, newest first: `.start(…)`, `.count(…)` (1 to 100; Riot returns 20),
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
            /// The platform a player of `lol` or `tft` plays on. (unsure: path and response)
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
            /// The best champions. Riot returns 3 when `count` is not set.
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
            /// The best players at an apex level. Not paginated: `limit` is all there is.
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
            /// The top of a rated ladder (Hyper Roll). Not paginated.
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
```

## Beside the description

The data types are plain `serde` types with Riot's names, named here but not shown. Ids, each a `#[serde(transparent)]` newtype: `Puuid`, `MatchId`, `TftMatchId`, `SummonerId`, `LeagueId`, `ClashTeamId`, `TournamentCode` (strings); `ChampionId`, `ChallengeId`, `ClashTournamentId`, `TournamentId`, `ProviderId` (numbers). Riot's enumerated values, each with `#[serde(other)] Unknown` for a value that Riot adds later: `Queue` (`RANKED_SOLO_5x5`, `RANKED_FLEX_SR`), `Tier`, `Division`, `TftQueue`, `RatedQueue`, `ApexLevel`, `MatchType`, `Game`, `TournamentApi` (`tournament` or `tournament-stub`). The responses: `Account`, `Match`, `Timeline`, `LeagueEntry`, `Summoner`, `CurrentGame`, and the others that the description names. The bodies: `NewProvider`, `NewTournament`, `CodeSettings`, `CodeUpdate`.

The routes, Riot's error type, and its retry policy:

```rust
use serde::{Deserialize, Serialize};

/// One server of LoL and TFT, and its host: `euw1` in `https://euw1.api.riotgames.com`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Br1, Eun1, Euw1, Jp1, Kr, La1, La2, Me1, Na1, Oc1, Ru, Sg2, Tr1, Tw2, Vn2,
}

/// A region, and its host: it holds the accounts and the matches.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Region {
    Americas, Asia, Europe, Sea,
}

impl Platform {
    /// The region that holds this platform's accounts and matches.
    pub fn region(self) -> Region {
        use Platform::*;
        match self {
            Br1 | La1 | La2 | Na1 => Region::Americas,
            Jp1 | Kr => Region::Asia,
            Eun1 | Euw1 | Me1 | Ru | Tr1 => Region::Europe,
            Oc1 | Sg2 | Tw2 | Vn2 => Region::Sea,
        }
    }
}

/// Riot's error response: `{"status": {"message": "Data not found", "status_code": 404}}`.
#[derive(Clone, Debug, Deserialize)]
pub struct RiotProblem {
    pub status: RiotStatus,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RiotStatus {
    pub message: String,
    pub status_code: u16,
}

/// Riot's policy: a `429` was turned away before any work, so even a `POST` may be retried.
pub fn retry() -> riot::Retry {
    riot::Retry::standard().not_applied_on(429)
}
```

The quota is shown in full, because it is the only part that is not obvious. Riot counts requests per key in two kinds of bucket: the application's, per host, and a method's, per host and endpoint. Every response reports how full each window of its buckets is (`x-app-rate-limit: 20:1,100:120` and `x-app-rate-limit-count: 20:1,31:120`), and a `429` says how long to wait. `Quota` holds a bucket while it is full, and each request waits for its two buckets before it is sent. That wait does not count as silence for the idle timeout, so only the call timeout, `with_timeout`, bounds it. This block needs `tokio`, with its `time` feature.

```rust
use concord::reqwest::{self, header::HeaderMap};
use concord::transport::{TransportError, TryInfo};
use concord::{Middleware, Next};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::{sleep_until, Instant};

/// Riot's limits: a bucket for the application on each host, and one for each method. A bucket
/// that the responses show full is held, and a request waits for its two buckets before it is sent.
#[derive(Clone, Default)]
pub struct Quota(Arc<Mutex<HashMap<String, Instant>>>);

impl Quota {
    pub fn new() -> Quota {
        Quota::default()
    }

    fn held(&self, bucket: &str) -> Option<Instant> {
        let now = Instant::now();
        let buckets = self.0.lock().unwrap_or_else(|e| e.into_inner());
        buckets.get(bucket).copied().filter(|until| *until > now)
    }

    fn hold(&self, bucket: &str, wait: Duration) {
        let until = Instant::now() + wait;
        let mut buckets = self.0.lock().unwrap_or_else(|e| e.into_inner());
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
                self.hold(bucket, window);      // a whole window: Riot does not say when it began
            }
        }
        if answer.status() == 429 {
            let wait = seconds(headers, "retry-after").unwrap_or(Duration::from_secs(1));
            match headers.get("x-rate-limit-type").and_then(|v| v.to_str().ok()) {
                Some("application") => self.hold(&app, wait),
                _ => self.hold(&method, wait),
            }
        }
        Ok(answer)
    }
}

/// The longest window whose count has reached its limit: `20:1,100:120` against `20:1,31:120`.
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

Two requests sent at the same time can both pass a bucket that the next response shows as full. The `429` that may follow holds the bucket for its `Retry-After`, and `retry()` retries the request after that, through the same wait. Here the live tournament API and its stub share one method bucket, because they are one endpoint of the description, which is stricter than Riot.

## Calling it

```rust
use concord::Secret;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// A player by Riot ID, then their ranks on their platform.
pub async fn ranks() -> Result<(), BoxError> {
    let riot = Riot::new(std::env::var("RIOT_API_KEY")?).with_retry(retry());
    let Some(account) = riot.account(Region::Asia).by_riot_id("Hide on bush", "KR1").await? else {
        println!("no such Riot ID");
        return Ok(());
    };
    for entry in riot.league(Platform::Kr).by_puuid(&account.puuid).await? {
        println!("{} LP", entry.league_points);               // no entry: unranked
    }
    Ok(())
}

/// Every ranked solo game of a player since a date, from the region of their platform.
pub async fn season(riot: &Riot, puuid: &Puuid, since: i64) -> riot::Result<i64> {
    let matches = riot.matches(Platform::Euw1.region());
    let mut ids = matches.ids(puuid).queue(420).start_time(since).count(100).pages().items();
    let mut seconds = 0;
    while let Some(id) = ids.try_next().await? {
        if let Some(game) = matches.get(&id).await? {        // `None`: Riot no longer keeps it
            seconds += game.info.game_duration;
        }
    }
    Ok(seconds)
}

/// A whole division of the ladder, page after page.
pub async fn division(riot: &Riot) -> riot::Result<()> {
    let mut players = riot.league(Platform::Euw1)
        .entries(Queue::RankedSolo, Tier::Diamond, Division::I)
        .pages()
        .items();
    while let Some(entry) = players.try_next().await? {
        println!("{:?} {} LP", entry.puuid, entry.league_points);
    }
    Ok(())
}

/// Is the player in a game now? A `404` is an answer; a key that expired is an error.
pub async fn in_game(riot: &Riot, puuid: &Puuid) -> riot::Result<()> {
    match riot.spectator(Platform::Kr).active_game(puuid).await {
        Ok(Some(game)) => println!("in a game for {} s", game.game_length),
        Ok(None) => println!("not in a game"),
        Err(e) if e.is_status(403) => eprintln!("the key was refused: a development key lasts a day"),
        Err(e) => return Err(e),
    }
    Ok(())
}

/// The signed-in player, with the token of Riot Sign On: no API key goes on these requests.
pub async fn me(riot: &Riot, token: Secret) -> riot::Result<()> {
    let account = riot.account(Region::Americas).me(&token).await?;
    let profile = riot.summoner(Platform::Na1).me(&token).await?;
    let recent = riot.rso_matches(Region::Americas, &token).ids().count(20).send().await?;
    println!("{:?}: level {}, {} recent games", account.game_name, profile.summoner_level, recent.len());
    Ok(())
}

/// A tournament: written against the stub, run against the live API. With `retry()`, a `429`
/// on these `POST`s waits for its `Retry-After` and is retried.
pub async fn tournament(riot: &Riot, live: bool, callback: String) -> riot::Result<Vec<TournamentCode>> {
    let tournaments = riot.tournament(if live { TournamentApi::Live } else { TournamentApi::Stub });
    let provider = tournaments.register_provider(NewProvider { region: "EUW".into(), url: callback }).await?;
    let tournament = tournaments.create(NewTournament { provider_id: provider, name: "Friday cup".into() }).await?;
    tournaments.create_codes(tournament, CodeSettings {}).count(8).await
}
```

## What does not fit well, and the workaround

| Riot wants | What to write |
|---|---|
| The tournament API and its stub: the same calls under `lol/tournament/v5` and `lol/tournament-stub/v5`. | One scope with the API as an input of its path, `path "lol/{api}/v5";`. The stub has no `update_code` and no `games`, but the client lets you call them there, and the stub rejects them. |
| "No such thing" as a `404` on 23 lookups. | `status { 404 => None }` on each lookup. On a scope, the arm would also apply to its lists, which are not an `Option`. |
| `match` and `type`, which are Rust keywords. | `matches` for the scope; `kind?: MatchType` with `"type" = kind;`. |
| A `429` on a `POST` of the tournament API: Riot rejected it before doing anything (unsure). | `Retry::standard().not_applied_on(429)`, provided as `retry()`. Not `idempotent;`: that would also retry the `POST` after a lost connection. |
| A `Retry-After` longer than 60 seconds (a window of 120 seconds). | Concord does not wait more than 60 seconds: the call fails, and `e.retry_after()` returns how long to wait. The quota still holds the bucket for the next call. |
| One quota for every client of one key. | One client for each key, cloned where it is needed: a clone shares the middleware. |
| A history without `.count(…)`: Riot returns 20. `by_offset_limit` stops at a page shorter than the `count` that the caller set, and knows nothing about Riot's 20. | Set `.count(…)`; without it, the last request returns an empty page. |
| The endpoints keyed by summoner id, which Riot replaced by the PUUID. | Left out, except one: `#[deprecated(note = …)]` on an endpoint warns each caller at compile time. |
