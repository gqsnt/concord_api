//! Consumer of the documented hashtag API, using only local fixtures and public SDK methods.

mod common;

use std::time::Duration;

use common::*;
use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use concord_sdk_mastodon::{Hashtag, StreamErrorKind, StreamEvent, TimelineLimit};
use serde_json::json;

#[tokio::test]
async fn read_hashtag_status_then_discover_and_follow_both_hashtag_streams() {
    let streaming = TestServer::start(|request: &Recorded| -> Reply {
        assert_eq!(authorization(request), [BEARER]);
        assert_eq!(request.header("accept"), Some("text/event-stream"));
        assert_eq!(request.query_all("tag"), ["café"]);
        let event = match request.path() {
            "/api/v1/streaming/hashtag" => "update",
            "/api/v1/streaming/hashtag/local" => "status.update",
            route => panic!("unexpected stream route: {route}"),
        };
        sse_with(Body::Chunks(vec![
            (
                Duration::ZERO,
                format!(":thump\n\nevent: {event}\nda").into_bytes().into(),
            ),
            (
                Duration::from_millis(5),
                format!("ta: {}\n\n", status_json("7", "<p>café</p>"))
                    .into_bytes()
                    .into(),
            ),
            (Duration::from_millis(5), frame("delete", "7").into()),
        ]))
    })
    .await;
    let streaming_url = streaming.url().replacen("http://", "ws://", 1);
    let instance = TestServer::start(move |request: &Recorded| -> Reply {
        assert_eq!(authorization(request), [BEARER]);
        match request.path() {
            "/api/v1/timelines/tag/caf%C3%A9" => {
                assert_eq!(request.query_all("any[]"), ["rust", "sdk"]);
                assert_eq!(request.query_all("all[]"), ["code"]);
                assert_eq!(request.query_all("none[]"), ["spoiler"]);
                json(200, format!("[{}]", status_json("7", "<p>café</p>")))
            }
            "/api/v1/statuses/7" => json(200, status_json("7", "<p>café</p>")),
            "/api/v2/instance" => json(
                200,
                json!({"configuration":{"urls":{"streaming":streaming_url}}}).to_string(),
            ),
            route => panic!("unexpected instance route: {route}"),
        }
    })
    .await;
    let mastodon = authorized(&instance);
    let tag = Hashtag::new("café").unwrap();
    let page = mastodon
        .hashtag_timeline(tag.clone())
        .set_any(vec![
            Hashtag::new("rust").unwrap(),
            Hashtag::new("sdk").unwrap(),
        ])
        .set_all(vec![Hashtag::new("code").unwrap()])
        .set_none(vec![Hashtag::new("spoiler").unwrap()])
        .set_local(true)
        .set_limit(TimelineLimit::new(2).unwrap())
        .send()
        .await
        .expect("hashtag page");
    assert_eq!(page.len(), 1);
    let status = mastodon.status(page[0].id.clone()).send().await.unwrap();
    assert_eq!(status.id, page[0].id);
    assert_eq!(status.content, "<p>café</p>");

    let host = mastodon.discover_streaming().await.unwrap();
    assert_eq!(host.instance(), mastodon.instance_url());
    assert_eq!(host.url(), format!("{}/", streaming.url()));
    assert!(
        streaming.requests().is_empty(),
        "discovery only reads the instance"
    );
    let mut all = mastodon
        .hashtag_stream(&host, tag.clone())
        .unwrap()
        .send()
        .await
        .unwrap();
    let mut local = mastodon
        .local_hashtag_stream(&host, tag)
        .unwrap()
        .send()
        .await
        .unwrap();
    assert!(
        matches!(all.next().await, Some(Ok(StreamEvent::Update(s))) if s.id == status.id && s.content == status.content)
    );
    assert!(
        matches!(local.next().await, Some(Ok(StreamEvent::StatusUpdate(s))) if s.id == status.id && s.content == status.content)
    );
    for stream in [&mut all, &mut local] {
        assert!(
            matches!(stream.next().await, Some(Ok(StreamEvent::Delete(id))) if id == status.id)
        );
        let end = stream.next().await.unwrap().unwrap_err();
        assert_eq!(end.kind(), StreamErrorKind::Closed);
        assert!(
            end.is_terminal(),
            "EOF is a disconnect, not successful completion"
        );
        assert!(stream.next().await.is_none());
    }
    assert_eq!(
        instance.requests().len(),
        3,
        "one timeline page, one status, one discovery"
    );
    assert_eq!(
        streaming.requests().len(),
        2,
        "one explicit open of each stream, no replay"
    );
    instance.shutdown().await;
    streaming.shutdown().await;
}
