//! The two connected workflows end to end against loopback stand-ins, with the slice's required
//! failure: a UTF-8 character and a `data:` field split across chunks, then a disconnect that is
//! surfaced, with no reconnect and no replay.
//!
//! The stand-ins' access rules are this suite's (synthetic) choices, modelled on the documented
//! ones: the instance below disabled unauthenticated access to its public timeline (documented
//! as an instance setting, answered 401), and the streaming host requires a user token
//! (documented since 4.2.0).

mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::*;
use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use concord_sdk_mastodon::concord::bytes::Bytes;
use concord_sdk_mastodon::{AccessToken, StatusId, StreamErrorKind, StreamEvent, TimelineLimit};

/// An instance that disabled unauthenticated access to its public timeline: no token, 401 with
/// the documented document; the first status of the documented page exists, others are 404.
fn instance_with_restricted_timeline(request: &Recorded) -> Reply {
    if request.header("authorization") != Some(BEARER) {
        return json(401, fixture("error_unauthorized.json"));
    }
    match request.path() {
        "/api/v1/timelines/public" => json(200, fixture("public_timeline.json")),
        "/api/v1/statuses/103206804533200177" => json(200, fixture("status.json")),
        // Any other status (deleted since the page was read) or path.
        _ => json(404, fixture("error_not_found.json")),
    }
}

#[tokio::test]
async fn workflow_1_public_timeline_then_one_status() {
    let server = TestServer::start(instance_with_restricted_timeline).await;

    // The access rule is the instance's: anonymously, this one refuses.
    let anonymous = anonymous(&server);
    let refused = anonymous.public_timeline().send().await.unwrap_err();
    let api = refused.api().expect("401");
    assert!(api.is_unauthorized());
    assert_eq!(api.message(), Some("The access token is invalid"));

    // With the user's token: a page of two, then the first status of that page.
    let mastodon = anonymous.with_token(&AccessToken::new(TOKEN).unwrap());
    let page = mastodon
        .public_timeline()
        .set_limit(TimelineLimit::new(2).unwrap())
        .send()
        .await
        .expect("a page");
    let ids: Vec<&str> = page.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, ["103206804533200177", "103206804086086361"]);
    assert_eq!(page[1].account.acct, "synthetic_b@remote.example");
    assert!(page[1].url.is_none());

    let first = &page[0];
    let status = mastodon
        .status(first.id.clone())
        .send()
        .await
        .expect("the status");
    assert_eq!(status.id, first.id);
    assert_eq!(status.uri, first.uri);
    assert_eq!(status.created_at, "2019-11-26T23:27:31.000Z");
    assert_eq!(status.content, "<p>SYNTHETIC first status, café</p>");
    assert_eq!(status.account.username, "synthetic_a");

    // A status that is gone (deleted since the page was read): 404, the document kept.
    let gone = mastodon
        .status(StatusId::new("103206804086086361").unwrap())
        .send()
        .await
        .unwrap_err();
    assert!(gone.api().unwrap().is_not_found());
    assert_eq!(gone.api().unwrap().message(), Some("Record not found"));

    let requests = server.requests();
    let targets: Vec<&str> = requests.iter().map(|r| r.target.as_str()).collect();
    assert_eq!(
        targets,
        [
            "/api/v1/timelines/public",
            "/api/v1/timelines/public?limit=2",
            "/api/v1/statuses/103206804533200177",
            "/api/v1/statuses/103206804086086361",
        ]
    );
    assert!(authorization(&requests[0]).is_empty());
    for request in &requests[1..] {
        assert_eq!(authorization(request), [BEARER]);
        assert_eq!(request.header("accept"), Some("application/json"));
    }
}

/// The documented stream, cut inside the 🐘 of the update's content and inside its `data:`
/// field name, then the connection dropped in the middle of a fourth event.
fn stream_chunks() -> Vec<Bytes> {
    let input = fixture("stream_public_documented.txt");
    let text = std::str::from_utf8(&input).unwrap();
    let data = text.find("data: {").unwrap();
    let elephant = text.find('🐘').unwrap();
    let mut chunks: Vec<Bytes> = cut(&input, &[data + 3, elephant + 1, elephant + 3])
        .into_iter()
        .map(Bytes::from)
        .collect();
    chunks.push(Bytes::from_static(
        b":thump\n\nevent: update\ndata: {\"id\":\"1",
    ));
    chunks
}

#[tokio::test]
async fn workflow_2_discover_the_streaming_host_then_stream_the_public_timeline() {
    // The streaming host: a separate server, requiring the token.
    let stream_requests = Arc::new(Mutex::new(0usize));
    let counter = Arc::clone(&stream_requests);
    let streaming = TestServer::start(move |request| {
        *counter.lock().unwrap() += 1;
        if request.header("authorization") != Some(BEARER) {
            return json(401, r#"{"error":"SYNTHETIC token required"}"#);
        }
        if request.path() != "/api/v1/streaming/public" {
            return json(400, fixture("error_unknown_channel.json"));
        }
        sse_with(Body::FailAfter(stream_chunks()))
    })
    .await;

    // The instance advertises it as `ws://` (the instance document's streaming URL).
    let advertised = format!("ws://{}", streaming.addr());
    let instance_doc =
        fixture_text("instance.json").replace("wss://streaming.mastodon.example", &advertised);
    let rest = TestServer::start(move |request| match request.path() {
        "/api/v2/instance" => json(200, instance_doc.clone()),
        _ => json(404, fixture("error_not_found.json")),
    })
    .await;

    let mastodon = authorized(&rest);
    let host = mastodon
        .discover_streaming()
        .await
        .expect("a streaming host");
    assert_eq!(host.url(), format!("http://{}/", streaming.addr()));
    assert_eq!(host.instance(), mastodon.instance_url());
    assert!(!host.is_instance_host());

    let mut stream = mastodon
        .public_stream(&host)
        .expect("advertised by this instance")
        .send()
        .await
        .expect("the stream opens");

    let Some(Ok(StreamEvent::Update(update))) = stream.next().await else {
        panic!("an update first")
    };
    assert_eq!(update.id.as_str(), "108914354907984653");
    // The character split across two chunks arrives whole.
    assert_eq!(update.content, "<p>これは合成テキストです 🐘</p>");
    let Some(Ok(StreamEvent::Delete(deleted))) = stream.next().await else {
        panic!("then a delete")
    };
    assert_eq!(deleted.as_str(), "107214471804101576");
    let Some(Ok(StreamEvent::StatusUpdate(edited))) = stream.next().await else {
        panic!("then an edit")
    };
    assert_eq!(
        edited.edited_at.as_deref(),
        Some("2022-11-15T16:10:43.000Z")
    );

    // The connection drops inside the fourth event: surfaced, terminal, nothing invented.
    let lost = stream
        .next()
        .await
        .expect("the disconnect is surfaced")
        .unwrap_err();
    assert_eq!(lost.kind(), StreamErrorKind::Disconnected);
    assert!(lost.is_terminal());
    assert_eq!(lost.position().records, 3);
    assert!(lost.transport().is_some());
    assert!(stream.next().await.is_none());
    assert!(stream.is_finished());

    // One stream request (no reconnect), with the token; the instance saw only discovery.
    assert_eq!(*stream_requests.lock().unwrap(), 1);
    let request = streaming.single();
    assert_eq!(request.target, "/api/v1/streaming/public");
    assert_eq!(request.header("accept"), Some("text/event-stream"));
    assert_eq!(authorization(&request), [BEARER]);
    let discovery = rest.single();
    assert_eq!(discovery.target, "/api/v2/instance");
    assert_eq!(authorization(&discovery), [BEARER]);
}

#[tokio::test]
async fn a_stream_the_server_closes_is_surfaced_not_completed() {
    let server = TestServer::start(|_| sse(vec![frame("delete", "1"), b":thump\n".to_vec()])).await;
    let mastodon = authorized(&server);
    let host = concord_sdk_mastodon::StreamingHost::instance_host(mastodon.instance_url());
    let mut stream = mastodon.public_stream(&host).unwrap().send().await.unwrap();
    assert!(matches!(
        stream.next().await,
        Some(Ok(StreamEvent::Delete(_)))
    ));
    let closed = stream.next().await.unwrap().unwrap_err();
    assert_eq!(closed.kind(), StreamErrorKind::Closed);
    assert!(closed.is_terminal());
    assert!(stream.next().await.is_none());
    assert_eq!(server.requests().len(), 1);
}

#[tokio::test]
async fn heartbeats_let_a_caller_tell_a_quiet_stream_from_a_dead_one() {
    // Ten heartbeats 10 ms apart, then nothing at all while the connection stays open (the
    // generator keeps yielding empty chunks, which the server does not write).
    let server = TestServer::start(|_| {
        let mut sent = 0;
        sse_with(Body::Generate(
            Box::new(move || {
                sent += 1;
                Some(if sent <= 10 {
                    Bytes::from_static(b":thump\n")
                } else {
                    Bytes::new()
                })
            }),
            Some(Duration::from_millis(10)),
        ))
    })
    .await;
    let mastodon = authorized(&server);
    let host = concord_sdk_mastodon::StreamingHost::instance_host(mastodon.instance_url());
    let mut stream = mastodon.public_stream(&host).unwrap().send().await.unwrap();

    // The caller's idle rule: a window in which not even a heartbeat arrived means dead.
    let window = Duration::from_millis(400);
    let mut seen = stream.position().bytes;
    let mut quiet_windows_with_heartbeats = 0;
    let verdict = loop {
        match tokio::time::timeout(window, stream.next()).await {
            Ok(item) => panic!("no event was sent: {item:?}"),
            Err(_elapsed) => {
                let now = stream.position().bytes;
                if now == seen {
                    break "dead";
                }
                quiet_windows_with_heartbeats += 1;
                seen = now;
                assert!(
                    quiet_windows_with_heartbeats < 20,
                    "the heartbeats never stop"
                );
            }
        }
    };
    assert_eq!(verdict, "dead");
    assert!(quiet_windows_with_heartbeats >= 1);
    assert_eq!(stream.position().bytes, 70, "ten heartbeats, no event");
    assert_eq!(stream.position().records, 0);
}
