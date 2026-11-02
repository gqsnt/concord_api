# Riot Games with Concord

A typed client for a sourced subset of the Riot Games API (League of Legends) and Data Dragon,
built with `concord::api!`, plus two application workflows. Everything runs offline against
synthetic fixtures; no Riot account or key is needed.

```rust,ignore
pub const X_RIOT_TOKEN: HeaderName = HeaderName::from_static("x-riot-token");

concord::api! {
    pub client Riot(#[sensitive] header(X_RIOT_TOKEN) key: String) {
        base "https://api.riotgames.com";
        headers { ACCEPT = "application/json"; }
        format json;

        scope account(route: AccountRoute) {          // americas.api.riotgames.com, ...
            subdomain "{route}";
            path "riot/account/v1/accounts";
            GET by_riot_id(game_name: String, tag_line: String) -> Account {
                path "by-riot-id/{game_name}/{tag_line}";
            }
        }
        scope tournament_stub(route: AccountRoute) {
            subdomain "{route}";
            path "lol/tournament-stub/v5";
            POST create_codes(
                query("tournamentId") tournament: TournamentId,
                query count?: u32,
                body parameters: TournamentCodeParameters,
            ) -> Vec<TournamentCode> { path "codes"; }
        }
        // platform(route: Platform) { summoner, top_masteries, rotation }, matches(route: MatchRoute) { ids, get }
    }
}
```

The whole declaration is [`src/api.rs`](src/api.rs). The key is a native `HeaderName` constant
marked `#[sensitive]`: it is sent in `x-riot-token` (not as `Authorization`) and withheld from
Concord's `Debug` output and errors. Each scope takes the route type its operations accept
(account and match routing differ; see [`SOURCES.md`](SOURCES.md)). Data Dragon is a separate,
credential-free client, so the key can never reach that host.

```rust,ignore
let riot = Riot::new(reqwest::Client::new(), RiotConfig { key });
let summary = player_summary(
    &riot, "Fixture Player", "EUW",
    AccountRoute::Europe, Platform::Euw1, MatchRoute::Europe, 3,
).await?;               // Riot ID → PUUID → summoner, masteries, ≤ 5 recent matches
```

[`player_summary`](src/lib.rs) and `tournament_codes` are application code: the request budget,
stopping on `429` (with Riot's `Retry-After`) and route choice live there, not in Concord.

## Run

```bash
cargo test -p concord-example-riot
```

The tests resolve Riot's routed host names to a loopback server and assert exact methods, hosts,
paths, queries, headers and body bytes, decoding of the fixtures, non-success bodies (JSON, empty,
HTML), `429` handling, and that the key appears on the wire but not in Concord's output.

Optional, read-only live mode (your key, in the environment; at most `4 + 5` GET requests, no
retries, never the tournament stub):

```bash
RIOT_API_KEY=… cargo run -p concord-example-riot -- live "<gameName>" "<tagLine>" europe euw1 europe 3
```

Live access was not exercised for this example; passing fixture tests say nothing about a key's
eligibility. Which keys may call the tournament stub, and its routing value, are not stated by the
Riot pages retrieved (`SOURCES.md`); the stub is only ever called against local fixtures here.
