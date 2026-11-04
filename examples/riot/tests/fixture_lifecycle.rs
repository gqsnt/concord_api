//! A control for the loopback fixture's resource lifecycle (not a provider wire test).
//!
//! CI job 126834420624 (native Windows) failed `optional_tournament_fields_are_omitted_not_null`
//! when the fixture's `bind("127.0.0.1:0")` returned WSAENOBUFS (10055), before any request. This
//! test repeats exactly that fixture lifecycle — bind an ephemeral loopback listener, serve
//! kept-alive requests from a reqwest client, drop the client and the server — and checks every
//! cycle that the server side released its connection before the next bind. A fixture that leaked
//! connections or listeners across tests would accumulate sockets here and fail the bounded wait;
//! the gate's socket snapshots (`scripts/socket-snapshot.py`) record the machine-wide counters
//! around this package. It does not, and cannot, reproduce a runner-side resource failure.

mod support;

use std::time::{Duration, Instant};

use support::{Reply, Server};

/// Cycles per run: enough to exceed the failing test binary's nine fixture servers by an order
/// of magnitude, small enough to keep the dynamic-port footprint (one listener and one client
/// port per cycle) far below the runner's measured free range.
const CYCLES: usize = 100;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn repeated_fixture_cycles_release_their_connections() {
    for cycle in 0..CYCLES {
        let server = Server::reply(Reply::json(200, "{\"ok\":true}")).await;
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
        // The two requests reused one kept-alive connection.
        assert_eq!(server.connections(), 1, "cycle {cycle}");
        drop(http);
        let deadline = Instant::now() + Duration::from_secs(5);
        while server.live_connections() != 0 {
            assert!(
                Instant::now() < deadline,
                "cycle {cycle}: the server still holds {} connection(s) after its client closed",
                server.live_connections()
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        drop(server);
    }
}
