# the Riot Games API, in full in Concord
Written by GPT-6 (Codex), from the supplied Concord guide and remembered Riot public documentation, with public documentation cross-checks, on 2026-11-12.

## The description

The inventory covers every requested family, TFT spectator, and the remembered LoL tournament, RSO-match and legacy operations. `(unsure)` marks provider details I cannot vouch for, including continued availability; it is not proposed Concord syntax. Routes and the account migration are documented in [Riot's LoL guide](https://developer.riotgames.com/docs/lol); the [reference index](https://developer.riotgames.com/apis/) also lists TFT spectator, tournaments and RSO-match. I read public documentation only; no request was sent to an API host. Data Dragon, local game/client APIs and receiving tournament callbacks are separate services, outside this routed client.

```rust
use concord::{Json, Secret, offset_limit};
use crate::model::*;

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
                GET ids(puuid: Puuid, start: Option<u32>, count: Option<u32>,
                    queue: Option<i32>, match_type: Option<MatchType>,
                    start_time: Option<i64>, end_time: Option<i64>) -> Vec<MatchId> {
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
                GET ids(puuid: Puuid, start: Option<u32>, count: Option<u32>,
                    start_time: Option<i64>, end_time: Option<i64>) -> Vec<TftMatchId> {
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
                GET ids(puuid: Puuid, start: Option<u32>, count: Option<u32>) -> Vec<MatchId> {
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
                    count: Option<u32>) -> Vec<TournamentCode> {
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
                    count: Option<u32>) -> Vec<TournamentCode> {
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
                GET top(puuid: Puuid, count: Option<u32>) -> Vec<ChampionMastery> {
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
                GET top_by_summoner(summoner_id: SummonerId, count: Option<u32>) -> Vec<ChampionMastery> {
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
                GET entries(queue: Queue, tier: Tier, division: Division, page: Option<u32>) -> Vec<LeagueEntry> {
                    path "entries/{queue}/{tier}/{division}";
                    query { page; }
                    paginate crate::league_pages!();
                }
            }
            /// league-exp-v4: also covers apex tiers (accepted division there is (unsure)).
            scope league_exp {
                path "lol/league-exp/v4";
                GET entries(queue: Queue, tier: Tier, division: Division, page: Option<u32>) -> Vec<LeagueEntry> {
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
                GET leaders(challenge_id: ChallengeId, level: ChallengeLevel, limit: Option<u32>) -> Vec<ChallengeLeader> {
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
                GET challenger(queue: Option<TftQueue>) -> TftLeagueList {
                    path "challenger"; query { queue; }
                }
                GET grandmaster(queue: Option<TftQueue>) -> TftLeagueList {
                    path "grandmaster"; query { queue; }
                }
                GET master(queue: Option<TftQueue>) -> TftLeagueList {
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
                GET entries(tier: Tier, division: Division, queue: Option<TftQueue>, page: Option<u32>) -> Vec<TftLeagueEntry> {
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
```

## Beside the macro

These are crate modules, not additional Concord constructs. The two complete DTOs are `Account` and `RiotProblem` (including its nested status). The remaining DTO declarations are schema signatures with fields deliberately elided, as requested; they are not proposed empty implementations. Their published implementations derive `Clone`, `Debug`, and `Deserialize`, use Riot's field names through serde, ignore unknown fields, and retain unknown enum strings. Requests also derive `Serialize`. No custom reader is needed: `Json` reads both ordinary replies and the problem.

```rust
pub mod model {
    use serde::{Deserialize, Serialize};
    use std::{collections::BTreeMap, fmt};

    // One wire scalar per ID/name; no accidental interchange of the two match ID kinds.
    macro_rules! wire_types {
        ($($name:ident($wire:ty)),+ $(,)?) => { $(
            #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
            #[serde(transparent)]
            pub struct $name(pub $wire);
            impl From<$wire> for $name { fn from(value: $wire) -> Self { Self(value) } }
            impl fmt::Display for $name {
                fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { fmt::Display::fmt(&self.0, f) }
            }
        )+ };
    }
    wire_types!(Puuid(String), MatchId(String), TftMatchId(String), SummonerId(String),
        EncryptedAccountId(String), LeagueId(String), ClashTeamId(String),
        TournamentCode(String), GameName(String), TagLine(String), Game(String),
        Queue(String), TftQueue(String), Tier(String), Division(String),
        ChallengeLevel(String), MatchType(String), ChampionId(i64), ChallengeId(i64),
        ClashTournamentId(u32), TournamentId(i32), ProviderId(i32));

    #[derive(Clone, Copy, Debug, Serialize, Deserialize)]
    #[serde(rename_all = "lowercase")]
    pub enum Region { Americas, Asia, Europe, Sea }
    #[derive(Clone, Copy, Debug, Serialize, Deserialize)]
    #[serde(rename_all = "lowercase")]
    pub enum Platform {
        Br1, Eun1, Euw1, Jp1, Kr, La1, La2, Na1, Oc1, Tr1, Ru,
        Ph2, Sg2, Th2, Tw2, Vn2,
        // (unsure) Me1 availability, and which legacy SEA hosts remain independently routed.
        Me1,
    }

    #[derive(Clone, Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Account {
        pub puuid: Puuid,
        pub game_name: Option<GameName>,
        pub tag_line: Option<TagLine>,
    }
    #[derive(Clone, Debug, Deserialize)]
    pub struct RiotProblem { pub status: RiotStatus }
    #[derive(Clone, Debug, Deserialize)]
    pub struct RiotStatus { pub message: String, pub status_code: u16 }

    pub type LevelPercentiles = BTreeMap<ChallengeLevel, f64>;
    pub type ChallengePercentiles = BTreeMap<ChallengeId, LevelPercentiles>;
    pub struct ActiveShard { /* puuid, game, activeShard */ }
    pub struct AccountRegion { /* puuid, game, region: String; mapping is (unsure) */ }
    pub struct ChampionMastery { /* championId, championLevel/Points, milestones, rewards */ }
    pub struct ChampionRotations { /* freeChampionIds, new-player IDs and max level */ }
    pub struct ClashPlayer { /* puuid/summonerId (unsure), teamId, position, role */ }
    pub struct ClashTeam { /* id, tournamentId, name, abbreviation, captain, players */ }
    pub struct ClashTournament { /* id, themeId, nameKey, nameKeySecondary, schedule */ }
    pub struct LeagueList { /* leagueId, tier, queue, name, items: Vec<LeagueItem> */ }
    pub struct LeagueItem { /* player identifier, rank, points, wins/losses, miniSeries */ }
    pub struct LeagueEntry { /* leagueId, queueType, tier, rank, points, wins/losses */ }
    pub struct MiniSeries { /* losses, progress, target, wins */ }
    pub struct ChallengeConfig { /* id, localizedNames, state, tracking, thresholds */ }
    pub struct ChallengeLeader { /* puuid, value, position */ }
    pub struct PlayerChallenges { /* totalPoints, categoryPoints, challenges, preferences */ }
    pub struct PlatformStatus { /* id, name, locales, maintenances, incidents */ }
    pub struct StatusIncident { /* id, timestamps, titles, updates, platforms */ }
    pub struct Match { /* metadata: MatchMetadata, info: MatchInfo */ }
    pub struct MatchMetadata { /* dataVersion, matchId, participants: Vec<Puuid> */ }
    pub struct MatchInfo { /* queue/map/game IDs, times, teams, participants */ }
    pub struct MatchParticipant { /* Puuid, champion, perks, challenges, stats */ }
    pub struct MatchTeam { /* objectives, bans, win, teamId */ }
    pub struct MatchTimeline { /* metadata: MatchMetadata, info: TimelineInfo */ }
    pub struct TimelineInfo { /* frameInterval, gameId, participants, frames */ }
    pub struct TimelineFrame { /* timestamp, participantFrames: BTreeMap<u32, ParticipantFrame>, events */ }
    pub struct ParticipantFrame { /* participantId, position, gold, XP, champion/damage stats */ }
    pub struct TimelineEvent { /* timestamp, open event type, event-specific optional fields */ }
    pub struct PlayerReplays { /* (unsure) replay-list response */ }
    pub struct CurrentGame { /* gameId, observers, participants, bans, queue, start/length */ }
    pub struct FeaturedGames { /* gameList, clientRefreshInterval */ }
    pub struct Summoner { /* puuid, profileIconId, revisionDate, summonerLevel; legacy IDs optional */ }
    pub struct TftLeagueList { /* leagueId, tier, queue, name, entries */ }
    pub struct TftLeagueEntry { /* player ID, tier/rank, points, wins/losses */ }
    pub struct TftTopRated { /* puuid/summonerId (unsure), rating, tier, wins */ }
    pub struct TftMatch { /* metadata: TftMetadata, info: TftMatchInfo */ }
    pub struct TftMetadata { /* data_version, match_id, participants */ }
    pub struct TftMatchInfo { /* game_datetime, length, version, set, queue_id, participants */ }
    pub struct TftParticipant { /* puuid, placement, traits, units, augments, companions */ }
    pub struct TftCurrentGame { /* observer and participant DTOs, game identity/times */ }
    pub struct TftSummoner { /* puuid, profileIconId, revisionDate, summonerLevel */ }
    pub struct CodeParameters { /* map/pick/spectator types, teamSize, metadata, allowed PUUIDs (unsure) */ }
    pub struct CodeUpdate { /* updateable code settings, allowed participants (unsure) */ }
    pub struct CodeDetails { /* code, tournament/provider IDs, settings, participants */ }
    pub struct LobbyEvents { /* eventList: Vec<LobbyEvent> */ }
    pub struct LobbyEvent { /* timestamp, eventType, player identifier */ }
    pub struct TournamentGame { /* (unsure) tournament game result */ }
    pub struct ProviderRegistration { /* region, url: callback URL */ }
    pub struct TournamentRegistration { /* providerId, name */ }
}
```

The page-number pager is complete below. A Rust macro supplies a closure whose call/answer/error types are inferred at each of the three uses. This avoids guessing the unnamed error type of `concord::mismatch`. `page` is a public call input field under §6.4; `.page(next)` is its generated setter. An omitted page means the server's documented first page, 1. Empty-page termination is the remembered league contract (unsure for future changes); no page size is assumed. The shipped `offset_limit` on LoL/TFT history advances by the number returned and uses a short-page stop only when count was explicitly supplied, as the Concord guide specifies for Riot.

```rust
#[macro_export]
macro_rules! league_pages {
    () => {
        |call, answer| {
            if answer.value().is_empty() { return Ok(None); }
            match call.page.unwrap_or(1).checked_add(1) {
                Some(next) => Ok(Some(call.page(next))),
                None => Err(concord::mismatch("league page number overflow")),
            }
        }
    };
}

/// Standard retry honors Retry-After (seconds or date). Missing header: author policy, 1 s.
/// At most 3 tries and at most 60 s waiting per retry; a larger header returns the 429.
pub fn retry() -> concord::Retry<model::RiotProblem> {
    concord::Retry::standard().when(|tried| {
        tried.status().is_some_and(|s| s == 429).then(|| {
            tried.headers().retry_after().unwrap_or(std::time::Duration::from_secs(1))
        })
    })
}
```

Riot requires a pause after a rate-limit response ([portal documentation](https://developer.riotgames.com/docs/portal)). Retrying one call does not pause its siblings. This complete transport conservatively stops *all* new requests on the affected host for this key, regardless of app/method/service bucket. Share one `Gate` across clients using that key. Already sent requests remain in flight. It is a reactive cooldown, not a proactive quota counter or a distributed limiter; it sends once per invocation and leaves retry to Concord. No API-specific `429` arm consumes the error body.

```rust
use concord::transport::{Transport, TransportError, TryInfo};
use std::{collections::HashMap, sync::{Arc, Mutex}, time::Duration};
use tokio::time::Instant;

#[derive(Clone, Copy)]
enum Hold { Until(Instant), Indefinite }
#[derive(Clone, Default)]
pub struct Gate(Arc<Mutex<HashMap<String, Hold>>>);
#[derive(Debug)]
struct CoolingDown;
impl std::fmt::Display for CoolingDown {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Riot host is cooling down; wait on the shared Gate")
    }
}
impl std::error::Error for CoolingDown {}

impl Gate {
    fn hold(&self, host: &str) -> Option<Hold> {
        let mut state = self.0.lock().unwrap_or_else(|p| p.into_inner());
        match state.get(host).copied() {
            Some(Hold::Until(at)) if at <= Instant::now() => { state.remove(host); None }
            other => other,
        }
    }
    fn pause(&self, host: String, wait: Duration) {
        let new = Instant::now().checked_add(wait).map(Hold::Until).unwrap_or(Hold::Indefinite);
        let mut state = self.0.lock().unwrap_or_else(|p| p.into_inner());
        let old = state.get(&host).copied();
        let merged = match (old, new) {
            (Some(Hold::Indefinite), _) | (_, Hold::Indefinite) => Hold::Indefinite,
            (Some(Hold::Until(a)), Hold::Until(b)) => Hold::Until(a.max(b)),
            (None, b) => b,
        };
        state.insert(host, merged);
    }
    /// Run outside a call: waiting inside Transport::send would consume idle_timeout.
    /// An unrepresentable deadline fails closed; caller must replace the gate deliberately.
    pub async fn wait(&self, host: &str) -> Result<(), &'static str> {
        loop {
            match self.hold(host) {
                None => return Ok(()),
                Some(Hold::Until(at)) => tokio::time::sleep_until(at).await,
                Some(Hold::Indefinite) => return Err("Retry-After deadline is unrepresentable"),
            }
        }
    }
}
pub struct Cooldown<T> { pub below: T, pub gate: Gate }
impl<T: Transport> Transport for Cooldown<T> {
    async fn send(&self, request: reqwest::Request, info: TryInfo)
        -> Result<reqwest::Response, TransportError>
    {
        let host = request.url().host_str().unwrap_or_default().to_owned();
        if self.gate.hold(&host).is_some() {
            return Err(TransportError::blocked().with_source(CoolingDown));
        }
        let response = self.below.send(request, info).await?;
        if response.status() == 429 {
            // Riot specifies integer seconds. Invalid/missing header: our one-second fallback.
            // Date-form headers still work for Concord retry, but this gate expects Riot's form.
            let seconds = response.headers().get("retry-after")
                .and_then(|v| v.to_str().ok()).and_then(|v| v.trim().parse::<u64>().ok())
                .unwrap_or(1);
            self.gate.pause(host, Duration::from_secs(seconds));
        }
        Ok(response)
    }
}
pub fn with_cooldown(key: concord::Secret, gate: Gate)
    -> Result<Riot, Box<dyn std::error::Error + Send + Sync>>
{
    let http = concord::Http::with(|builder| builder)?;
    Ok(Riot::new(key).with_transport(Cooldown { below: http, gate }).with_retry(retry()))
}
```

The gate fails fast with `blocked()` while a pause is active; callers can wait before making another call. It never sleeps inside the transport. Sharing it across two keys unnecessarily couples their quotas. Proactive accounting of `X-App-Rate-Limit[-Count]`, `X-Method-Rate-Limit[-Count]` and service buckets would require another stateful transport; it is not implemented or implied here. API-key replacement uses the guide's `Secret::replace`; the application supplies RSO access tokens and owns their refresh.

## Calling it

Six independent snippets inside async functions. `?` propagates to a surrounding `Result`; examples with mixed setup errors use `Box<dyn std::error::Error + Send + Sync>`. Imports are `use crate::{Riot, model::*}; use futures_util::TryStreamExt;`. Tuple constructors are intentional: text arguments are accepted for `String`/`Secret`, not arbitrary newtypes. No snippet was executed.

1. Find a player by Riot ID, then use their PUUID on a platform.

```rust
let riot = Riot::new(key);
let account = riot.regional(Region::Europe).account()
    .by_riot_id(GameName("Example Player".into()), TagLine("EUW".into())).await?;
match account {
    Some(a) => match riot.platform(Platform::Euw1).summoner().by_puuid(&a.puuid).await? {
        Some(s) => println!("{s:?}"),
        None => println!("account exists, but no LoL summoner on EUW1"),
    },
    None => println!("no account with that Riot ID"),
}
```

2. Traverse LoL history, filter on a queue, and fetch a match and its timeline.

```rust
let matches = riot.regional(Region::Europe).matches();
let mut ids = matches.ids(&puuid).count(100).queue(420).start_time(1_790_000_000).items();
while let Some(id) = ids.try_next().await? {
    let Some(game) = matches.get(&id).await? else { continue; }; // no longer available
    let timeline = matches.timeline(&id).await?; // Option: its availability is independent
    println!("{game:?} {timeline:?}");
    break; // demonstration budget; remove to traverse history, which can change between pages
}
```

3. Read a league by page number; use the same surface for experimental and TFT leagues.

```rust
let euw = riot.platform(Platform::Euw1);
let mut pages = euw.league().entries(Queue("RANKED_SOLO_5x5".into()),
    Tier("DIAMOND".into()), Division("I".into())).page(1).pages();
while let Some(entries) = pages.try_next().await? { println!("{entries:?}"); }
let first_tft_page = euw.tft_league().entries(Tier("GOLD".into()), Division("II".into()))
    .queue(TftQueue("RANKED_TFT".into())).await?;
println!("{first_tft_page:?}");
```

4. A player outside a game is an ordinary answer; authenticate the signed-in player.

```rust
match riot.platform(Platform::Euw1).spectator().current_game(&puuid).await? {
    Some(game) => println!("{game:?}"),
    None => println!("no observable LoL game"),
}
// The app has already obtained access_token via RSO; this sends no x-riot-token.
let me = riot.regional(Region::Europe).account().me(access_token).await?;
println!("{}", me.puuid);
// An RSO-only program can use Riot::new(""); the unused Secret input is still required.
```

5. Follow TFT history on a regional cluster, and inspect platform data separately.

```rust
let tft = riot.regional(Region::Europe).tft_matches();
let mut ids = tft.ids(&puuid).start(0).count(20).items();
if let Some(id) = ids.try_next().await? { println!("{:?}", tft.get(id).await?); }
let top = riot.platform(Platform::Euw1).tft_league()
    .top_rated(TftQueue("RANKED_TFT_TURBO".into())).await?;
let status = riot.platform(Platform::Euw1).tft_status().send_full().await?;
println!("{top:?} {:?}", status.value());
```

6. Honor a shared cooldown, handle exhausted retry, and keep the provider's error evidence.

```rust
let gate = crate::Gate::default();
let riot = crate::with_cooldown(key, gate.clone())?;
gate.wait("euw1.api.riotgames.com").await?; // outside Concord's per-call clocks
match riot.platform(Platform::Euw1).mastery().top(&puuid).count(3).await {
    Ok(top) => println!("{top:?}"),
    Err(e) if e.is_status(429) => {
        // Retry was exhausted, refused a long wait, or this operation could not be repeated.
        let wait = e.headers().retry_after();
        eprintln!("rate limited after {} tries; retry-after={wait:?}", e.tries());
        if let Some(p) = e.problem() { eprintln!("{}", p.status.message); }
        return Err(e.into()); // the shared gate still prevents immediate sibling sends
    }
    Err(e) => { eprintln!("{e}"); return Err(e.into()); } // also handles a concurrent blocked call
}
```

## Findings

Provider uncertainty is marked at the operation; the table below is about writing Concord. Successful 404 handling and the two host templates themselves needed no escape hatch.

| # | where (endpoint or feature) | what got in the way | what I wrote instead | kind |
|---|---|---|---|---|
| 1 | Riot's shared app/method/service quotas | No description of quota buckets or a pause across calls; per-call retry cannot enforce Riot's cross-call pause. | A complete conservative host cooldown transport and an author constructor; proactive accounting remains application work. | cannot be said |
| 2 | league-v4, league-exp-v4, tft-league-v1 pages | Page number is common here, but no shipped page-number helper; the error type for an error-returning helper is not named in the allowed guide. | A Rust macro returning the same inferred, overflow-checking closure at three sites. | said with pain |
| 3 | cooldown transport and idle_timeout | Sleeping for Retry-After inside send counts as silence; a 120-second cooldown would hit the default 30-second clock. | Fail fast with blocked(), and expose Gate::wait outside a call. | said with pain |
| 4 | match history filters | start/count and their query placement are repeated for LoL, TFT and RSO; the LoL filters make a long signature. | Repeat legal inputs/entries; no RSO pager without a known contract. | said with pain |
| 5 | regional account versus match services | The shared regional scope admits Sea even where account routing may only admit three clusters; the route type cannot vary by descendant. | Document the subset; separate typed account handles would repeat the base and change the public tree. | said with pain |
| 6 | model argument ergonomics | Domain newtypes do not inherit String's text-argument convenience. | GameName/TagLine and ID tuple constructors; explicit distinct LoL/TFT match IDs. | small |
| 7 | 404 on resource lookups | Option alone does not mean 404; a root arm would break collection and status return types. | Endpoint-local 404 arms, intentionally repeated; no global conversion to absence. | small |
| 8 | match_type query filter | Riot's key type is a Rust keyword and raw identifiers are refused. | match_type in the signature and "type" in query. | small |
| 9 | timeline response size | Json's inherited 10 MiB cap is easy to overlook for a large timeline. | Explicit Json.limit(64 * concord::MIB), still a bounded full-body read. | small |
| 10 | status arm and HTTP transport examples | Grammar, rules and chapter examples still show the replaced arm syntax and client_builder. | Apply the sheet: 200 { body: game } and concord::Http::with. | said with pain |
| 11 | header key versus RSO token | One client constructor still requires a Secret input when the program only calls me. | Riot::new("") supplies an unused sentinel; remove its header at each RSO endpoint and take the bearer token there. | said with pain |
| 12 | tournament callback delivery | This client tree describes outbound requests, not an inbound HTTP receiver. | ProviderRegistration names the callback URL; an application HTTP server receives callbacks. | cannot be said |

Wishes (pseudocode intentions, not additional legal syntax):

- #1: Describe Riot's quota grouping and Retry-After pause once, shared by every call using the key.
- #2: Name a shipped checked page-number pager, or find the named pager error type in the guide.
- #3: Let shared admission waiting be distinct from silence on a request already sent.
- #4: Reuse the common history paging inputs and their placement without copying them.
- #5: Express a descendant's allowed route subset without adding another public call level.
- #10: Read one current arm grammar and one current HTTP constructor across every supplied page.
- #11: Construct an RSO-only use of this client without providing an unused API key.
- #12: Describe the callback's incoming payload beside the registration that enables it.

## Read twice

| question | answer and where |
|---|---|
| Does 404 => None suppress the implicit 200 success? | No: arms add. §5.5 and rule 13; serde reads an object as Some(Account). |
| Does Option itself map 404 to absence? | No; §3.7 and §5.5 require an arm. |
| Which arm spelling wins? Must headers/status be bound? | Phase A sheet item 1: field patterns, only explicitly bound names; the grammar page is stale. |
| Must an inherited API key be referenced as client.key inside every scope? | No: place it at the root; rule 10 inherits the entry. An expression written below that needs the value uses its owner (§3.7, rule 4). |
| Can platform and region serialize as lowercase host labels? | Yes: serde supplies wire text; §3.7/rule 8. Every host slot is still one label. |
| Can the allowed host-label enum depend on the descendant endpoint? | Not found. Different input types require different signatures/tree placement. |
| Can match_type preserve Riot's type query key? | Yes, explicit entry keys (§3.4); raw keyword inputs are refused (§3.7). |
| Do optional page inputs in paginate mean their values or their places? | Their places; sheet item 8 and §6.4. Within the returned closure call.page is the value. |
| What is the public concrete error type of an error-returning pager? | Not found in the permitted pages; §6.4 shows concord::mismatch only in an inferred closure. |
| Does offset_limit know Riot's omitted count default? | No default is declared (§3.3); §6.4 says short-page termination uses the count the caller gave. With none, empty-page termination suffices. |
| Does a page-number pager stop on a short nonempty page? | The guide's by_page stops only on empty (§6.4); provider termination must be the author's statement. |
| Can a pager report overflow without discarding the page that arrived? | Yes: §6.4 defers the paging error to the following try_next(). |
| Does retry alone share the Retry-After pause with sibling calls? | No: §6.3 distinguishes retry from cross-call rate limiting; §6.6 puts state in a transport. |
| Does waiting in a transport consume idle_timeout? | Yes, §6.2 transport table. Hence the outside-call wait and fast blocked result. |
| What happens when Retry-After exceeds the retry budget? | Return the last response/error with headers; §6.3, standard max wait 60 seconds. |
| Can a PUT/POST be retried merely because Riot returned 429? | GET/PUT are idempotent by method; POST needs a guarantee or never-sent evidence (§6.3). No guarantee was assumed for tournament creation. |
| Does replacing a Secret update a retry already waiting? | No: request built once; sheet item 8 and §6.2. Future calls read the replaced key. |
| Does a me endpoint need a refresh transport just to place a token? | No: a supplied Secret header suffices (§6.2); acquiring/renewing the token is outside. |
| Which HTTP constructor replaces client_builder? | Phase A sheet item 5, concord::Http::with; §4.3's example is older. |
| Must I implement a reader or parse rate-limit errors inside an arm? | No: root Json plus problem (§5.6), retry uses status/headers (§6.3); a 429 remains an error. |
| Does one timeout bound the whole history traversal? | No: §6.4 pages are individual calls; use an application timer/page budget around the loop. |

## Scores

- **Writing this description: 6/10.** Most endpoints are straightforward, but paging glue, shared cooldowns and reconciling the sheet with the older pages take substantial attention.
- **Reading it back a month later: 7/10.** The routed tree and local exceptions are readable, while the pager macro and external admission policy need their accompanying explanations.
- **The calling code: 8/10.** Required arguments, optional setters, Option answers and streams compose well; typed text constructors and rate-limit coordination add ceremony.
