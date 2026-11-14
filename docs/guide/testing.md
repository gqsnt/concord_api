# Testing a client

*[Concord](../../README.md) · [docs](../README.md) · guide · previous: [Pages](pages.md) · next: [What Concord promises](promises.md)*

For the author and the user of a client: how to test it with no network. A function in place of the network for a plain answer; a scripted transport on virtual time for a retry or a clock; a local server for the bytes that really go out.

| You test | Use | From |
|---|---|---|
| What a call sends, and what it makes of an answer | `concord::fake(…)` | `concord-api` itself |
| A retry, a clock, a body that stalls, on virtual time | `concord_testkit::Script` | `concord-testkit` |
| The bytes on the wire, with Concord's own HTTP client | `concord_testkit::Server` | `concord-testkit` |

The examples call three clients: `Tasks` ([an API is a tree](tour.md#an-api-is-a-tree-write-it-as-one)) and `Riot` ([what is shared is written once](tour.md#what-is-shared-is-written-once)), both of the tour, and `Files`, a client with a key and a signing middleware whose `readme(repo)` is `GET {repo}/readme` with `read Text;`. All three are in `tests/tour` of the repository. The example of a retry writes its own client, `Stations`.

## A fake transport

```rust
let riot = Riot::new("test-key").with_transport(concord::fake(|request| {
    assert_eq!(request.url().host_str(), Some("europe.api.riotgames.com"));
    Ok(concord::reply(200, r#"{"puuid":"abc","gameName":"Faker","tagLine":"KR1"}"#))
}));

let account = riot.account(Europe).by_riot_id("Faker", "KR1").await?;
```

The fake takes the place of what really sends. It gets the request as it leaves, after the middlewares: for a signed API, the signed one. It gives a response (`concord::reply(status, body)`, or `concord::reply_with(status, headers, body)`), or a failure with its class. Everything of Concord runs for real on that answer: retry, the status rule, the arms, the reader, the clock. Only the network is replaced. `reply` and `reply_with` panic on a status outside 100 to 999 or on a header that is not valid: they are helpers for tests.

A failure before any head is a fake that gives an error:

```rust
use concord::error::{Class, Kind};
use concord::transport::TransportError;

let fake = concord::fake(|_| Err(TransportError::connect()));
let error = Tasks::new().with_transport(fake).get(7).await.unwrap_err();
assert_eq!(error.kind(), Kind::Transport);
assert_eq!(error.class(), Some(Class::Connect));
```

Behind `fake` is one trait with one method:

```rust
pub trait Transport: Send + Sync + 'static {
    async fn send(&self, request: reqwest::Request, info: TryInfo) -> Result<reqwest::Response, TransportError>;
}
```

`concord::Http` and `reqwest::Client` have it, a function wrapped by `concord::fake` has it, and a type of yours can have it ([a transport](extending.md#a-transport); a server that never answers is one line there). `info` tells the transport the name of the endpoint and the number of the try.

A reader of yours is tested alone, with no client, on an answer made by `concord::reply` ([reading a success](responses-and-errors.md#reading-a-success)). A client whose description signs each request gives the fake the signed request ([a middleware](credentials.md#a-middleware)).

## The lines in `Cargo.toml`

`fake` and `reply` are in `concord-api`. The test kit is a crate of its own, for your tests only: a dev-dependency, never a dependency of your library. Until the crates are on crates.io, it comes from the repository:

```toml
[dev-dependencies]
concord-testkit = { git = "https://github.com/gqsnt/concord_api" }
tokio = { version = "1", features = ["macros", "rt", "test-util"] }
```

After the first upload, the first line is `concord-testkit = "5.0.0-rc.1"`. `test-util` is the feature of Tokio that `start_paused` needs.

## A script: a transport with virtual time

`Script` plays the tries that a test writes, one for each send, in the order they were written, and records each request it was given. Under `#[tokio::test(start_paused = true)]`, time moves only when the test or the engine sleeps: a wait of an hour takes no time.

```rust
use concord_testkit::Script;
use std::time::Duration;
use tokio::time::Instant;

#[tokio::test(start_paused = true)]
async fn a_silent_server_ends_the_call_after_30_seconds() {
    let script = Script::new();
    script.never();                                  // this try never answers
    let tasks = Tasks::new().with_transport(script.clone());

    let start = Instant::now();
    let error = tasks.get(7).await.unwrap_err();
    assert_eq!(start.elapsed(), Duration::from_secs(30));
    assert_eq!(
        error.to_string(),
        "transport error: no answer for 30 s (with_idle_timeout changes it), in tasks.get"
    );
}
```

| | |
|---|---|
| `script.answer(status)` | The next try gets an answer with this status. Then, in one statement: `.after(duration)` (the head comes later), `.header(name, value)`, `.body(piece)` (one piece for each call), `.pause(duration)` between pieces, `.stall()` (the body never ends), `.cut()` (the body fails in the middle), `.read_upload()` (the request body is read first). |
| `script.fail(error)` | The next try fails before any head, with a `TransportError`; `.after(duration)` too. |
| `script.never()` | The next try never answers. |
| `script.requests()` | Every request given to the script, in order: `method()`, `url()`, `header(name)`, `body()`, `info().number()` (the try) and `at()` (its instant on virtual time). |
| `script.unused()` | How many scripted tries were not played. A try that finds nothing scripted gets a `599`, so that a test fails loudly. |

Clones of a script share it: give one to the client, keep one to read. A `Script` also works on a multi-thread runtime (`#[tokio::test(flavor = "multi_thread")]`, tasks that share one client): the tries are played in the order the sends reach it, and its waits are then real time, since only a current-thread runtime can pause the clock.

## Retry on virtual time

An API says: "a `5xx` on a `GET` may be tried again, at most 4 more times, after 1, 2, 4 and 8 seconds". The author writes that rule once, as a function beside the description, and a test plays it with the time of each try:

```rust
use concord::Json;
use concord_testkit::Script;
use std::time::Duration;
use tokio::time::Instant;

concord::api! {
    pub client Stations {
        base "https://api.test";
        read Json;
        GET list() -> Vec<String>;
        HEAD exists() -> bool { status { 2xx => true, 404 => false } idempotent false; }
    }
}

/// Every 5xx, 5 tries in all, after 1 s, 2 s, 4 s and 8 s.
pub fn retry() -> stations::Retry {
    stations::Retry::none().tries(5).when(|tried| {
        let server_error = tried.status()?.is_server_error();
        server_error.then(|| Duration::from_secs(1 << (tried.number() - 1)))
    })
}

#[tokio::test(start_paused = true)]
async fn the_policy_waits_1_2_4_8_seconds() -> stations::Result<()> {
    let script = Script::new();
    for status in [500, 502, 503, 500] {
        script.answer(status);
    }
    script.answer(200).body("[]");
    let api = Stations::new().with_transport(script.clone()).with_retry(retry());

    let start = Instant::now();
    api.list().await?;
    let sent = script.requests();
    let at: Vec<Duration> = sent.iter().map(|request| request.at() - start).collect();
    let tries: Vec<u32> = sent.iter().map(|request| request.info().number()).collect();
    assert_eq!(at, [0, 1, 3, 7, 15].map(Duration::from_secs)); // waits of 1, 2, 4, 8 s
    assert_eq!(tries, [1, 2, 3, 4, 5]);

    script.answer(503);
    let error = api.exists().await.unwrap_err();   // `idempotent false;`: not tried again
    assert_eq!((error.tries(), error.is_status(503)), (1, true));
    Ok(())
}
```

`Retry::none()` has one try and no table; `.tries(5)` allows five, and then only the rules decide. A wait from `when` is a floor: the try waits the longest of that wait, the `Retry-After` of the answer, and a random part (up to 1 s before try 2, then 2, 4, 8 s…, at most 60 s). This rule asks at least that much, so the waits are exact. The method decides first: `exists` says `idempotent false;`, so it is not sent again whatever the rule says ([retries](retries.md)).

## A local server

`Server` is a real HTTP/1.1 server on `127.0.0.1` that a test scripts. It records every request exactly as it arrived: header names in their case and in their order, a repeated name repeated. Use it with Concord's own HTTP client, to test what really goes out:

```rust
use concord_testkit::Server;

#[tokio::test]
async fn http_does_not_decompress() {
    let server = Server::start().await;
    server
        .on("GET", "/repo/readme")
        .reply(200)
        .header("content-encoding", "gzip")
        .body("as it came");
    let http = concord::Http::with(|builder| builder.no_proxy()).unwrap();

    let text = Files::new("key")
        .with_transport(http)
        .with_base(server.url())
        .readme("repo")
        .await
        .unwrap();

    assert_eq!(text, "as it came");
}
```

`with_base(server.url())` sends every request of the client to the server. `no_proxy()` keeps a proxy of the environment out of a local test: `concord::Http` reads the proxy settings of the environment ([the client](calling.md#the-client)). `server.requests()` gives what arrived: `method()`, `target()`, `path()`, `query()`, `header(name)`, `header_all(name)`, `body()`. `server.on(method, path)` scripts the answer to the next request with that method and path; rules for one route answer in the order they were written (a `503`, then a `200`). `.close()` instead reads the request and closes the connection before any byte of an answer.

## Where each piece comes from

| Piece | |
|---|---|
| `concord::fake`, `concord::reply`, `concord::reply_with` | `concord-api`, in every build. |
| `Script`, `Server` | `concord-testkit`, a dev-dependency. Its documentation is on docs.rs after the first upload; until then, `cargo doc -p concord-testkit --open`. |
| `start_paused = true` | Tokio, with its feature `test-util`; a current-thread runtime (the flavor of `#[tokio::test]` by default). |

No test of Concord calls a real API: every test runs against a fake, a script or a local server.
