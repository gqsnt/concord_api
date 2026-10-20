#![cfg(feature = "sse")]

use std::time::Duration;

use concord::http::SafeProxy;
use concord::records::{EventSource, EventStream, ServerEvent};
use concord::response::StreamErrorKind;
use concord::{ItemStream, api};
use concord_test_support::server::{ResponseScript, ScriptedServer};

api! {
    client Feed {
        base "http://events.example.test/";
        GET live() -> ItemStream<ServerEvent> via EventStream;
        GET replay() -> Vec<ServerEvent> via EventStream;
        GET resume(header("Last-Event-ID") last_event_id?: String)
            -> ItemStream<ServerEvent> via EventStream;
    }
}

fn response(body: impl Into<Vec<u8>>) -> ResponseScript {
    ResponseScript::ok(body).with_header("Content-Type", "text/event-stream")
}

fn client(server: &ScriptedServer) -> Feed {
    let proxy = SafeProxy::parse(server.url("")).unwrap();
    Feed::builder()
        .configure_http(move |http| http.proxy(proxy))
        .build()
        .unwrap()
}

fn source(feed: Feed) -> EventSource {
    EventSource::new(move |id| {
        let feed = feed.clone();
        async move { feed.resume().set_last_event_id_if_some(id).await }
    })
    .with_reconnect_time(Duration::ZERO)
}

#[tokio::test]
async fn generated_buffered_and_streamed_events_match_across_every_wire_split() {
    let body = b"\xef\xbb\xbf:keepalive\r\nid: first\revent: update\ndata: h\xc3\xa9\r\ndata: two\r\nretry: 17\r\n\r\ndata\n\nid:\ndata: cleared\n\ndata: discarded";
    let server = ScriptedServer::start([response(body.to_vec())]).unwrap();
    let expected = client(&server).replay().await.unwrap();
    assert_eq!(expected.len(), 3);
    assert_eq!(expected[0].event(), "update");
    assert_eq!(expected[0].data(), "hé\ntwo");
    assert_eq!(expected[0].last_event_id(), "first");
    assert_eq!(expected[0].retry(), Some(Duration::from_millis(17)));
    assert_eq!(expected[1].event(), "message");
    assert_eq!(expected[1].data(), "");
    assert_eq!(expected[1].last_event_id(), "first");
    assert_eq!(expected[2].last_event_id(), "");
    for split in 0..=body.len() {
        server.push(ResponseScript::Chunked {
            status: 200,
            headers: vec![(
                "Content-Type".into(),
                "TEXT/Event-Stream; charset=UTF-8".into(),
            )],
            chunks: [body[..split].to_vec(), body[split..].to_vec()]
                .into_iter()
                .filter(|chunk| !chunk.is_empty())
                .collect(),
        });
        let mut stream = client(&server).live().await.unwrap();
        let mut actual = Vec::new();
        while let Some(event) = stream.next().await {
            actual.push(event.unwrap());
        }
        assert_eq!(actual, expected, "wire split {split}");
    }
    assert!(
        server
            .requests()
            .iter()
            .all(|r| r.header("accept") == Some("text/event-stream"))
    );
}

#[tokio::test]
async fn media_type_is_required_except_for_no_content() {
    for media in [
        None,
        Some("application/json"),
        Some("text/event-stream-invalid"),
    ] {
        let mut script = ResponseScript::ok(b"data: secret\n\n".to_vec());
        if let Some(media) = media {
            script = script.with_header("Content-Type", media);
        }
        let server = ScriptedServer::start([script.clone(), script]).unwrap();
        let feed = client(&server);
        let error = feed.replay().await.unwrap_err();
        assert_eq!(
            error.category(),
            concord::error::ErrorCategory::ResponseContract
        );
        assert!(!format!("{error} {error:?}").contains("secret"));
        assert!(feed.live().await.is_err());
    }
    let server = ScriptedServer::start([
        ResponseScript::status(204, vec![]),
        ResponseScript::status(204, vec![]).with_header("Content-Type", "application/json"),
        ResponseScript::status(204, vec![]),
    ])
    .unwrap();
    let feed = client(&server);
    assert!(feed.replay().await.unwrap().is_empty());
    assert!(feed.live().await.unwrap().next().await.is_none());
    let mut source = source(feed);
    assert!(source.next().await.is_none());
    assert_eq!(source.connections(), 1);
}

#[tokio::test]
async fn source_carries_ids_and_retry_even_from_blocks_without_events_and_stops_on_204() {
    let server = ScriptedServer::start([
        response(b"id: first\nretry: 1\n\ndata: one\n\n".to_vec()),
        response(b"data: two\n\nid:\nretry: 0\n\n".to_vec()),
        response(b"data: three\n\n".to_vec()),
        ResponseScript::status(204, vec![]),
    ])
    .unwrap();
    let mut source = source(client(&server)).with_last_event_id("initial");
    let one = source.next().await.unwrap().unwrap();
    assert_eq!((one.data(), one.last_event_id()), ("one", "first"));
    assert_eq!(source.reconnect_time(), Duration::from_millis(1));
    let two = source.next().await.unwrap().unwrap();
    assert_eq!((two.data(), two.last_event_id()), ("two", "first"));
    assert_eq!(two.retry(), Some(Duration::from_millis(1)));
    let three = source.next().await.unwrap().unwrap();
    assert_eq!((three.data(), three.last_event_id()), ("three", ""));
    assert_eq!(three.retry(), Some(Duration::ZERO));
    assert!(source.next().await.is_none());
    assert!(source.next().await.is_none());
    assert_eq!(source.connections(), 4);
    let requests = server.requests();
    assert_eq!(
        requests
            .iter()
            .map(|r| r.header("last-event-id"))
            .collect::<Vec<_>>(),
        [Some("initial"), Some("first"), None, None]
    );
}

#[tokio::test]
async fn source_suppresses_transport_drops_but_yields_status_media_and_limit_failures_once() {
    let prefix = b"id: kept\nretry: 0\ndata: before\n\ndata: partial".to_vec();
    let server = ScriptedServer::start([
        ResponseScript::TruncatedWithHeaders {
            status: 200,
            declared_length: prefix.len() + 50,
            prefix,
            headers: vec![("Content-Type".into(), "text/event-stream".into())],
        },
        response(b"data: after\n\n".to_vec()),
        ResponseScript::status(204, vec![]),
    ])
    .unwrap();
    let mut events = source(client(&server));
    assert_eq!(events.next().await.unwrap().unwrap().data(), "before");
    let after = events.next().await.unwrap().unwrap();
    assert_eq!((after.data(), after.last_event_id()), ("after", "kept"));
    assert!(events.next().await.is_none());
    assert_eq!(events.connections(), 3);

    for script in [
        ResponseScript::status(403, b"private status".to_vec()),
        ResponseScript::ok(b"private media".to_vec()),
        response(format!("data: {}\n\n", "x".repeat(1_048_577)).into_bytes()),
    ] {
        let server = ScriptedServer::start([script]).unwrap();
        let mut events = source(client(&server));
        let error = events.next().await.unwrap().unwrap_err();
        assert!(!format!("{error} {error:?}").contains("private"));
        assert!(events.next().await.is_none());
        assert_eq!(events.connections(), 1);
    }
}

#[tokio::test]
async fn maximum_empty_reconnections_and_event_reset_are_exact() {
    let server = ScriptedServer::start([
        response(vec![]),
        response(vec![]),
        response(b"data: reset\n\n".to_vec()),
        response(vec![]),
        response(vec![]),
        response(b"data: unreachable\n\n".to_vec()),
    ])
    .unwrap();
    let mut events = source(client(&server)).with_max_reconnects(2);
    assert_eq!(events.next().await.unwrap().unwrap().data(), "reset");
    assert!(events.next().await.is_none());
    assert_eq!(events.connections(), 5);

    let server = ScriptedServer::start([
        response(vec![]),
        response(b"data: unreachable\n\n".to_vec()),
    ])
    .unwrap();
    let mut events = source(client(&server)).with_max_reconnects(0);
    assert!(events.next().await.is_none());
    assert_eq!(events.connections(), 1);
}

#[tokio::test]
async fn comments_do_not_accumulate_but_multiline_event_data_obeys_runtime_frame_limit() {
    let mut body = b": ping\nignored\n\n".repeat(500);
    body.extend_from_slice(b"data: ok\n\n");
    let server = ScriptedServer::start([
        response(body),
        response(b"data: abcdefgh\ndata: ijklmnop\ndata: qrstuvwx\n\n".to_vec()),
    ])
    .unwrap();
    let proxy = SafeProxy::parse(server.url("")).unwrap();
    let feed = Feed::builder()
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(|runtime| runtime.stream_frame_limit(24))
        .build()
        .unwrap();
    let mut stream = feed.live().await.unwrap();
    assert_eq!(stream.next().await.unwrap().unwrap().data(), "ok");
    assert!(stream.next().await.is_none());
    let mut stream = feed.live().await.unwrap();
    let error = stream.next().await.unwrap().unwrap_err();
    assert_eq!(
        error.stream_summary().unwrap().error_kind(),
        Some(StreamErrorKind::FrameLimit)
    );
    assert!(stream.next().await.is_none());
}

#[tokio::test]
async fn source_and_pending_next_are_send() {
    fn assert_send<T: Send>(_: T) {}
    let server = ScriptedServer::start([ResponseScript::status(204, vec![])]).unwrap();
    let mut events = source(client(&server));
    assert_send(events.next());
    assert_send(events);
}

#[tokio::test]
async fn replacement_expansion_does_not_charge_later_lines_to_the_current_frame() {
    let body = b"data: \xff\xff\xff\xff\xff\xff\xff\n\n:keepalive\n\ndata: ok\n\n";
    for chunks in [
        vec![body.to_vec()],
        body.iter().map(|byte| vec![*byte]).collect(),
    ] {
        let server = ScriptedServer::start([ResponseScript::Chunked {
            status: 200,
            headers: vec![("Content-Type".into(), "text/event-stream".into())],
            chunks,
        }])
        .unwrap();
        let proxy = SafeProxy::parse(server.url("")).unwrap();
        let feed = Feed::builder()
            .configure_http(move |http| http.proxy(proxy))
            .configure_runtime(|runtime| runtime.stream_frame_limit(24))
            .build()
            .unwrap();
        let mut stream = feed.live().await.unwrap();
        assert_eq!(stream.next().await.unwrap().unwrap().data(), "�".repeat(7));
        assert_eq!(stream.next().await.unwrap().unwrap().data(), "ok");
        assert!(stream.next().await.is_none());
    }
}

#[tokio::test]
async fn exact_event_data_ceiling_passes_both_managed_drivers_and_first_excess_fails() {
    let half = "x".repeat(524_288);
    let exact = format!("data: {half}\ndata: {}\n\n", &half[..half.len() - 1]).into_bytes();
    let excess = format!("data: {half}\ndata: {half}\n\n").into_bytes();
    let server = ScriptedServer::start([
        response(exact.clone()),
        response(exact),
        response(excess.clone()),
        response(excess),
    ])
    .unwrap();
    let feed = client(&server);
    let collected = feed.replay().await.unwrap();
    assert_eq!(collected[0].data().len(), 1_048_576);
    let mut stream = feed.live().await.unwrap();
    assert_eq!(
        stream.next().await.unwrap().unwrap().data().len(),
        1_048_576
    );
    assert!(stream.next().await.is_none());
    assert!(feed.replay().await.is_err());
    let mut stream = feed.live().await.unwrap();
    let error = stream.next().await.unwrap().unwrap_err();
    assert_eq!(
        error.stream_summary().unwrap().error_kind(),
        Some(StreamErrorKind::FrameLimit)
    );
    assert!(stream.next().await.is_none());
}
