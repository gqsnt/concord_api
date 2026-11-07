//! Focused hashtag regressions. Every HTTP destination is a loopback fixture.

mod common;

use std::time::Duration;

use common::*;
use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use concord_sdk_mastodon::{
    DiscoveryError, Hashtag, PublicStream, StatusId, StreamErrorKind, StreamEvent, StreamingHost,
    TimelineLimit,
};

fn tag(name: &str) -> Hashtag {
    Hashtag::new(name).unwrap()
}

async fn open(mastodon: &concord_sdk_mastodon::Mastodon, local: bool) -> PublicStream {
    let host = StreamingHost::instance_host(mastodon.instance_url());
    if local {
        mastodon
            .local_hashtag_stream(&host, tag("café"))
            .unwrap()
            .send()
            .await
            .unwrap()
    } else {
        mastodon
            .hashtag_stream(&host, tag("café"))
            .unwrap()
            .send()
            .await
            .unwrap()
    }
}

#[tokio::test]
async fn invalid_hashtags_are_refused_before_any_request() {
    let server = TestServer::start(|_: &Recorded| -> Reply { json(200, "[]") }).await;
    let mastodon = authorized(&server);
    let host = StreamingHost::instance_host(mastodon.instance_url());
    for name in [
        "", "#cats", "ca#ts", " ", "two tags", "a\t", "a\n", "a\r", "a\0", "a\u{7f}", "a\u{a0}b",
        ".", "..",
    ] {
        // All three entry points require the same validated value, including extra tag lists.
        let timeline = Hashtag::new(name).map(|name| mastodon.hashtag_timeline(name));
        let stream = Hashtag::new(name).map(|name| mastodon.hashtag_stream(&host, name));
        let local = Hashtag::new(name).map(|name| mastodon.local_hashtag_stream(&host, name));
        assert!(timeline.is_err(), "{name:?}");
        assert!(stream.is_err(), "{name:?}");
        assert!(local.is_err(), "{name:?}");
    }
    assert!(
        !format!("{:?}", Hashtag::new("#private-input").unwrap_err()).contains("private-input")
    );
    assert!(
        !Hashtag::new("#private-input")
            .unwrap_err()
            .to_string()
            .contains("private-input")
    );
    for name in [
        "Rust",
        "café",
        "日本語",
        "Cafe\u{301}",
        "a_b",
        "a/b?c&d=%2F",
    ] {
        assert_eq!(Hashtag::try_from(name.to_owned()).unwrap().as_str(), name);
    }
    assert!(server.requests().is_empty());
    server.shutdown().await;
}

#[tokio::test]
async fn timeline_encodes_one_segment_and_repeats_bracketed_tag_filters() {
    let server =
        TestServer::start(|_: &Recorded| -> Reply { json(200, fixture("public_timeline.json")) })
            .await;
    let page = authorized(&server)
        .hashtag_timeline(tag("Café/été?x=1&y=%2F"))
        .set_any(vec![tag("rust"), tag("a&b")])
        .set_all(vec![tag("日本語"), tag("sdk")])
        .set_none(vec![tag("spoiler"), tag("c++")])
        .set_local(true)
        .set_remote(false)
        .set_only_media(true)
        .set_max_id(StatusId::new("9").unwrap())
        .set_since_id(StatusId::new("1").unwrap())
        .set_min_id(StatusId::new("2").unwrap())
        .set_limit(TimelineLimit::new(40).unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(page.len(), 2);
    let request = server.single();
    assert_eq!(request.method, "GET");
    assert_eq!(
        request.path(),
        "/api/v1/timelines/tag/Caf%C3%A9%2F%C3%A9t%C3%A9%3Fx%3D1%26y%3D%252F"
    );
    assert_eq!(request.query_all("any[]"), ["rust", "a&b"]);
    assert_eq!(request.query_all("all[]"), ["日本語", "sdk"]);
    assert_eq!(request.query_all("none[]"), ["spoiler", "c++"]);
    assert_eq!(
        request.query().unwrap(),
        "any%5B%5D=rust&any%5B%5D=a%26b&all%5B%5D=%E6%97%A5%E6%9C%AC%E8%AA%9E&all%5B%5D=sdk&none%5B%5D=spoiler&none%5B%5D=c%2B%2B&local=true&remote=false&only_media=true&max_id=9&since_id=1&min_id=2&limit=40"
    );
    assert_eq!(request.header("accept"), Some("application/json"));
    assert_eq!(authorization(&request), [BEARER]);
    assert!(request.body.is_empty());
    server.shutdown().await;
}

#[tokio::test]
async fn timeline_defaults_and_empty_extra_tags_are_omitted() {
    let server = TestServer::start(|_: &Recorded| -> Reply { json(200, "[]") }).await;
    let mastodon = anonymous(&server);
    assert!(
        mastodon
            .hashtag_timeline(tag("cats"))
            .send()
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        mastodon
            .hashtag_timeline(tag("cats"))
            .set_any(vec![])
            .set_all(vec![])
            .set_none(vec![])
            .send()
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(server.requests().len(), 2);
    for request in server.requests() {
        assert_eq!(request.target, "/api/v1/timelines/tag/cats");
        assert!(authorization(&request).is_empty());
    }
    server.shutdown().await;
}

#[tokio::test]
async fn hashtag_streams_encode_the_tag_and_keep_the_instance_binding() {
    let streaming =
        TestServer::start(|_: &Recorded| -> Reply { sse(vec![b":thump\n\n".to_vec()]) }).await;
    let instance = TestServer::start(|_: &Recorded| -> Reply { json(200, "[]") }).await;
    let mastodon = authorized(&instance);
    let host = StreamingHost::advertised_by(mastodon.instance_url(), &streaming.url()).unwrap();
    let mut all = mastodon
        .hashtag_stream(&host, tag("café&tag=other/%"))
        .unwrap()
        .send()
        .await
        .unwrap();
    let mut local = mastodon
        .local_hashtag_stream(&host, tag("café&tag=other/%"))
        .unwrap()
        .send()
        .await
        .unwrap();
    for stream in [&mut all, &mut local] {
        assert_eq!(
            stream.next().await.unwrap().unwrap_err().kind(),
            StreamErrorKind::Closed
        );
        assert!(stream.next().await.is_none());
    }
    let requests = streaming.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[0].target,
        "/api/v1/streaming/hashtag?tag=caf%C3%A9%26tag%3Dother%2F%25"
    );
    assert_eq!(
        requests[1].target,
        "/api/v1/streaming/hashtag/local?tag=caf%C3%A9%26tag%3Dother%2F%25"
    );
    for request in requests {
        assert_eq!(request.method, "GET");
        assert_eq!(request.query_all("tag"), ["café&tag=other/%"]);
        assert_eq!(request.header("accept"), Some("text/event-stream"));
        assert_eq!(authorization(&request), [BEARER]);
        assert_eq!(request.host, streaming.addr().to_string());
        assert!(request.body.is_empty());
    }
    let other = authorized(&streaming);
    for error in [
        other.hashtag_stream(&host, tag("cats")).unwrap_err(),
        other.local_hashtag_stream(&host, tag("cats")).unwrap_err(),
    ] {
        assert!(matches!(
            error.discovery(),
            Some(DiscoveryError::OtherInstance)
        ));
    }
    assert_eq!(
        streaming.requests().len(),
        2,
        "mismatched instances send nothing"
    );
    assert!(instance.requests().is_empty());
    instance.shutdown().await;
    streaming.shutdown().await;
}

#[tokio::test]
async fn hashtag_errors_keep_the_endpoint_and_provider_access_refusal() {
    let server = TestServer::start(|_: &Recorded| -> Reply {
        json(401, fixture("error_unauthorized.json"))
    })
    .await;
    let mastodon = anonymous(&server);
    let host = StreamingHost::instance_host(mastodon.instance_url());
    let errors = [
        (
            mastodon
                .hashtag_timeline(tag("cats"))
                .send()
                .await
                .unwrap_err(),
            "Api.hashtag_timeline",
        ),
        (
            mastodon
                .hashtag_stream(&host, tag("cats"))
                .unwrap()
                .send()
                .await
                .unwrap_err(),
            "Api.hashtag_stream",
        ),
        (
            mastodon
                .local_hashtag_stream(&host, tag("cats"))
                .unwrap()
                .send()
                .await
                .unwrap_err(),
            "Api.local_hashtag_stream",
        ),
    ];
    for (error, operation) in errors {
        let api = error.api().unwrap();
        assert!(api.is_unauthorized());
        assert_eq!(api.message(), Some("The access token is invalid"));
        assert_eq!(api.operation().unwrap().name(), operation);
    }
    assert_eq!(
        server.requests().len(),
        3,
        "access failures are not retried"
    );
    server.shutdown().await;
}

#[tokio::test]
async fn both_hashtag_streams_reassemble_fields_and_utf8_then_surface_disconnect_once() {
    for local in [false, true] {
        let mut bytes = b":thump\n\nevent: update\ndata: ".to_vec();
        bytes.extend(status_json("7", "<p>café 🐘</p>").into_bytes());
        bytes.extend(b"\n\nevent: future.kind\ndata: first\ndata: second\n\n");
        // Two cuts are inside `data:` and inside the four-byte elephant, respectively.
        let field_cut = bytes.windows(5).position(|s| s == b"data:").unwrap() + 2;
        let utf8_cut = bytes.windows(4).position(|s| s == "🐘".as_bytes()).unwrap() + 1;
        assert!(std::str::from_utf8(&bytes[..utf8_cut]).is_err());
        bytes.extend(b"event: update\ndata: {\"id\":\"lost-tail");
        let body_bytes = bytes.len();
        let chunks = cut(&bytes, &[field_cut, utf8_cut]);
        let server = TestServer::start(move |_: &Recorded| -> Reply {
            sse_with(Body::FailAfter(
                chunks.iter().cloned().map(Into::into).collect(),
            ))
        })
        .await;
        let mut stream = open(&authorized(&server), local).await;
        assert_eq!(stream.position().bytes, 0);
        let next = tokio::time::timeout(Duration::from_secs(1), stream.next())
            .await
            .unwrap();
        let Some(Ok(StreamEvent::Update(status))) = next else {
            panic!("expected a complete status: {next:?}")
        };
        assert_eq!(status.id.as_str(), "7");
        assert_eq!(status.content, "<p>café 🐘</p>");
        let Some(Ok(StreamEvent::Other { event, data })) = stream.next().await else {
            panic!("expected the multiline event")
        };
        assert_eq!(event, "future.kind");
        assert_eq!(data, "first\nsecond");
        let error = stream.next().await.unwrap().unwrap_err();
        assert_eq!(error.kind(), StreamErrorKind::Disconnected);
        assert!(error.is_terminal());
        assert_eq!(
            error.operation(),
            Some(if local {
                "Api.local_hashtag_stream"
            } else {
                "Api.hashtag_stream"
            })
        );
        assert_eq!(error.position().bytes, body_bytes as u64);
        assert!(error.transport().is_some());
        assert!(stream.is_finished());
        assert!(stream.next().await.is_none());
        assert!(stream.next().await.is_none());
        assert_eq!(
            server.requests().len(),
            1,
            "no reconnect, replay or invented tail event"
        );
        assert!(server.single().header("last-event-id").is_none());
        server.shutdown().await;
    }
}
