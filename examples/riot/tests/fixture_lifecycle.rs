//! A control for the loopback fixture's resource lifecycle (not a provider wire test).
//!
//! CI job 126834420624 (native Windows) failed `optional_tournament_fields_are_omitted_not_null`
//! when the fixture's `bind("127.0.0.1:0")` returned WSAENOBUFS (10055), before any request. This
//! test repeats that fixture lifecycle — bind an ephemeral loopback listener, serve kept-alive
//! requests from a reqwest client, drop the client, drop the server — and checks every cycle,
//! within bounded waits, that the server's connection tasks ended after the client closed and
//! that its accept task (which owns the listener) ended after the server was dropped. A fixture
//! that kept connection tasks or listeners alive across tests would fail here; the gate's socket
//! snapshots (`scripts/socket-snapshot.py`) record the machine-wide counters around this
//! package. It does not, and cannot, reproduce a runner-side resource failure.

mod support;

use std::time::{Duration, Instant};

use support::{Reply, Server};

/// Cycles per run: an order of magnitude more fixture servers than the failing test binary
/// started (nine), while one listener and one or two client ports per cycle stay far below the
/// free dynamic ports measured on the CI runners.
const CYCLES: usize = 100;

/// Waits (bounded) until `done()` holds.
async fn wait(what: &str, cycle: usize, done: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !done() {
        assert!(Instant::now() < deadline, "cycle {cycle}: {what}");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn repeated_fixture_cycles_release_their_connections_and_listener() {
    for cycle in 0..CYCLES {
        let server = Server::reply(Reply::json(200, "{\"ok\":true}")).await;
        let probe = server.probe();
        let http = concord::reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        for _ in 0..2 {
            let response = http.get(server.url()).send().await.unwrap();
            assert_eq!(response.status(), 200);
            response.bytes().await.unwrap();
        }
        // Usually one kept-alive connection; returning it to reqwest's pool races the second
        // checkout, so a second connection is legitimate (not a leak).
        assert!((1..=2).contains(&server.connections()), "cycle {cycle}");
        drop(http);
        wait("a connection task outlived its client", cycle, || {
            probe.live_connections() == 0
        })
        .await;
        assert_eq!(probe.accepting(), 1, "cycle {cycle}");
        drop(server);
        wait(
            "the accept task (listener) outlived the server",
            cycle,
            || probe.accepting() == 0,
        )
        .await;
    }
}
