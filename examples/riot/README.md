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
        format Json;

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
credential-free client, so the key can never reach that host. The declaration states `Accept`
itself; `format Json;` alone would make Concord suggest the same value, and a declared `Accept` is
sent as written.

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
HTML), `429` handling, redirects, and that the key appears on the wire but not in Concord's output.

The fixture replies carry realistic `Content-Type`s (`application/json`, `text/plain`,
`application/octet-stream` for the PNG), because `Json` checks the response head before it parses
the body. A `200` that is not labelled JSON (a proxy's HTML page) is an `ErrorKind::Metadata` error
whose response stays readable; a body labelled JSON that is not JSON is an `ErrorKind::Decode`
error (`tests/errors.rs`). Neither prints the key.

## Copying it into a project of your own

This directory is a member of the Concord workspace: its manifest inherits `rust-version`,
`license`, the `serde`/`tokio`/`serde_json` requirements and the lints from the root `Cargo.toml`,
and a workspace build unifies features across all members, which can hide a feature a member
forgets. A copy elsewhere is its own workspace and states everything itself:

```toml
[package]
name = "riot-client"
version = "0.1.0"
edition = "2024"
rust-version = "1.99.1"
publish = false

[workspace] # its own root, also when it sits inside another workspace's directory

[dependencies]
# Default features: `json` (which brings `Binary`) and reqwest's defaults (TLS for live mode).
concord = { path = "../concord_api/crates/concord" }
serde = { version = "1", features = ["derive"] }
tokio = { version = "1", default-features = false, features = ["macros", "rt-multi-thread"] }

[dev-dependencies]
serde_json = "1"
tokio = { version = "1", default-features = false, features = ["macros", "rt", "rt-multi-thread", "net", "io-util", "sync", "time"] }
```

Copy `src/`, `tests/` and `fixtures/` (the tests read the fixtures from the package directory).
`src/main.rs` and the tests import the library as `concord_example_riot`: rename those imports to
your package's name (`riot_client` here). The workspace lints are not needed; copy the root's `[workspace.lints]` tables as `[lints]` if you
want them. With `default-features = false` on `concord`, enable `json` yourself and choose
reqwest's TLS through your own `reqwest` dependency.

## Why the client refuses redirects

The `reqwest::Client` that `Riot` uses is built with [`http_policy`](src/lib.rs) (live mode:
`live_http_client()`, which adds HTTPS-only): redirects are never followed and each request has a
10-second timeout. reqwest follows up to ten redirects by default and, on a cross-origin redirect,
removes only a fixed list of credential headers (`Authorization`, cookies, proxy credentials).
`X-Riot-Token` is not on that list, so a followed redirect would send the API key to the new
origin. With the policy a `3xx` reaches the caller: `send()` reports it as an `ErrorKind::Status`
error that holds the unread response, and `send_raw()` returns it.

This is the application's transport choice, made where the `reqwest::Client` is built; Concord
itself has no redirect policy and never configures your client. [`tests/redirects.rs`](tests/redirects.rs)
shows both sides on two loopback origins: with the policy the second origin receives nothing, and
a control with reqwest's default policy shows the key arriving there. If a future reqwest strips
that header, the control fails and the comment on `http_policy` must change.

Optional, read-only live mode (your key, in the environment; at most `4 + 5` GET requests, no
retries, never the tournament stub):

```bash
RIOT_API_KEY=… cargo run -p concord-example-riot -- live "<gameName>" "<tagLine>" europe euw1 europe 3
```

Live access was not exercised for this example; passing fixture tests say nothing about a key's
eligibility. Which keys may call the tournament stub, and its routing value, are not stated by the
Riot pages retrieved (`SOURCES.md`); the stub is only ever called against local fixtures here.
