# concord-testkit

Tools for the tests of a client that Concord writes. Concord is the crate `concord-api`,
used as `concord` in code.

- `Server`: a real local HTTP/1.1 server that a test scripts. It records every request as it arrived.
- `Script`: a transport with virtual time, and no socket. Under `start_paused = true`, an hour takes no time.

For a plain answer, `concord::fake` is enough. Use this kit when a test depends on time
(a retry, a clock), or on the bytes that really went out.

Your tests depend on it, as a dev-dependency; your library never does. `start_paused` is
Tokio's `test-util` feature, and it runs on the current thread only (the flavor of
`#[tokio::test]` by default). `Script` works on a multi-thread runtime too
(`#[tokio::test(flavor = "multi_thread")]`, tasks that share one client): its clones share
one record behind a lock, the tries are played in the order the sends reach it, and its
waits (`after`, `pause`) are then real time, since the clock is not paused.

The crates are not on crates.io yet. Until they are, depend on the repository:

```toml
[dev-dependencies]
concord-testkit = { git = "https://github.com/gqsnt/concord_api" }
tokio = { version = "1", features = ["macros", "rt", "test-util"] }
```

After the first upload, the first line is `concord-testkit = "5.0.0-rc.1"`.

In this example, `Tasks` is a client that `concord::api!` wrote, with `GET get(id: u64) -> Task` (the client of [the tour](https://github.com/gqsnt/concord_api/blob/main/docs/guide/tour.md)).

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

A retry on virtual time, a local server, and what each method of `Script` and `Server` does: [testing a client](https://github.com/gqsnt/concord_api/blob/main/docs/guide/testing.md).

Minimum Rust version: 1.96.
