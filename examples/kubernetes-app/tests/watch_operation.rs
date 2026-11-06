//! One watch connection through the SDK's **generated** watch operation, called the way an
//! ordinary caller does (`watch_pods().set_resource_version(rv).send().await?`, then events), with
//! no workflow around it. The watch endpoints are declared `-> stream WatchEvent<T>`, so `send()`
//! returns the open reader as soon as the head is accepted.
//!
//! Fixtures: `tests/fixtures/ku_b/watch_pods_team_a_1.json-values`,
//! `watch_pods_team_a_2.json-values`, `watch_error_410.json-values`, `status_410_watch_open.json`,
//! `watch_deployments_team_b.json-values` (schema-derived synthetic, see `MANIFEST.json`).

mod support;

use std::time::Duration;

use concord::ErrorKind;
use concord::bytes::Bytes;
use concord::stream::{Completion, ReadErrorKind};
use concord_example_kubernetes_sdk::model::{Deployment, Pod};
use concord_example_kubernetes_sdk::watch::{WatchEvent, WatchOperation, WatchStream};
use concord_example_kubernetes_sdk::{KubeError, ResourceVersion};
use concord_example_testserver::{Body, Reply, TestServer};
use support::*;

const NAMESPACED_PODS: &str = "Kube.core_v1.namespaced.watch_pods";

fn rv(value: &str) -> ResourceVersion {
    ResourceVersion::new(value)
}

/// A `200` watch head with these chunks, then the normal end of the body.
fn events(chunks: Vec<Bytes>) -> Reply {
    Reply::new(200)
        .header("content-type", "application/json")
        .with_body(Body::Chunks(
            chunks.into_iter().map(|c| (Duration::ZERO, c)).collect(),
        ))
}

/// The bytes split into pieces of `size` (the last one shorter).
fn pieces(bytes: &[u8], size: usize) -> Vec<Bytes> {
    bytes.chunks(size).map(Bytes::copy_from_slice).collect()
}

/// The events of one connection, until the end of the body or the first read error.
async fn drain<T: concord_example_kubernetes_sdk::Resource>(
    stream: &mut WatchStream<T>,
) -> (Vec<WatchEvent<T>>, Option<concord::stream::ReadError>) {
    let mut seen = Vec::new();
    while let Some(event) = stream.next().await {
        match event {
            Ok(event) => seen.push(event),
            Err(error) => return (seen, Some(error)),
        }
    }
    (seen, None)
}

#[tokio::test]
async fn the_generated_send_returns_before_the_body_is_read() {
    // The head, then no byte of body until shutdown: a drained or collected output would never
    // return.
    let server = TestServer::start(|_| {
        Reply::new(200)
            .header("content-type", "application/json")
            .with_body(Body::Stall)
    })
    .await;
    let kube = kube(&server);
    let operation = kube
        .namespace(ns("team-a"))
        .core()
        .watch_pods()
        .set_resource_version(rv("8000"));

    // Control: the handling of an endpoint without output (any 2xx, body drained), which the
    // watch declarations had before, blocks on this body.
    let (client, request, _) = operation.clone().prepare().unwrap().into_parts();
    let drained = tokio::time::timeout(
        Duration::from_millis(200),
        concord::Decoder::drain().finish(client.execute(request).await),
    )
    .await;
    assert!(
        drained.is_err(),
        "a drained output waits for the end of the body"
    );

    let mut events = tokio::time::timeout(Duration::from_secs(5), operation.send())
        .await
        .expect("send() returned without reading the body")
        .unwrap();
    assert_eq!(events.position().records, 0);
    assert_eq!(events.position().bytes, 0);
    assert_eq!(events.completion(), Completion::Reading);
    // Reading is the caller's: the next event is pending on the stalled body.
    assert!(
        tokio::time::timeout(Duration::from_millis(100), events.next())
            .await
            .is_err()
    );
    assert_eq!(
        events.completion(),
        Completion::Reading,
        "a stall is not an end"
    );
    let requests = server.requests();
    assert_eq!(requests.len(), 2, "the control and the watch");
    for request in &requests {
        assert_eq!(
            request.target,
            "/api/v1/namespaces/team-a/pods?resourceVersion=8000&allowWatchBookmarks=true&watch=1"
        );
        assert_eq!(request.header("accept"), Some("application/json"));
    }
    assert_authenticated(&requests);
    drop(events);
    finish(server).await;
}

#[tokio::test]
async fn events_decode_across_chunk_splits_and_end_cleanly() {
    let bytes = fixture("ku_b/watch_pods_team_a_1.json-values");
    // Odd piece sizes put boundaries inside tokens, numbers and multibyte UTF-8 characters.
    for size in [1, 7, 61, bytes.len()] {
        let body = bytes.clone();
        let server = TestServer::start(move |_| events(pieces(&body, size))).await;
        let kube = kube(&server);
        let mut stream = kube
            .namespace(ns("team-a"))
            .core()
            .watch_pods()
            .set_resource_version(rv("8000"))
            .send()
            .await
            .unwrap();
        let (seen, error) = drain(&mut stream).await;
        assert!(error.is_none(), "piece size {size}: {error:?}");
        let types: Vec<_> = seen.iter().map(WatchEvent::type_name).collect();
        assert_eq!(
            types,
            ["MODIFIED", "ADDED", "BOOKMARK", "DELETED"],
            "{size}"
        );
        let versions: Vec<_> = seen
            .iter()
            .map(|e| e.resource_version().unwrap().as_str().to_owned())
            .collect();
        assert_eq!(versions, ["8001", "8002", "8003", "8004"], "{size}");
        assert_eq!(stream.completion(), Completion::EndOfInput, "{size}");
        assert_eq!(stream.position().records, 4);
        assert_eq!(stream.position().bytes, bytes.len() as u64);
        drop(stream);
        finish(server).await;
    }
}

#[tokio::test]
async fn a_410_answer_is_a_status_error_read_as_expired() {
    let server = TestServer::start(|_| status(410, "ku_b/status_410_watch_open.json")).await;
    let error = kube(&server)
        .namespace(ns("team-a"))
        .core()
        .watch_pods()
        .set_resource_version(rv("8000"))
        .send()
        .await
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Status);
    assert_eq!(error.status().map(|s| s.as_u16()), Some(410));
    assert_eq!(error.endpoint(), Some(NAMESPACED_PODS));
    // The response was kept unread: the SDK reads its `Status` document.
    let KubeError::Api(api) = KubeError::from_concord(error).await else {
        panic!("not an API error")
    };
    assert!(api.is_expired());
    assert_eq!(api.reason(), Some("Expired"));
    assert_eq!(api.status().and_then(|s| s.code), Some(410));
    assert_eq!(api.endpoint(), Some(NAMESPACED_PODS));
    finish(server).await;
}

#[tokio::test]
async fn a_410_error_event_is_the_last_event() {
    let server = TestServer::start(|_| {
        events(vec![
            fixture("ku_b/watch_pods_team_a_2.json-values").into(),
            fixture("ku_b/watch_error_410.json-values").into(),
        ])
    })
    .await;
    let mut stream = kube(&server)
        .namespace(ns("team-a"))
        .core()
        .watch_pods()
        .set_resource_version(rv("8004"))
        .send()
        .await
        .unwrap();
    let (seen, error) = drain(&mut stream).await;
    assert!(error.is_none(), "{error:?}");
    assert_eq!(seen.len(), 2);
    assert!(matches!(&seen[0], WatchEvent::Modified(_)));
    let WatchEvent::Error(status) = &seen[1] else {
        panic!("{:?}", seen[1])
    };
    assert!(status.is_expired());
    assert_eq!(status.code, Some(410));
    assert_eq!(status.reason.as_deref(), Some("Expired"));
    assert_eq!(stream.completion(), Completion::EndOfInput);
    drop(stream);
    finish(server).await;
}

#[tokio::test]
async fn a_refused_head_is_metadata_and_kept_unread() {
    // An authenticating proxy's login page instead of the watch.
    let server =
        TestServer::start(|_| Reply::text(200, "text/html", "<html>proxy login</html>")).await;
    let error = kube(&server).core().watch_pods().send().await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Metadata);
    assert_eq!(error.endpoint(), Some("Kube.core_v1.watch_pods"));
    let body = error.into_response().unwrap().text().await.unwrap();
    assert_eq!(body, "<html>proxy login</html>");
    finish(server).await;
}

#[tokio::test]
async fn a_mid_stream_read_error_names_the_declared_endpoint() {
    let good = fixture("ku_b/watch_deployments_team_b.json-values");
    let first_line = good
        .split_inclusive(|b| *b == b'\n')
        .next()
        .unwrap()
        .to_vec();
    let server = TestServer::start(move |r| {
        let first = Bytes::copy_from_slice(&first_line);
        if r.query_all("resourceVersion") == ["1"] {
            // One event, then an event that is not JSON.
            events(vec![
                first,
                Bytes::from_static(b"{\"type\":\"ADDED\",\"object\":}\n"),
            ])
        } else {
            // One event, then the connection is lost mid-body.
            Reply::new(200)
                .header("content-type", "application/json")
                .with_body(Body::FailAfter(vec![
                    first,
                    Bytes::from_static(b"{\"type\":"),
                ]))
        }
    })
    .await;
    let kube = kube(&server);
    let operation = kube.namespace(ns("team-b")).apps().watch_deployments();
    // The name the declaration gives the operation (no SDK string is involved).
    let declared = operation
        .clone()
        .prepare_watch()
        .unwrap()
        .into_parts()
        .2
        .operation()
        .map(|op| op.name());
    assert_eq!(declared, Some("Kube.apps_v1.namespaced.watch_deployments"));

    for (from, kind) in [
        ("1", ReadErrorKind::Format),
        ("2", ReadErrorKind::Transport),
    ] {
        let mut stream: WatchStream<Deployment> = operation
            .clone()
            .set_resource_version(rv(from))
            .send()
            .await
            .unwrap();
        let (seen, error) = drain(&mut stream).await;
        assert_eq!(seen.len(), 1, "{from}");
        let error = error.expect("a read error");
        assert_eq!(error.kind(), kind);
        assert!(error.is_terminal());
        assert_eq!(error.operation(), declared, "{from}");
        assert_eq!(error.position().records, 1);
        assert!(
            error
                .to_string()
                .starts_with("Kube.apps_v1.namespaced.watch_deployments")
        );
        assert_eq!(
            stream.completion(),
            Completion::Failed,
            "an error is not an end"
        );
        assert!(stream.next().await.is_none(), "fused");
    }
    finish(server).await;
}

#[tokio::test]
async fn dropping_the_stream_early_is_not_end_of_input() {
    // One event, then whitespace every 10 ms forever: more may always follow.
    let server = TestServer::start(|_| {
        let mut first = Some(Bytes::from(fixture("ku_b/watch_pods_team_a_2.json-values")));
        let next = Box::new(move || Some(first.take().unwrap_or(Bytes::from_static(b"\n"))));
        Reply::new(200)
            .header("content-type", "application/json")
            .with_body(Body::Generate(next, Some(Duration::from_millis(10))))
    })
    .await;
    let mut stream: WatchStream<Pod> = kube(&server)
        .namespace(ns("team-a"))
        .core()
        .watch_pods()
        .set_resource_version(rv("8004"))
        .send()
        .await
        .unwrap();
    let first = stream.next().await.unwrap().unwrap();
    assert_eq!(first.resource_version(), Some(&rv("8005")));
    assert_eq!(
        stream.completion(),
        Completion::Reading,
        "stopping after an event proves nothing about the rest of the watch"
    );
    assert_eq!(server.active_bodies(), 1);
    drop(stream);
    // Dropping the stream closed the connection: the server's next heartbeat write fails.
    assert!(
        server
            .wait_until(Duration::from_secs(5), |s| s.active_bodies() == 0
                && s.live_connections() == 0)
            .await
    );
    finish(server).await;
}
