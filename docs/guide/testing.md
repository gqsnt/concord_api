# Testing a client

*[Concord](../../README.md) · [docs](../README.md) · guide · previous: [Pages](pages.md) · next: [What Concord promises](promises.md)*

This page shows how to test a client without a network: a fake transport for a plain response, a scripted transport on virtual time for a retry or a timeout, and a local server for the exact bytes that are sent.

| To test | Use | From |
|---|---|---|
| What a call sends, and what it makes of a response | `concord::fake(…)` | `concord-api` itself |
| A retry, a timeout, a body that stalls, on virtual time | `concord_testkit::Script` | `concord-testkit` |
| The bytes on the wire, with Concord's own HTTP client | `concord_testkit::Server` | `concord-testkit` |

The examples use three clients: `Tasks` ([an API is a tree](tour.md#an-api-is-a-tree-write-it-as-one)) and `Riot` ([what is shared is written once](tour.md#what-is-shared-is-written-once)), both from the tour, and `Files`, a client with a key and a signing middleware, whose `readme(repo)` is `GET {repo}/readme` with `read Text;`. All three are in `tests/tour` in the repository. The retry example defines its own client, `Stations`.

## A fake transport

```rust
let riot = Riot::new("test-key").with_transport(concord::fake(|request| {
    assert_eq!(request.url().host_str(), Some("europe.api.riotgames.com"));
    Ok(concord::reply(200, r#"{"puuid":"abc","gameName":"Faker","tagLine":"KR1"}"#))
}));

let account = riot.account(Europe).by_riot_id("Faker", "KR1").await?;
```

The fake replaces the transport. It receives the request as it is sent, after the middleware: for a signed API, the signed request. It returns a response (`concord::reply(status, body)`, or `concord::reply_with(status, headers, body)`), or a failure with its class. All of Concord runs for real on that response: retry, the status rule, the arms, the reader, the timeouts. Only the network is replaced. `reply` and `reply_with` panic on a status outside 100 to 999 or on an invalid header, because they are test helpers.

To simulate a failure before any head arrives, make the fake return an error:

```rust
use concord::error::{Class, Kind};
use concord::transport::TransportError;

let fake = concord::fake(|_| Err(TransportError::connect()));
let error = Tasks::new().with_transport(fake).get(7).await.unwrap_err();
assert_eq!(error.kind(), Kind::Transport);
assert_eq!(error.class(), Some(Class::Connect));
```

Behind `fake` is a trait with one method:

```rust
pub trait Transport: Send + Sync + 'static {
    async fn send(&self, request: reqwest::Request, info: TryInfo) -> Result<reqwest::Response, TransportError>;
}
```

`concord::Http` and `reqwest::Client` implement it, a function wrapped by `concord::fake` implements it, and your own type can implement it ([a transport](extending.md#a-transport); a server that never responds takes one line there). `info` tells the transport the name of the endpoint and the attempt number.

Your own reader is tested on its own, without a client, on a response made by `concord::reply` ([reading a success](responses-and-errors.md#reading-a-success)). A client whose description signs each request passes the signed request to the fake ([a middleware](credentials.md#a-middleware)).

## What to add to `Cargo.toml`

`fake` and `reply` are in `concord-api`. The test kit is a separate crate, for your tests only: a dev-dependency, never a dependency of your library. Until the crates are published on crates.io, it comes from the repository:

```toml
[dev-dependencies]
concord-testkit = { git = "https://github.com/gqsnt/concord_api" }
tokio = { version = "1", features = ["macros", "rt", "test-util"] }
```

Once the crates are published, the first line becomes `concord-testkit = "5.0.0-rc.1"`. `test-util` is the Tokio feature that `start_paused` needs.

## A script: a transport with virtual time

`Script` plays back the attempts that a test scripts, one per send, in the order they were written, and records every request it receives. Under `#[tokio::test(start_paused = true)]`, time advances only when the test or the engine sleeps, so a one-hour wait takes no real time.

```rust
use concord_testkit::Script;
use std::time::Duration;
use tokio::time::Instant;

#[tokio::test(start_paused = true)]
async fn a_silent_server_ends_the_call_after_30_seconds() {
    let script = Script::new();
    script.never();                                  // this attempt never gets a response
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
| `script.answer(status)` | The next attempt gets a response with this status. Then, chained in one statement: `.after(duration)` (the head arrives later), `.header(name, value)`, `.body(piece)` (one chunk per call), `.pause(duration)` between chunks, `.stall()` (the body never ends), `.cut()` (the body fails halfway), `.read_upload()` (the request body is read first). |
| `script.fail(error)` | The next attempt fails before any head, with a `TransportError`; `.after(duration)` works here too. |
| `script.never()` | The next attempt never gets a response. |
| `script.requests()` | Every request the script received, in order: `method()`, `url()`, `header(name)`, `body()`, `info().number()` (the attempt) and `at()` (its instant on virtual time). |
| `script.unused()` | How many scripted attempts were not played. An attempt that finds nothing scripted gets a `599`, so that the test fails loudly. |

Clones of a script share its state: give one to the client, and keep one to inspect. A `Script` also works on a multi-thread runtime (`#[tokio::test(flavor = "multi_thread")]`, tasks that share one client): the attempts are played in the order in which the sends reach it, and its waits are then in real time, since only a current-thread runtime can pause the clock.

## Retry on virtual time

Suppose an API says: "a `5xx` on a `GET` may be retried at most 4 more times, after 1, 2, 4 and 8 seconds". The author writes that rule once, as a function beside the description, and a test checks it, including the time of each attempt:

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
    let error = api.exists().await.unwrap_err();   // `idempotent false;`: not retried
    assert_eq!((error.tries(), error.is_status(503)), (1, true));
    Ok(())
}
```

`Retry::none()` allows one attempt and has no table; `.tries(5)` allows five, and then only the rules decide. A wait from `when` is a minimum: the attempt waits for the longest of that wait, the `Retry-After` of the response, and a random part (up to 1 s before attempt 2, then 2, 4, 8 s…, at most 60 s). This rule requests at least that much, so the waits are exact. The method is checked first: `exists` declares `idempotent false;`, so it is not retried, whatever the rule says ([retries](retries.md)).

## A local server

`Server` is a real HTTP/1.1 server on `127.0.0.1`, scripted by the test. It records every request exactly as it arrived: header names with their case and in their order, and a repeated name as many times as it was sent. Use it with Concord's own HTTP client to test exactly what is sent:

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

`with_base(server.url())` sends every request of the client to the server. `no_proxy()` keeps any proxy configured in the environment out of a local test, because `concord::Http` reads the environment's proxy settings ([the client](calling.md#the-client)). `server.requests()` returns what arrived: `method()`, `target()`, `path()`, `query()`, `header(name)`, `header_all(name)`, `body()`. `server.on(method, path)` scripts the response to the next request with that method and path; several rules for one route respond in the order they were written (a `503`, then a `200`). `.close()`, used instead of a reply, reads the request and closes the connection before sending any byte of a response.

## Where each tool comes from

| Tool | |
|---|---|
| `concord::fake`, `concord::reply`, `concord::reply_with` | `concord-api`, in every build. |
| `Script`, `Server` | `concord-testkit`, a dev-dependency. Its documentation will be on docs.rs once it is published; until then, run `cargo doc -p concord-testkit --open`. |
| `start_paused = true` | Tokio, with its `test-util` feature, on a current-thread runtime (the default flavor of `#[tokio::test]`). |

No test of Concord calls a real API: every test runs against a fake, a script or a local server.
