//! Fresh-consumer application tests, written from public documentation only.
//! All HTTP destinations are loopback fixtures; provider calls are NOT_RUN.

use std::time::Duration;

use concord_example_testserver::{Body, Recorded, Reply, Script, TestServer};
use concord_sdk_mastodon::concord::{ErrorKind, reqwest};
use concord_sdk_mastodon::{
    AccessToken, DiscoveryError, InstanceUrl, Mastodon, PublicStream, StatusId, StreamError,
    StreamErrorKind, StreamEvent, StreamingHost, TimelineLimit, Visibility,
};
use serde_json::{Value, json};

const TOKEN: &str = "synthetic-first-consumer-token-not-real";
const NEWEST: &str = "113441981234567890";
const OLDER: &str = "113441981234567889";
const MISSING: &str = "113441981234567888";
const BROKEN_JSON: &str = "113441981234567887";
const BROKEN_SHAPE: &str = "113441981234567886";
const WRONG_MEDIA: &str = "113441981234567885";
const PROXY_FAILURE: &str = "113441981234567884";

fn status_document(id: &str, content: &str) -> Value {
    json!({
        "id": id,
        "uri": format!("https://social.example/users/alice/statuses/{id}"),
        "url": format!("https://social.example/@alice/{id}"),
        "created_at": "2026-11-07T09:41:30.000Z",
        "edited_at": null,
        "account": {
            "id": "108892712797543112", "username": "alice", "acct": "alice",
            "display_name": "Alice 🌻", "url": "https://social.example/@alice",
            "locked": false, "bot": false, "emojis": [], "fields": []
        },
        "content": content, "visibility": "public", "sensitive": false,
        "spoiler_text": "", "language": "fr", "in_reply_to_id": null,
        "in_reply_to_account_id": null, "reblog": null,
        "replies_count": 1, "reblogs_count": 2, "favourites_count": 3,
        "media_attachments": [{
            "id": "113441900000000001", "type": "image",
            "url": "https://media.example/sunflower.jpg",
            "preview_url": "https://media.example/sunflower-small.jpg",
            "description": "A sunflower in the rain", "meta": {}
        }],
        "mentions": [], "tags": [], "emojis": [], "card": null, "poll": null
    })
}

fn client(origin: &str) -> Mastodon {
    let http = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(3))
        .build()
        .expect("local HTTP client");
    Mastodon::new(http, InstanceUrl::parse(origin).expect("loopback origin"))
}

fn authenticated(origin: &str) -> Mastodon {
    client(origin).with_token(&AccessToken::new(TOKEN).expect("synthetic bearer token"))
}

fn assert_wire(request: &Recorded, host: &str, accept: &str, authorized: bool) {
    assert_eq!(request.method, "GET");
    assert_eq!(request.host, host);
    assert!(request.body.is_empty());
    assert_eq!(request.body_len, 0);
    assert!(!request.chunked);
    assert_eq!(request.header("accept"), Some(accept));
    let bearer = format!("Bearer {TOKEN}");
    assert_eq!(
        request.header("authorization"),
        authorized.then_some(bearer.as_str())
    );
    assert!(request.query_all("access_token").is_empty());
    assert!(request.header("last-event-id").is_none());
}

fn assert_redacted(rendered: &str, origin: &str) {
    assert!(!rendered.contains(TOKEN), "token leaked in diagnostics");
    assert!(!rendered.contains(origin), "origin leaked in diagnostics");
}

#[tokio::test]
async fn public_timeline_then_status_as_a_fresh_consumer() {
    let server = TestServer::start(|request: &Recorded| -> Reply {
        match request.path() {
            "/api/v1/timelines/public" => {
                if request.header("authorization").is_none() {
                    return Reply::json(401, r#"{"error":"The access token is invalid"}"#);
                }
                if !request.query_all("max_id").is_empty() {
                    return Reply::json(200, "[]");
                }
                Reply::json(
                    200,
                    json!([
                        status_document(NEWEST, "<p>Bonjour 🌻</p>"),
                        status_document(OLDER, "<p>La pluie au jardin</p>")
                    ])
                    .to_string(),
                )
                // No automatic request may follow this external Link target.
                .header("link", "<https://never-contact.example/next>; rel=\"next\"")
            }
            path if path == format!("/api/v1/statuses/{NEWEST}") => {
                let mut status = status_document(NEWEST, "<p>Bonjour 🌻, après la pluie</p>");
                status["edited_at"] = json!("2026-11-07T09:42:00.000Z");
                Reply::json(200, status.to_string())
            }
            path if path == format!("/api/v1/statuses/{MISSING}") => Reply::json(
                404,
                json!({"error": format!("Record not found; diagnostic echo {TOKEN}")}).to_string(),
            ),
            path if path == format!("/api/v1/statuses/{BROKEN_JSON}") => {
                Reply::json(200, "{broken").header("x-request-id", "status-bad-json")
            }
            path if path == format!("/api/v1/statuses/{BROKEN_SHAPE}") => {
                Reply::json(200, json!({"id": BROKEN_SHAPE}).to_string())
                    .header("x-request-id", "status-bad-shape")
            }
            path if path == format!("/api/v1/statuses/{WRONG_MEDIA}") => {
                // A refusal must happen at the head, even if the body never arrives.
                Reply::text(200, "text/html", "")
                    .header("x-request-id", "status-wrong-media")
                    .with_body(Body::Stall)
            }
            path if path == format!("/api/v1/statuses/{PROXY_FAILURE}") => {
                Reply::text(502, "text/html", "<h1>Upstream unavailable</h1>")
            }
            _ => Reply::json(404, r#"{"error":"Unexpected fixture route"}"#),
        }
    })
    .await;
    let mastodon = authenticated(&server.url());

    let request = mastodon
        .public_timeline()
        .set_local(true)
        .set_only_media(true)
        .set_limit(TimelineLimit::new(2).unwrap());
    assert!(!format!("{mastodon:?} {request:?}").contains(TOKEN));
    let page = request.send().await.expect("first public timeline page");
    println!("FIRST_WORKING_CALL: public_timeline().send() returned two statuses");
    assert_eq!(page.len(), 2);
    assert_eq!(page[0].id.as_str(), NEWEST);
    assert_eq!(page[0].content, "<p>Bonjour 🌻</p>");
    assert_eq!(page[0].account.acct, "alice");
    assert_eq!(page[0].account.display_name, "Alice 🌻");
    assert!(matches!(page[0].visibility, Visibility::Public));
    assert_eq!(server.requests().len(), 1, "Link does not trigger paging");

    let status = mastodon
        .status(page[0].id.clone())
        .send()
        .await
        .expect("retrieve an id from the timeline");
    assert_eq!(status.id, page[0].id);
    assert_eq!(status.content, "<p>Bonjour 🌻, après la pluie</p>");
    assert_eq!(
        status.edited_at.as_deref(),
        Some("2026-11-07T09:42:00.000Z")
    );

    let older = mastodon
        .public_timeline()
        .set_local(true)
        .set_only_media(true)
        .set_max_id(page.last().unwrap().id.clone())
        .set_limit(TimelineLimit::new(2).unwrap())
        .send()
        .await
        .expect("empty page is a valid page");
    assert!(older.is_empty());
    assert_eq!(
        page.len(),
        2,
        "application retains results from earlier pages"
    );
    let requests = server.requests();
    assert_eq!(
        requests.len(),
        3,
        "caller stops paging after the empty page"
    );
    assert_eq!(
        requests[0].target,
        "/api/v1/timelines/public?local=true&only_media=true&limit=2"
    );
    assert_eq!(requests[1].target, format!("/api/v1/statuses/{NEWEST}"));
    assert_eq!(
        requests[2].target,
        format!("/api/v1/timelines/public?local=true&only_media=true&max_id={OLDER}&limit=2")
    );

    let missing = mastodon
        .status(StatusId::new(MISSING).unwrap())
        .send()
        .await
        .unwrap_err();
    let api = missing.api().expect("provider status error");
    assert!(api.is_not_found());
    assert_eq!(api.status().as_u16(), 404);
    assert!(api.document().is_some());
    assert!(
        api.message().unwrap().contains(TOKEN),
        "explicit access to error text"
    );
    assert_eq!(api.operation().unwrap().name(), "Api.status");
    assert_eq!(api.operation().unwrap().method(), Some("GET"));
    assert_redacted(&format!("{missing} {missing:?}"), &server.url());

    for (id, kind, request_id) in [
        (BROKEN_JSON, ErrorKind::Decode, "status-bad-json"),
        (BROKEN_SHAPE, ErrorKind::Decode, "status-bad-shape"),
        (WRONG_MEDIA, ErrorKind::Metadata, "status-wrong-media"),
    ] {
        let error = mastodon
            .status(StatusId::new(id).unwrap())
            .send()
            .await
            .unwrap_err();
        let request = error.request().expect("Concord request error kept whole");
        assert_eq!(request.kind(), kind);
        assert_eq!(request.endpoint(), Some("Api.status"));
        let head = request
            .head()
            .expect("successful head survives decoding failure");
        assert_eq!(head.status().as_u16(), 200);
        assert_eq!(head.headers()["x-request-id"], request_id);
        assert_eq!(request.response().is_some(), kind == ErrorKind::Metadata);
        assert_redacted(&format!("{error} {error:?}"), &server.url());
    }

    let proxy = mastodon
        .status(StatusId::new(PROXY_FAILURE).unwrap())
        .send()
        .await
        .unwrap_err();
    assert_eq!(proxy.api().unwrap().status().as_u16(), 502);
    assert!(proxy.api().unwrap().document().is_none());
    assert!(proxy.api().unwrap().message().is_none());

    let unauthorized = client(&server.url())
        .public_timeline()
        .send()
        .await
        .unwrap_err();
    assert!(unauthorized.api().unwrap().is_unauthorized());
    assert_eq!(
        unauthorized.api().unwrap().message(),
        Some("The access token is invalid")
    );
    assert_eq!(
        unauthorized.api().unwrap().operation().unwrap().name(),
        "Api.public_timeline"
    );
    assert_eq!(
        server.requests().last().unwrap().target,
        "/api/v1/timelines/public"
    );

    let sent = server.requests().len();
    assert!(TimelineLimit::new(0).is_err());
    assert!(TimelineLimit::new(41).is_err());
    assert!(StatusId::new("with whitespace").is_err());
    assert!(AccessToken::new("with whitespace").is_err());
    assert_eq!(server.requests().len(), sent, "invalid values send nothing");
    let prepared = mastodon.public_timeline().prepare().unwrap();
    assert!(prepared.request().headers()["authorization"].is_sensitive());
    assert!(!format!("{prepared:?}").contains(TOKEN));

    let requests = server.requests();
    assert_eq!(requests.len(), 9, "no status retry or hidden paging");
    for (index, request) in requests.iter().enumerate() {
        assert_wire(
            request,
            &server.addr().to_string(),
            "application/json",
            index != 8,
        );
    }
    server.shutdown().await;
}

async fn next_event(stream: &mut PublicStream) -> Result<StreamEvent, StreamError> {
    tokio::time::timeout(Duration::from_secs(1), stream.next())
        .await
        .expect("fixture event or terminal error arrives promptly")
        .expect("stream must surface a terminal error before None")
}

#[tokio::test]
async fn discover_host_then_follow_public_sse_as_a_fresh_consumer() {
    let update = format!(
        ":thump\n\n: a comment, not an event\n\nevent: update\ndata: {}\n\n",
        status_document(NEWEST, "<p>Bonjour 🌻</p>")
    );
    let field_split = update.find("data: ").unwrap() + 2; // da | ta:
    let utf8_split = update.find("Bonjour 🌻").unwrap() + "Bonjour ".len() + 1;
    assert!(std::str::from_utf8(&update.as_bytes()[..utf8_split]).is_err());
    let mut edited = status_document(NEWEST, "<p>Bonjour 🌻, édition</p>");
    edited["edited_at"] = json!("2026-11-07T09:42:00.000Z");
    let edited = edited.to_string();
    let multiline_split = edited.find("\"content\"").unwrap();
    let subsequent = format!(
        "event: update\ndata: private-payload-sentinel {TOKEN}\n\n\
         event: status.update\ndata: {}\ndata: {}\n\n\
         event: delete\ndata: {OLDER}\n\n\
         event: future.public\ndata: opaque one\ndata: opaque two\n\n\
         event: update\ndata: {{\"id\":\"lost-partial",
        &edited[..multiline_split],
        &edited[multiline_split..]
    );
    let broken_body_bytes = update.len() + subsequent.len();
    let heartbeat = b":thump\n\n";
    let replies = Script::new([
        Reply::text(200, "text/event-stream; charset=utf-8", "").with_body(Body::FailAfter(vec![
            update.as_bytes()[..field_split].to_vec().into(),
            update.as_bytes()[field_split..utf8_split].to_vec().into(),
            update.as_bytes()[utf8_split..].to_vec().into(),
            subsequent.into_bytes().into(),
        ])),
        Reply::text(200, "text/event-stream", "").with_body(Body::Chunks(vec![
            (Duration::ZERO, heartbeat.to_vec().into()),
            (
                Duration::from_millis(200),
                b"event: delete\nda".to_vec().into(),
            ),
            (
                Duration::from_millis(200),
                format!("ta: {NEWEST}\n\n").into_bytes().into(),
            ),
        ])),
        Reply::text(
            200,
            "text/event-stream",
            "event: delete\ndata: 1134419812345678",
        ),
        Reply::text(200, "application/json", "")
            .header("x-request-id", "stream-wrong-media")
            .with_body(Body::Stall),
        Reply::text(200, "text/event-stream", b"event: update\ndata: \xff\n\n"),
    ]);
    let remaining = replies.clone();
    let streaming = TestServer::start(move |request: &Recorded| -> Reply {
        assert_eq!(request.path(), "/api/v1/streaming/public");
        if request.header("authorization").is_none() {
            Reply::json(401, r#"{"error":"The access token is invalid"}"#)
        } else {
            replies.next()
        }
    })
    .await;
    let discovery = Script::new([
        Reply::json(
            200,
            json!({
                "domain": "social.example", "title": "A local fixture instance",
                "version": "4.5.0",
                "configuration": {"urls": {
                    "streaming": streaming.url().replacen("http://", "ws://", 1)
                }}
            })
            .to_string(),
        ),
        Reply::json(200, r#"{"configuration":{"urls":{}}}"#),
        Reply::json(200, "{}").header("x-request-id", "instance-bad-shape"),
        Reply::json(503, r#"{"error":"Streaming discovery unavailable"}"#),
    ]);
    let instance = TestServer::start(move |request: &Recorded| -> Reply {
        assert_eq!(request.target, "/api/v2/instance");
        discovery.next()
    })
    .await;
    let mastodon = authenticated(&instance.url());
    let host = mastodon
        .discover_streaming()
        .await
        .expect("advertised streaming origin");
    assert_eq!(host.url(), format!("{}/", streaming.url()));
    assert_eq!(host.instance(), mastodon.instance_url());
    assert!(!host.is_instance_host());
    assert!(
        streaming.requests().is_empty(),
        "discovery only contacts the REST instance"
    );

    let mut stream = mastodon
        .public_stream(&host)
        .unwrap()
        .set_only_media(true)
        .send()
        .await
        .unwrap();
    assert_eq!(
        stream.position().bytes,
        0,
        "opening does not read event bytes"
    );
    assert!(!stream.is_finished());
    let mut received_ids = Vec::new();
    let StreamEvent::Update(status) = next_event(&mut stream).await.unwrap() else {
        panic!("expected the reassembled status update");
    };
    assert_eq!(status.content, "<p>Bonjour 🌻</p>");
    assert_eq!(status.account.display_name, "Alice 🌻");
    received_ids.push(status.id.clone());
    let payload = next_event(&mut stream).await.unwrap_err();
    assert_eq!(payload.kind(), StreamErrorKind::Payload);
    assert!(!payload.is_terminal());
    assert!(!stream.is_finished());
    assert_eq!(payload.operation(), Some("Api.public_stream"));
    assert!(payload.codec().is_some());
    assert!(payload.position().bytes > 0);
    let diagnostics = format!("{payload} {payload:?} {stream:?}");
    assert!(!diagnostics.contains("private-payload-sentinel"));
    assert_redacted(&diagnostics, &streaming.url());

    let StreamEvent::StatusUpdate(edited) = next_event(&mut stream).await.unwrap() else {
        panic!("expected the edited status after the skipped payload");
    };
    assert_eq!(edited.id.as_str(), NEWEST);
    assert_eq!(edited.content, "<p>Bonjour 🌻, édition</p>");
    assert_eq!(
        edited.edited_at.as_deref(),
        Some("2026-11-07T09:42:00.000Z")
    );
    let StreamEvent::Delete(deleted) = next_event(&mut stream).await.unwrap() else {
        panic!("expected the unquoted deleted id");
    };
    assert_eq!(deleted.as_str(), OLDER);
    let StreamEvent::Other { event, data } = next_event(&mut stream).await.unwrap() else {
        panic!("expected an undecoded future event type");
    };
    assert_eq!(event, "future.public");
    assert_eq!(data, "opaque one\nopaque two");

    let disconnect = next_event(&mut stream).await.unwrap_err();
    assert_eq!(disconnect.kind(), StreamErrorKind::Disconnected);
    assert!(disconnect.is_terminal());
    assert_eq!(disconnect.operation(), Some("Api.public_stream"));
    assert_eq!(disconnect.position().bytes, broken_body_bytes as u64);
    let transport = disconnect
        .transport()
        .expect("broken HTTP body has a transport cause");
    assert_eq!(transport.kind(), ErrorKind::Transport);
    assert_eq!(transport.endpoint(), Some("Api.public_stream"));
    assert_redacted(&format!("{disconnect} {disconnect:?}"), &streaming.url());
    assert!(stream.is_finished());
    assert!(stream.next().await.is_none());
    assert!(stream.next().await.is_none());
    assert_eq!(
        received_ids[0].as_str(),
        NEWEST,
        "complete results survive the dropped tail"
    );
    assert_eq!(
        received_ids.len(),
        1,
        "no status invented from the partial event"
    );
    assert_eq!(
        streaming.requests().len(),
        1,
        "no reconnect or replay request"
    );

    // Heartbeats count as activity even when a pending next() is cancelled by a timeout.
    let sent_before = streaming.body_bytes_sent();
    let mut quiet = mastodon.public_stream(&host).unwrap().send().await.unwrap();
    assert!(
        streaming
            .wait_until(Duration::from_secs(1), |server| {
                server.body_bytes_sent() >= sent_before + heartbeat.len() as u64
            })
            .await
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(30), quiet.next())
            .await
            .is_err()
    );
    assert_eq!(quiet.position().bytes, heartbeat.len() as u64);
    assert!(!quiet.is_finished());
    assert!(
        matches!(next_event(&mut quiet).await.unwrap(), StreamEvent::Delete(id) if id.as_str() == NEWEST)
    );
    let closed = next_event(&mut quiet).await.unwrap_err();
    assert_eq!(closed.kind(), StreamErrorKind::Closed);
    assert!(closed.is_terminal());
    assert!(quiet.is_finished());
    assert!(quiet.next().await.is_none());

    let mut truncated = mastodon.public_stream(&host).unwrap().send().await.unwrap();
    let error = next_event(&mut truncated).await.unwrap_err();
    assert_eq!(error.kind(), StreamErrorKind::Truncated);
    assert!(error.is_terminal());
    assert!(truncated.is_finished());
    assert!(truncated.next().await.is_none());

    let metadata = mastodon
        .public_stream(&host)
        .unwrap()
        .send()
        .await
        .unwrap_err();
    let request = metadata.request().unwrap();
    assert_eq!(request.kind(), ErrorKind::Metadata);
    assert_eq!(request.endpoint(), Some("Api.public_stream"));
    assert_eq!(
        request.response().unwrap().headers()["x-request-id"],
        "stream-wrong-media"
    );
    assert_eq!(request.head().unwrap().status().as_u16(), 200);

    let mut malformed = mastodon.public_stream(&host).unwrap().send().await.unwrap();
    let error = next_event(&mut malformed).await.unwrap_err();
    assert_eq!(error.kind(), StreamErrorKind::Malformed);
    assert!(error.is_terminal());
    assert!(malformed.next().await.is_none());
    let unauthorized = client(&instance.url())
        .public_stream(&host)
        .unwrap()
        .send()
        .await
        .unwrap_err();
    assert!(unauthorized.api().unwrap().is_unauthorized());
    assert_eq!(
        unauthorized.api().unwrap().operation().unwrap().name(),
        "Api.public_stream"
    );
    assert_eq!(remaining.remaining(), 0);

    let absent = mastodon.discover_streaming().await.unwrap_err();
    assert!(matches!(
        absent.discovery(),
        Some(DiscoveryError::NotAdvertised)
    ));
    let malformed = mastodon.discover_streaming().await.unwrap_err();
    let request = malformed.request().unwrap();
    assert_eq!(request.kind(), ErrorKind::Decode);
    assert_eq!(request.endpoint(), Some("Api.instance"));
    assert_eq!(
        request.head().unwrap().headers()["x-request-id"],
        "instance-bad-shape"
    );
    let unavailable = mastodon.discover_streaming().await.unwrap_err();
    assert_eq!(unavailable.api().unwrap().status().as_u16(), 503);
    assert_eq!(
        unavailable.api().unwrap().message(),
        Some("Streaming discovery unavailable")
    );

    // Configuration guards are local: no request to any hypothetical remote instance.
    let secure = InstanceUrl::parse("https://social.example").unwrap();
    assert!(matches!(
        StreamingHost::advertised_by(&secure, "ws://streaming.example"),
        Err(DiscoveryError::Insecure)
    ));
    let other = authenticated(&streaming.url())
        .public_stream(&host)
        .unwrap_err();
    assert!(matches!(
        other.discovery(),
        Some(DiscoveryError::OtherInstance)
    ));

    let requests = streaming.requests();
    assert_eq!(
        requests.len(),
        6,
        "five explicit authorized opens, one unauthorized open"
    );
    assert_eq!(
        requests[0].target,
        "/api/v1/streaming/public?only_media=true"
    );
    for (index, request) in requests.iter().enumerate() {
        assert_wire(
            request,
            &streaming.addr().to_string(),
            "text/event-stream",
            index != 5,
        );
        if index != 0 {
            assert_eq!(request.target, "/api/v1/streaming/public");
        }
    }
    let requests = instance.requests();
    assert_eq!(
        requests.len(),
        4,
        "one discovery per explicit caller request"
    );
    for request in &requests {
        assert_wire(
            request,
            &instance.addr().to_string(),
            "application/json",
            true,
        );
        assert_eq!(request.target, "/api/v2/instance");
    }
    streaming.shutdown().await;
    instance.shutdown().await;
}
