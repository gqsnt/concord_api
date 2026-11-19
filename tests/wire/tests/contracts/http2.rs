//! HTTP/2 for real (C4.1, C4.4, C9.5): many calls on one connection, and a graceful shutdown
//! of the server while calls are in flight, in which hyper sends `GOAWAY` and finishes the
//! streams that it accepted. The same shutdown runs over HTTP/1.1, where hyper finishes the
//! request in flight and closes the connection.

use crate::server::{settle, Gate, Server, Wire};
use axum::extract::State;
use axum::routing::get;
use axum::Router;
use concord::error::{Class, Kind};
use concord::Text;
use std::future::IntoFuture;

concord::api! {
    client Slow {
        base "http://slow.test";
        read Text;
        GET held() -> String { path "held"; }
    }
}

/// `/held` answers `released` once the gate is open.
fn held(gate: &Gate) -> Router {
    Router::new()
        .route(
            "/held",
            get(|State(gate): State<Gate>| async move {
                gate.wait().await;
                "released"
            }),
        )
        .with_state(gate.clone())
}

#[tokio::test]
async fn c4_4_thirty_two_calls_at_once_share_one_http2_connection_and_each_is_sent_once() {
    let gate = Gate::default();
    let server = Server::start(held(&gate)).await;
    let slow = Slow::new()
        .with_transport(Wire::Two.http())
        .with_base(server.url());
    let calls: Vec<_> = (0..32)
        .map(|_| tokio::spawn(slow.held().into_future()))
        .collect();
    // Every call is on the server at once before any is answered.
    server.seen.until(32).await;
    gate.open();
    for call in calls {
        assert_eq!(call.await.unwrap().unwrap(), "released");
    }
    let seen = server.seen.all();
    assert_eq!(seen.len(), 32);
    assert!(seen
        .iter()
        .all(|arrival| arrival.version == Wire::Two.version()));
    assert!(
        seen.iter()
            .all(|arrival| arrival.connection == seen[0].connection),
        "the calls used more than one connection"
    );
}

async fn graceful_shutdown(wire: Wire) {
    let gate = Gate::default();
    let mut server = Server::start(held(&gate)).await;
    let slow = Slow::new()
        .with_transport(wire.http())
        .with_base(server.url());
    let calls: Vec<_> = (0..3)
        .map(|_| tokio::spawn(slow.held().into_future()))
        .collect();
    server.seen.until(3).await;

    // The server stops accepting and tells each connection to finish (HTTP/2: `GOAWAY`).
    server.shutdown();
    settle().await;
    gate.open();
    for call in calls {
        assert_eq!(call.await.unwrap().unwrap(), "released");
    }
    server.stopped().await;

    // The port is closed: a call with no retry policy makes one attempt, fails to connect,
    // and Concord's own client states that nothing was sent (C9.5).
    let error = slow.held().await.unwrap_err();
    assert_eq!(error.kind(), Kind::Transport);
    assert_eq!(error.class(), Some(Class::Connect));
    assert!(error.request_not_sent());
    assert_eq!(error.tries(), 1);
    // Nothing was sent again below the engine: three calls, three requests.
    assert_eq!(server.seen.len(), 3);
}

#[tokio::test]
async fn c4_1_a_graceful_shutdown_finishes_the_calls_in_flight_and_resends_nothing_over_http1() {
    graceful_shutdown(Wire::One).await;
}

#[tokio::test]
async fn c4_1_a_graceful_shutdown_finishes_the_calls_in_flight_and_resends_nothing_over_http2() {
    graceful_shutdown(Wire::Two).await;
}
