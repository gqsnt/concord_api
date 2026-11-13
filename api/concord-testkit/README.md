# concord-testkit

Tools for the tests of a Concord (the crate `concord`) client.

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

```toml
[dev-dependencies]
concord-testkit = "5.0.0-rc.1"
tokio = { version = "1", features = ["macros", "rt", "test-util"] }
```

```rust
use concord_testkit::Script;
use std::time::Duration;

#[tokio::test(start_paused = true)]
async fn a_silent_server_ends_the_call() {
    let script = Script::new();
    script.answer(200).after(Duration::from_secs(40)).body("[]");
    let tasks = Tasks::new().with_transport(script.clone());

    let error = tasks.list().await.unwrap_err();
    assert!(error.is_timeout()); // 30 seconds of silence, in virtual time
    assert_eq!(script.requests().len(), 1);
}
```

Minimum Rust version: 1.96.
