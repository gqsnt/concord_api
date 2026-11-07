//! Wire-level request shapes, every documented error class, malformed / truncated / oversized /
//! wrong-media bodies, the stream's framing and payload rules, the token's destinations and
//! redaction. Every server is a loopback stand-in (`examples/testserver`); fixtures are listed in
//! `PROVENANCE.md`.

mod common;

use std::time::Duration;

use common::*;
use concord_example_testserver::{Body, Reply, TestServer, generated_records};
use concord_sdk_mastodon::concord::ErrorKind;
use concord_sdk_mastodon::concord::bytes::Bytes;
use concord_sdk_mastodon::{
    AccessToken, DiscoveryError, Error, InstanceUrl, Mastodon, PublicStream, StatusId,
    StreamErrorKind, StreamEvent, StreamingHost, TimelineLimit, Visibility,
};

fn id(text: &str) -> StatusId {
    StatusId::new(text).expect("a valid id")
}

/// A stream opened against `server` (the instance's own host).
async fn open_stream(server: &TestServer) -> PublicStream {
    let mastodon = authorized(server);
    let host = StreamingHost::instance_host(mastodon.instance_url());
    mastodon
        .public_stream(&host)
        .expect("same instance")
        .send()
        .await
        .expect("the head is accepted")
}

/// Every event until the stream stops: the events, then the terminal error's kind.
async fn drain(stream: &mut PublicStream) -> (Vec<StreamEvent>, StreamErrorKind) {
    let mut events = Vec::new();
    loop {
        match stream.next().await.expect("an error comes before the end") {
            Ok(event) => events.push(event),
            Err(error) if !error.is_terminal() => panic!("unexpected payload error: {error}"),
            Err(error) => {
                assert!(stream.next().await.is_none(), "fused after the error");
                return (events, error.kind());
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Request shapes
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn public_timeline_request_on_the_wire() {
    let server = TestServer::start(|_| json(200, fixture("public_timeline.json"))).await;
    let page = authorized(&server)
        .public_timeline()
        .set_local(true)
        .set_remote(false)
        .set_only_media(true)
        .set_max_id(id("103206804533200177"))
        .set_since_id(id("1"))
        .set_min_id(id("2"))
        .set_limit(TimelineLimit::new(40).unwrap())
        .send()
        .await
        .expect("a page");
    assert_eq!(page.len(), 2);

    let request = server.single();
    assert_eq!(request.method, "GET");
    assert_eq!(
        request.target,
        "/api/v1/timelines/public?local=true&remote=false&only_media=true\
         &max_id=103206804533200177&since_id=1&min_id=2&limit=40"
    );
    assert_eq!(request.header("accept"), Some("application/json"));
    assert_eq!(authorization(&request), [BEARER]);
    assert!(request.body.is_empty());
}

#[tokio::test]
async fn unset_parameters_and_no_token_send_nothing() {
    let server = TestServer::start(|_| json(200, "[]")).await;
    let page = anonymous(&server).public_timeline().send().await.unwrap();
    assert!(
        page.is_empty(),
        "an empty page is a page (a disabled feed answers one too)"
    );
    let request = server.single();
    assert_eq!(request.target, "/api/v1/timelines/public");
    assert!(authorization(&request).is_empty());
}

#[test]
fn timeline_limit_is_1_to_40() {
    assert!(TimelineLimit::new(0).is_err());
    assert!(TimelineLimit::new(41).is_err());
    assert_eq!(TimelineLimit::new(1).unwrap().get(), 1);
    assert_eq!(TimelineLimit::new(40).unwrap().get(), 40);
}

#[tokio::test]
async fn status_request_on_the_wire() {
    let server = TestServer::start(|_| json(200, fixture("status.json"))).await;
    let status = authorized(&server)
        .status(id("103206804533200177"))
        .send()
        .await
        .unwrap();
    assert_eq!(status.id.as_str(), "103206804533200177");
    let request = server.single();
    assert_eq!(request.target, "/api/v1/statuses/103206804533200177");
    assert_eq!(request.header("accept"), Some("application/json"));
    assert_eq!(authorization(&request), [BEARER]);
}

#[test]
fn a_status_id_is_one_path_segment_and_never_blank() {
    let mastodon = Mastodon::new(
        http(),
        InstanceUrl::parse("https://mastodon.example").unwrap(),
    );
    let prepared = mastodon.status(id("a/b?c")).prepare().unwrap();
    assert_eq!(
        prepared.request().url().as_str(),
        "https://mastodon.example/api/v1/statuses/a%2Fb%3Fc"
    );
    for bad in ["", " ", "1 2", "1\n", "\t1"] {
        assert!(StatusId::new(bad).is_err(), "{bad:?}");
    }
    // `.` and `..` are valid ids for this SDK, but not path segments: Concord refuses to build
    // the request and nothing is sent.
    let error = mastodon.status(id("..")).prepare().unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Build);
}

#[tokio::test]
async fn instance_request_on_the_wire() {
    let server = TestServer::start(|_| json(200, fixture("instance.json"))).await;
    let info = anonymous(&server).instance().send().await.unwrap();
    assert_eq!(
        info.streaming_url(),
        Some("wss://streaming.mastodon.example")
    );
    let request = server.single();
    assert_eq!(request.target, "/api/v2/instance");
    assert_eq!(request.header("accept"), Some("application/json"));
}

#[tokio::test]
async fn public_stream_request_on_the_wire() {
    let server = TestServer::start(|_| sse(vec![b":thump\n".to_vec()])).await;
    let mastodon = authorized(&server);
    let host = StreamingHost::instance_host(mastodon.instance_url());
    let mut stream = mastodon
        .public_stream(&host)
        .unwrap()
        .set_only_media(true)
        .send()
        .await
        .unwrap();
    assert_eq!(
        stream.next().await.unwrap().unwrap_err().kind(),
        StreamErrorKind::Closed
    );
    let request = server.single();
    assert_eq!(request.target, "/api/v1/streaming/public?only_media=true");
    assert_eq!(request.header("accept"), Some("text/event-stream"));
    assert_eq!(authorization(&request), [BEARER]);
}

#[test]
fn instance_url_rules() {
    for (input, reason) in [
        ("mastodon.example", "is not an absolute URL"),
        ("ftp://mastodon.example", "must use http or https"),
        (
            "https://user:secretpw@mastodon.example",
            "must not contain user information",
        ),
        ("https://mastodon.example/?a=1", "must not contain a query"),
        (
            "https://mastodon.example/#top",
            "must not contain a fragment",
        ),
        ("https://mastodon.example/api", "must not contain a path"),
        ("https://mastodon.example:0", "must not use port 0"),
    ] {
        let error = InstanceUrl::parse(input).unwrap_err();
        assert_eq!(error.reason(), reason, "{input}");
        assert!(!error.to_string().contains("secretpw"));
    }
    let ok = InstanceUrl::parse("https://Mastodon.Example").unwrap();
    assert_eq!(ok.as_str(), "https://mastodon.example/");
    assert_eq!(
        InstanceUrl::parse("http://127.0.0.1:8080/")
            .unwrap()
            .as_str(),
        "http://127.0.0.1:8080/"
    );
}

#[test]
fn token_rules() {
    for bad in ["", "a b", "a\tb", "tök", "a\nb"] {
        let error = AccessToken::new(bad).unwrap_err();
        assert_eq!(
            error.to_string(),
            "an access token must be non-empty visible ASCII without spaces"
        );
    }
    assert!(AccessToken::new(TOKEN).is_ok());
}

// ---------------------------------------------------------------------------------------------
// Documented error classes and other non-2xx answers
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn public_timeline_401_keeps_the_documented_error() {
    let server = TestServer::start(|_| json(401, fixture("error_unauthorized.json"))).await;
    let error = anonymous(&server)
        .public_timeline()
        .send()
        .await
        .unwrap_err();
    let api = error.api().expect("a non-2xx answer");
    assert!(api.is_unauthorized());
    assert_eq!(api.status().as_u16(), 401);
    assert_eq!(api.message(), Some("The access token is invalid"));
    assert_eq!(
        api.operation().map(|op| op.name()),
        Some("Api.public_timeline")
    );
    // The document's text is not part of `Display`.
    assert_eq!(
        error.to_string(),
        "GET Api.public_timeline: Mastodon answered 401"
    );
}

#[tokio::test]
async fn status_404_keeps_the_documented_error() {
    let server = TestServer::start(|_| json(404, fixture("error_not_found.json"))).await;
    let error = authorized(&server)
        .status(id("1"))
        .send()
        .await
        .unwrap_err();
    let api = error.api().unwrap();
    assert!(api.is_not_found());
    assert_eq!(api.message(), Some("Record not found"));
}

#[tokio::test]
async fn stream_400_unknown_channel_keeps_the_documented_error() {
    let server = TestServer::start(|_| json(400, fixture("error_unknown_channel.json"))).await;
    let mastodon = authorized(&server);
    let host = StreamingHost::instance_host(mastodon.instance_url());
    let error = mastodon
        .public_stream(&host)
        .unwrap()
        .send()
        .await
        .unwrap_err();
    let api = error.api().unwrap();
    assert_eq!(api.status().as_u16(), 400);
    assert_eq!(api.message(), Some("Unknown channel requested"));
    assert_eq!(
        api.operation().map(|op| op.name()),
        Some("Api.public_stream")
    );
}

#[tokio::test]
async fn stream_401_without_a_token() {
    // Synthetic: the page states that this stream requires a user token, not the answer.
    let server = TestServer::start(|request| {
        if request.header("authorization").is_none() {
            json(401, r#"{"error":"SYNTHETIC missing token"}"#)
        } else {
            sse(vec![])
        }
    })
    .await;
    let mastodon = anonymous(&server);
    let host = StreamingHost::instance_host(mastodon.instance_url());
    let error = mastodon
        .public_stream(&host)
        .unwrap()
        .send()
        .await
        .unwrap_err();
    assert!(error.api().unwrap().is_unauthorized());
}

#[tokio::test]
async fn other_statuses_keep_the_status_without_a_document() {
    let large = format!(r#"{{"error":"{}"}}"#, "x".repeat(70 * 1024));
    let replies: Vec<(u16, Reply)> = vec![
        // Synthetic: a rate limit, a proxy's HTML page, an oversized document, a document not
        // labelled JSON, a non-JSON body, an empty body.
        (429, json(429, r#"{"error":"Too many requests"}"#)),
        (
            502,
            Reply::text(502, "text/html", "<html>SYNTHETIC bad gateway</html>"),
        ),
        (500, json(500, large)),
        (
            503,
            Reply::text(503, "text/plain", r#"{"error":"labelled text"}"#),
        ),
        (500, json(500, "not json")),
        (503, Reply::new(503)),
    ];
    for (status, reply) in replies {
        let reply = std::sync::Mutex::new(Some(reply));
        let server = TestServer::start(move |_| reply.lock().unwrap().take().unwrap()).await;
        let error = authorized(&server)
            .public_timeline()
            .send()
            .await
            .unwrap_err();
        let api = error.api().expect("a non-2xx answer");
        assert_eq!(api.status().as_u16(), status);
        if status == 429 {
            assert_eq!(api.message(), Some("Too many requests"));
        } else {
            assert!(api.document().is_none(), "{status}");
        }
    }
}

#[tokio::test]
async fn no_answer_is_a_transport_error() {
    let server = TestServer::start(|_| Reply::hang_up()).await;
    let error = authorized(&server)
        .public_timeline()
        .send()
        .await
        .unwrap_err();
    assert_eq!(error.request().unwrap().kind(), ErrorKind::Transport);
    assert!(!format!("{error} {error:?}").contains(TOKEN));
}

// ---------------------------------------------------------------------------------------------
// Malformed, truncated, oversized and wrong-media success bodies
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn malformed_and_wrong_shape_bodies_are_decode_errors() {
    let cases: Vec<&'static str> = vec![
        "[{\"id\": SECRETPAYLOAD",          // not JSON
        r#"{"id":"1"}"#,                    // an object, not a page
        r#"[{"id":"1","created_at":"x"}]"#, // required members missing
        r#"[{"id":"","uri":"u","created_at":"c","content":"","visibility":"public","account":{"id":"1","username":"u","acct":"u"}}]"#, // empty id
    ];
    for body in cases {
        let server = TestServer::start(move |_| json(200, body)).await;
        let error = authorized(&server)
            .public_timeline()
            .send()
            .await
            .unwrap_err();
        let request = error.request().expect("Concord's error");
        assert_eq!(request.kind(), ErrorKind::Decode, "{body}");
        assert!(!format!("{error} {error:?}").contains("SECRETPAYLOAD"));
    }
}

#[tokio::test]
async fn a_status_missing_its_account_does_not_decode() {
    let server = TestServer::start(|_| {
        json(
            200,
            r#"{"id":"1","uri":"u","created_at":"c","content":"","visibility":"public"}"#,
        )
    })
    .await;
    let error = authorized(&server)
        .status(id("1"))
        .send()
        .await
        .unwrap_err();
    assert_eq!(error.request().unwrap().kind(), ErrorKind::Decode);
}

#[tokio::test]
async fn unknown_visibility_and_members_are_kept_or_ignored() {
    let server = TestServer::start(|_| {
        json(
            200,
            r#"{"id":"1","uri":"u","created_at":"c","content":"","visibility":"limited",
                "account":{"id":"1","username":"u","acct":"u"},"future_member":{"x":1}}"#,
        )
    })
    .await;
    let status = authorized(&server).status(id("1")).send().await.unwrap();
    assert_eq!(status.visibility, Visibility::Other("limited".to_owned()));
    assert!(status.url.is_none() && !status.sensitive && status.spoiler_text.is_empty());
}

#[tokio::test]
async fn a_body_cut_off_is_a_transport_error() {
    let server = TestServer::start(|_| {
        Reply::new(200)
            .header("content-type", "application/json; charset=utf-8")
            .with_body(Body::FailAfter(vec![Bytes::from_static(b"[{\"id\":\"1")]))
    })
    .await;
    let error = authorized(&server)
        .public_timeline()
        .send()
        .await
        .unwrap_err();
    let request = error.request().unwrap();
    assert_eq!(request.kind(), ErrorKind::Transport);
    assert_eq!(request.head().map(|head| head.status().as_u16()), Some(200));
}

#[tokio::test]
async fn an_oversized_body_is_a_limit_error() {
    // 9 MiB of statuses, generated lazily, past the 8 MiB bound.
    let server = TestServer::start(|_| {
        let records = generated_records(9 * 1024, 1024, |i| {
            let mut record = if i == 0 { b"[".to_vec() } else { b",".to_vec() };
            record.extend_from_slice(&[b' '; 1022]);
            record.push(b'1');
            record
        });
        Reply::new(200)
            .header("content-type", "application/json")
            .with_body(Body::Generate(records, None))
    })
    .await;
    let error = authorized(&server)
        .public_timeline()
        .send()
        .await
        .unwrap_err();
    assert_eq!(error.request().unwrap().kind(), ErrorKind::Limit);
}

#[tokio::test]
async fn a_success_not_labelled_json_is_refused_unread() {
    for content_type in ["text/html", "text/plain"] {
        let server =
            TestServer::start(move |_| Reply::text(200, content_type, fixture("status.json")))
                .await;
        let error = authorized(&server)
            .status(id("1"))
            .send()
            .await
            .unwrap_err();
        let request = error.request().unwrap();
        assert_eq!(request.kind(), ErrorKind::Metadata, "{content_type}");
        assert!(request.response().is_some(), "the response is kept unread");
    }
    let server = TestServer::start(|_| Reply::new(200).body(fixture("status.json"))).await;
    let error = authorized(&server)
        .status(id("1"))
        .send()
        .await
        .unwrap_err();
    assert_eq!(error.request().unwrap().kind(), ErrorKind::Metadata);
}

#[tokio::test]
async fn a_stream_not_labelled_text_event_stream_is_refused_unread() {
    for content_type in [
        "application/json",
        "text/plain",
        "text/event-stream; charset=iso-8859-1",
    ] {
        let server =
            TestServer::start(move |_| Reply::text(200, content_type, frame("delete", "1"))).await;
        let mastodon = authorized(&server);
        let host = StreamingHost::instance_host(mastodon.instance_url());
        let error = mastodon
            .public_stream(&host)
            .unwrap()
            .send()
            .await
            .unwrap_err();
        assert_eq!(
            error.request().unwrap().kind(),
            ErrorKind::Metadata,
            "{content_type}"
        );
    }
    // `charset=utf-8` is accepted.
    let server = TestServer::start(|_| {
        Reply::text(
            200,
            "text/event-stream; charset=utf-8",
            frame("delete", "1"),
        )
    })
    .await;
    let mut stream = open_stream(&server).await;
    assert!(matches!(
        stream.next().await,
        Some(Ok(StreamEvent::Delete(_)))
    ));
}

// ---------------------------------------------------------------------------------------------
// The stream: framing, payloads, ends
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn the_documented_events_decode() {
    let server = TestServer::start(|_| sse(vec![fixture("stream_public_documented.txt")])).await;
    let mut stream = open_stream(&server).await;
    let (events, end) = drain(&mut stream).await;
    assert_eq!(end, StreamErrorKind::Closed);
    let [
        StreamEvent::Update(update),
        StreamEvent::Delete(deleted),
        StreamEvent::StatusUpdate(edited),
    ] = events.as_slice()
    else {
        panic!("{events:?}")
    };
    assert_eq!(update.id.as_str(), "108914354907984653");
    assert_eq!(update.account.acct, "aiueohisama@mstdn.jp");
    assert_eq!(update.content, "<p>これは合成テキストです 🐘</p>");
    assert_eq!(update.visibility, Visibility::Public);
    assert_eq!(deleted.as_str(), "107214471804101576");
    assert_eq!(edited.id.as_str(), "109348684737626801");
    assert_eq!(
        edited.edited_at.as_deref(),
        Some("2022-11-15T16:10:43.000Z")
    );
    assert_eq!(stream.position().records, 3, "heartbeats are not events");
}

/// Offsets of interest in the documented stream: inside each multi-byte character of the
/// update's content, and around and inside every `data:` field name.
fn interesting_cuts(input: &[u8]) -> Vec<usize> {
    let text = std::str::from_utf8(input).unwrap();
    let mut cuts = Vec::new();
    for (at, c) in text.char_indices() {
        if c.len_utf8() > 1 {
            cuts.extend(at + 1..at + c.len_utf8());
        }
    }
    for (at, _) in text.match_indices("data:") {
        cuts.extend(at..=at + 6);
    }
    cuts.sort_unstable();
    cuts.dedup();
    cuts
}

#[tokio::test]
async fn a_character_or_a_data_field_split_across_chunks_is_reassembled() {
    let input = fixture("stream_public_documented.txt");
    let whole = {
        let server = TestServer::start({
            let input = input.clone();
            move |_| sse(vec![input.clone()])
        })
        .await;
        format!("{:?}", drain(&mut open_stream(&server).await).await)
    };
    let cuts = interesting_cuts(&input);
    assert!(cuts.len() > 20, "{cuts:?}");
    for at in cuts {
        let chunks = cut(&input, &[at]);
        // Delay the second chunk so that it travels on its own.
        let reply = move || {
            sse_with(Body::Chunks(vec![
                (Duration::ZERO, Bytes::from(chunks[0].clone())),
                (Duration::from_millis(2), Bytes::from(chunks[1].clone())),
            ]))
        };
        let server = TestServer::start(move |_| reply()).await;
        let split = format!("{:?}", drain(&mut open_stream(&server).await).await);
        assert_eq!(split, whole, "cut at {at}");
    }
}

#[tokio::test]
async fn one_byte_per_chunk_gives_the_same_events() {
    let input = fixture("stream_public_documented.txt");
    let chunks: Vec<(Duration, Bytes)> = input
        .iter()
        .map(|&b| (Duration::ZERO, Bytes::from(vec![b])))
        .collect();
    let server = TestServer::start(move |_| sse_with(Body::Chunks(chunks.clone()))).await;
    let (events, end) = drain(&mut open_stream(&server).await).await;
    assert_eq!(end, StreamErrorKind::Closed);
    assert_eq!(events.len(), 3);
    let StreamEvent::Update(update) = &events[0] else {
        panic!()
    };
    assert_eq!(update.content, "<p>これは合成テキストです 🐘</p>");
}

#[tokio::test]
async fn heartbeats_and_comments_are_not_events_but_count_as_bytes() {
    let server = TestServer::start(|_| sse(vec![b":thump\n:thump\n\n: comment\n".to_vec()])).await;
    let mut stream = open_stream(&server).await;
    let error = stream.next().await.unwrap().unwrap_err();
    assert_eq!(error.kind(), StreamErrorKind::Closed);
    assert_eq!(error.position().records, 0);
    assert_eq!(error.position().bytes, 25);
    assert!(stream.next().await.is_none());
    assert!(stream.is_finished());
}

#[tokio::test]
async fn crlf_and_multi_line_data_follow_the_sse_rules() {
    // Synthetic: Mastodon's documented frames use LF; the reader accepts CRLF and joins data lines.
    let body = format!(
        "event: update\r\ndata: {}\r\ndata: \r\n\r\nevent: other.kind\r\ndata: a\r\ndata: b\r\n\r\n",
        status_json("5", "x")
    );
    let server = TestServer::start(move |_| sse(vec![body.clone().into_bytes()])).await;
    let (events, end) = drain(&mut open_stream(&server).await).await;
    assert_eq!(end, StreamErrorKind::Closed);
    assert!(matches!(&events[0], StreamEvent::Update(s) if s.id.as_str() == "5"));
    let StreamEvent::Other { event, data } = &events[1] else {
        panic!("{events:?}")
    };
    assert_eq!((event.as_str(), data.as_str()), ("other.kind", "a\nb"));
}

#[tokio::test]
async fn a_payload_that_does_not_decode_is_skipped_not_terminal() {
    let mut body = Vec::new();
    body.extend(frame("update", "{\"id\": SECRETPAYLOAD"));
    body.extend(frame("update", r#"{"id":"1","SECRETPAYLOAD":true}"#));
    body.extend(frame("status.update", "[]"));
    body.extend(b"event: delete\ndata:\n\n");
    body.extend(frame("delete", "has space"));
    body.extend(frame("update", &status_json("7", "ok")));
    let server = TestServer::start(move |_| sse(vec![body.clone()])).await;
    let mut stream = open_stream(&server).await;

    let mut labels = Vec::new();
    for event in 1..=5u64 {
        let error = stream.next().await.unwrap().unwrap_err();
        if event <= 3 {
            // The serde error is reachable deliberately.
            assert!(error.source_unredacted().is_some());
        }
        assert_eq!(error.kind(), StreamErrorKind::Payload);
        assert!(!error.is_terminal());
        assert_eq!(error.codec().and_then(|c| c.get("event")), Some(event));
        let shown = format!("{error} {error:?}");
        assert!(
            !shown.contains("SECRETPAYLOAD") && !shown.contains("has space"),
            "{shown}"
        );
        labels.push(error.codec().unwrap().label());
    }
    assert_eq!(
        labels,
        [
            "an event's data is not one JSON value",
            "an event's data is not a Mastodon status",
            "an event's data is not a Mastodon status",
            "a delete event's data is not a status id",
            "a delete event's data is not a status id",
        ]
    );
    assert!(
        matches!(stream.next().await, Some(Ok(StreamEvent::Update(s))) if s.id.as_str() == "7")
    );
    assert_eq!(
        stream.next().await.unwrap().unwrap_err().kind(),
        StreamErrorKind::Closed
    );
}

#[tokio::test]
async fn invalid_utf8_is_malformed_and_terminal() {
    let mut body = frame("delete", "1");
    body.extend(b"event: update\ndata: \xff\xfe\n\n");
    body.extend(frame("delete", "2"));
    let server = TestServer::start(move |_| sse(vec![body.clone()])).await;
    let (events, end) = drain(&mut open_stream(&server).await).await;
    assert_eq!(events.len(), 1);
    assert_eq!(end, StreamErrorKind::Malformed);
}

#[tokio::test]
async fn a_body_ending_inside_an_event_is_truncated() {
    for tail in [
        &b"event: delete\ndata: 2\n"[..],
        b"event: delete\ndata: 2",
        b"data: 2\r",
    ] {
        let mut body = frame("delete", "1");
        body.extend(tail);
        let server = TestServer::start(move |_| sse(vec![body.clone()])).await;
        let mut stream = open_stream(&server).await;
        let (events, end) = drain(&mut stream).await;
        assert_eq!(events.len(), 1, "{tail:?}");
        assert_eq!(end, StreamErrorKind::Truncated, "{tail:?}");
    }
}

#[tokio::test]
async fn an_oversized_event_is_too_large_and_terminal() {
    // A `data:` line past 1 MiB, generated lazily: never buffered whole, never truncated.
    let server = TestServer::start(|_| {
        let mut sent = 0usize;
        sse_with(Body::Generate(
            Box::new(move || {
                sent += 1;
                match sent {
                    1 => Some(Bytes::from_static(b"event: update\ndata: ")),
                    2..=1100 => Some(Bytes::from(vec![b'a'; 1024])),
                    _ => None,
                }
            }),
            None,
        ))
    })
    .await;
    let (events, end) = drain(&mut open_stream(&server).await).await;
    assert!(events.is_empty());
    assert_eq!(end, StreamErrorKind::TooLarge);
}

#[tokio::test]
async fn a_dropped_connection_is_disconnected() {
    let server = TestServer::start(|_| {
        let mut chunks = vec![Bytes::from(frame("delete", "1"))];
        chunks.push(Bytes::from_static(b"event: update\ndata: {\"id\""));
        sse_with(Body::FailAfter(chunks))
    })
    .await;
    let mut stream = open_stream(&server).await;
    assert!(matches!(
        stream.next().await,
        Some(Ok(StreamEvent::Delete(_)))
    ));
    let error = stream.next().await.unwrap().unwrap_err();
    assert_eq!(error.kind(), StreamErrorKind::Disconnected);
    assert!(error.is_terminal());
    assert_eq!(error.operation(), Some("Api.public_stream"));
    let transport = error.transport().expect("Concord's transport error");
    assert_eq!(transport.kind(), ErrorKind::Transport);
    assert_eq!(error.position().records, 1);
    let shown = format!("{error} {error:?}");
    assert!(
        !shown.contains("127.0.0.1") && !shown.contains(TOKEN),
        "{shown}"
    );
    assert!(stream.next().await.is_none());
    // No reconnect: the server saw one request.
    assert_eq!(server.requests().len(), 1);
}

// ---------------------------------------------------------------------------------------------
// The streaming host and the token's destinations
// ---------------------------------------------------------------------------------------------

#[test]
fn advertised_streaming_urls() {
    let https = InstanceUrl::parse("https://mastodon.example").unwrap();
    let http = InstanceUrl::parse("http://127.0.0.1:3000").unwrap();
    let host = |instance: &InstanceUrl, url: &str| {
        StreamingHost::advertised_by(instance, url).map(|host| host.url().to_owned())
    };
    assert_eq!(
        host(&https, "wss://streaming.mastodon.example").unwrap(),
        "https://streaming.mastodon.example/"
    );
    assert_eq!(
        host(&https, "wss://streaming.mastodon.example:4443").unwrap(),
        "https://streaming.mastodon.example:4443/"
    );
    assert_eq!(
        host(&https, "https://mastodon.example/").unwrap(),
        "https://mastodon.example/"
    );
    assert_eq!(
        host(&http, "ws://127.0.0.1:4000").unwrap(),
        "http://127.0.0.1:4000/"
    );
    assert_eq!(
        host(&http, "http://127.0.0.1:4000").unwrap(),
        "http://127.0.0.1:4000/"
    );
    assert_eq!(
        host(&https, "ws://streaming.mastodon.example"),
        Err(DiscoveryError::Insecure)
    );
    assert_eq!(
        host(&https, "http://streaming.mastodon.example"),
        Err(DiscoveryError::Insecure)
    );
    assert_eq!(host(&https, ""), Err(DiscoveryError::NotAdvertised));
    for bad in [
        "streaming.mastodon.example",
        "ftp://streaming.mastodon.example",
        "wss://streaming.mastodon.example/api",
        "wss://streaming.mastodon.example/?x=1",
        "wss://user:pw@streaming.mastodon.example",
    ] {
        assert!(
            matches!(host(&https, bad), Err(DiscoveryError::Unsupported(_))),
            "{bad}"
        );
    }
    let same = StreamingHost::instance_host(&https);
    assert!(same.is_instance_host());
    assert!(
        !StreamingHost::advertised_by(&https, "wss://streaming.mastodon.example")
            .unwrap()
            .is_instance_host()
    );
}

#[tokio::test]
async fn discovery_without_an_advertised_url() {
    for body in [
        r#"{"configuration":{"urls":{"streaming":null}}}"#,
        r#"{"configuration":{"urls":{"streaming":""}}}"#,
        r#"{"configuration":{}}"#,
    ] {
        let server = TestServer::start(move |_| json(200, body)).await;
        let error = authorized(&server).discover_streaming().await.unwrap_err();
        assert_eq!(
            error.discovery(),
            Some(&DiscoveryError::NotAdvertised),
            "{body}"
        );
    }
    // A document without `configuration` is not the instance document this SDK reads.
    for body in ["[]", r#"{"domain":"mastodon.example"}"#] {
        let server = TestServer::start(move |_| json(200, body)).await;
        let error = authorized(&server).discover_streaming().await.unwrap_err();
        assert_eq!(error.request().unwrap().kind(), ErrorKind::Decode, "{body}");
    }
}

#[tokio::test]
async fn a_streaming_host_of_another_instance_is_refused_before_sending() {
    let a = TestServer::start(|_| sse(vec![])).await;
    let b = TestServer::start(|_| sse(vec![])).await;
    let host_of_a = StreamingHost::instance_host(&instance(&a));
    let error = authorized(&b).public_stream(&host_of_a).unwrap_err();
    assert!(matches!(
        error,
        Error::Discovery(DiscoveryError::OtherInstance)
    ));
    assert!(a.requests().is_empty() && b.requests().is_empty());
}

#[tokio::test]
async fn a_redirect_to_another_origin_does_not_carry_the_token() {
    let other = TestServer::start(|_| json(200, fixture("status.json"))).await;
    let target = format!("{}/api/v1/statuses/103206804533200177", other.url());
    let server =
        TestServer::start(move |_| Reply::new(302).header("location", target.clone())).await;
    // reqwest's default redirect policy follows the hop and drops `Authorization`.
    let status = authorized(&server).status(id("1")).send().await.unwrap();
    assert_eq!(status.id.as_str(), "103206804533200177");
    assert_eq!(authorization(&server.single()), [BEARER]);
    assert!(authorization(&other.single()).is_empty());
}

// ---------------------------------------------------------------------------------------------
// Redaction
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn the_token_is_never_shown() {
    let token = AccessToken::new(TOKEN).unwrap();
    assert_eq!(format!("{token:?}"), "AccessToken(<redacted>)");

    let server = TestServer::start(|_| json(401, fixture("error_unauthorized.json"))).await;
    let mastodon = authorized(&server);
    let prepared = mastodon.public_timeline().prepare().unwrap();
    assert!(prepared.request().headers()["authorization"].is_sensitive());
    let host = StreamingHost::instance_host(mastodon.instance_url());
    let stream = mastodon.public_stream(&host).unwrap();
    let error = mastodon.public_timeline().send().await.unwrap_err();
    let shown = format!(
        "{mastodon:?} {prepared:?} {stream:?} {:?} {error} {error:?}",
        mastodon.public_timeline()
    );
    assert!(!shown.contains(TOKEN), "{shown}");
    assert!(shown.contains("<redacted>"), "{shown}");
    assert!(!shown.contains("The access token is invalid"), "{shown}");
}
