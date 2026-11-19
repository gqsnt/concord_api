//! HTTP/2 for real (C4.1, C4.4, C7.1, C9.5): many calls on one connection; a graceful shutdown
//! of the server while calls are in flight, in which hyper sends `GOAWAY` and finishes the
//! streams that it accepted (over HTTP/1.1, hyper finishes the request in flight and closes the
//! connection); and a stream that the shutdown refuses, which `concord::Http` does not send
//! again, while a `reqwest::Client` of the caller's own does.

use crate::server::{settle, Gate, Link, Seen, Server, Wire};
use axum::extract::State;
use axum::routing::get;
use axum::Router;
use concord::error::{Class, Kind};
use concord::Text;
use std::future::{Future, IntoFuture};
use std::sync::{Arc, Mutex};
use std::task::Poll;

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

    // The server stops accepting and tells each connection to finish (HTTP/2: `GOAWAY`). It
    // refuses none of these streams: it had accepted each of them.
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
    assert_eq!(server.seen.len(), 3);
}

#[tokio::test]
async fn c9_5_a_graceful_shutdown_finishes_the_calls_in_flight_and_the_next_is_not_sent_over_http1()
{
    graceful_shutdown(Wire::One).await;
}

#[tokio::test]
async fn c9_5_a_graceful_shutdown_finishes_the_calls_in_flight_and_the_next_is_not_sent_over_http2()
{
    graceful_shutdown(Wire::Two).await;
}

// A stream that the shutdown refuses -------------------------------------------------------

/// Whether the server wrote a `PING` that is not an acknowledgement: in a graceful shutdown,
/// hyper writes `GOAWAY` with the highest stream id and this `PING`, and on its
/// acknowledgement a second `GOAWAY` with the last stream that it accepted.
fn wrote_a_ping(log: &[u8]) -> bool {
    let mut frames = log;
    while frames.len() >= 9 {
        let length = u32::from_be_bytes([0, frames[0], frames[1], frames[2]]) as usize;
        let (kind, flags) = (frames[3], frames[4]);
        if kind == 0x6 && flags & 0x1 == 0 {
            return true;
        }
        match frames.get(9 + length..) {
            Some(rest) => frames = rest,
            None => return false,
        }
    }
    false
}

/// What the call made during a shutdown returned, and what each of the two servers saw.
struct Refused {
    outcome: Result<String, slow::Error>,
    first: usize,
    second: usize,
}

/// A call whose stream the first server refuses during its graceful shutdown, and a second
/// server on the port that the first let go, which counts what is sent again.
///
/// The servers run on a thread and a runtime of their own. The test thread polls the second
/// call once, so that its request waits in the connection's queue, then blocks until the
/// server has written `GOAWAY` and `PING` and the second server listens: when the connection
/// runs again, it acknowledges the `PING` before it writes the request, and the server answers
/// with the `GOAWAY` that leaves the new stream out. The client's runtime polls the sockets
/// after every task (`event_interval(1)`).
fn refused_during_a_shutdown(own_client: bool) -> Refused {
    let gate = Gate::default();
    let (ready, started) = std::sync::mpsc::channel::<(String, Seen, Arc<Mutex<Vec<Arc<Link>>>>)>();
    let (listening, second_started) = std::sync::mpsc::channel::<Seen>();
    // Each message tells the first server to shut down; the end of the channel stops both.
    let (shutdown, mut shutdowns) = tokio::sync::mpsc::unbounded_channel::<()>();
    let held_gate = gate.clone();
    let servers = std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async move {
            let mut first = Server::start(held(&held_gate)).await;
            ready
                .send((first.url(), first.seen.clone(), first.links.clone()))
                .unwrap();
            let mut second = None;
            while shutdowns.recv().await.is_some() {
                first.shutdown();
                // The first server lets its port go; the second takes it.
                while second.is_none() {
                    tokio::task::yield_now().await;
                    let counting = Router::new().route("/held", get(|| async { "second server" }));
                    if let Ok(server) = Server::start_at(first.addr(), counting).await {
                        listening.send(server.seen.clone()).unwrap();
                        second = Some(server);
                    }
                }
            }
            drop(second);
        });
    });
    let (url, first_seen, links) = started.recv().unwrap();

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .event_interval(1)
        .build()
        .unwrap();
    let (outcome, second_seen) = runtime.block_on(async {
        let slow = if own_client {
            let own = concord::reqwest::Client::builder()
                .no_proxy()
                .http2_prior_knowledge()
                .build()
                .unwrap();
            Slow::new().with_transport(own).with_base(url)
        } else {
            Slow::new().with_transport(Wire::Two.http()).with_base(url)
        };
        let first = tokio::spawn(slow.held().into_future());
        first_seen.until(1).await;
        settle().await;

        let mut second = Box::pin(slow.held().into_future());
        std::future::poll_fn(|cx| {
            assert!(second.as_mut().poll(cx).is_pending());
            Poll::Ready(())
        })
        .await;
        shutdown
            .send(())
            .unwrap_or_else(|_| panic!("the servers stopped"));
        let link = links.lock().unwrap()[0].clone();
        while !wrote_a_ping(&link.log()) {
            std::thread::yield_now();
        }
        let second_seen = second_started.recv().unwrap();

        let outcome = second.await;
        gate.open();
        assert_eq!(first.await.unwrap().unwrap(), "released");
        (outcome, second_seen)
    });
    let refused = Refused {
        outcome,
        first: first_seen.len(),
        second: second_seen.len(),
    };
    drop(shutdown);
    servers.join().unwrap();
    refused
}

#[test]
fn c4_1_c7_1_a_stream_refused_by_goaway_is_not_sent_again_by_concords_http() {
    let refused = refused_during_a_shutdown(false);
    let error = refused.outcome.unwrap_err();
    // With no retry policy, one attempt: the refusal is a Connect error (C4.1), and nothing
    // below the engine sends the request again (C7.1): the second server sees nothing.
    assert_eq!(error.kind(), Kind::Transport);
    assert_eq!(error.class(), Some(Class::Connect));
    assert_eq!(error.tries(), 1);
    assert!(!error.request_not_sent());
    assert_eq!((refused.first, refused.second), (1, 0));
}

#[test]
fn c4_4_a_callers_own_reqwest_client_sends_a_stream_refused_by_goaway_again() {
    let refused = refused_during_a_shutdown(true);
    // `reqwest`'s default policy sends it again, below the engine's count (C4.4).
    assert_eq!(refused.outcome.unwrap(), "second server");
    assert_eq!((refused.first, refused.second), (1, 1));
}
