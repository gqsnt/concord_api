//! Wire-level checks: what each operation sends (method, target, headers, exact body bytes),
//! the documented success and failure shapes, and the negative cases of the brief (SSE framing
//! across chunk boundaries, keepalives, unknown fields, broken and truncated bodies, refused
//! heads, error documents). Every response comes from the loopback server in
//! `examples/testserver`; nothing here contacts ntfy.

mod common;

use std::time::Duration;

use common::*;
use concord::ErrorKind;
use concord::bytes::Bytes;
use concord::stream::{Completion, ReadErrorKind};
use concord_example_testserver::{Body, Reply, TestServer};
use concord_sdk_ntfy::{
    AccessToken, ApiError, Cache, Event, EventId, EventKind, Events, EventsError, MAX_EVENT_BYTES,
    MessageText, Mode, NtfyError, Priority, Since, Tags, Topic,
};

/// Every event of a reader, then the terminal error if one ended it.
async fn drain(events: &mut Events) -> (Vec<Event>, Option<EventsError>) {
    let mut out = Vec::new();
    while let Some(item) = events.next().await {
        match item {
            Ok(event) => out.push(event),
            Err(error) => {
                assert!(events.next().await.is_none(), "fused after an error");
                return (out, Some(error));
            }
        }
    }
    (out, None)
}

fn sse(body: Body) -> Reply {
    Reply::new(200)
        .header("content-type", SSE_TYPE)
        .with_body(body)
}

fn ndjson(body: Body) -> Reply {
    Reply::new(200)
        .header("content-type", NDJSON_TYPE)
        .with_body(body)
}

fn api_error(error: NtfyError) -> ApiError {
    match error {
        NtfyError::Api(api) => api,
        other => panic!("expected an API error, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------------------------
// Requests on the wire
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn publish_sends_the_text_as_body_and_the_options_as_headers() {
    let server = TestServer::start(|_| Reply::json(200, PUBLISH_RESPONSE)).await;
    let text = "Backup successful \u{1F600}";
    let published = alerts_with_token(&server)
        .publish(MessageText::new(text).unwrap())
        .set_title("Backup")
        .set_priority(Priority::High)
        .set_tags(Tags::new(["warning", "skull"]).unwrap())
        .set_cache(Cache::No)
        .send()
        .await
        .unwrap();

    let request = server.single();
    assert_eq!(request.method, "POST");
    assert_eq!(request.target, "/alerts");
    assert_eq!(request.body, text.as_bytes(), "exact body bytes");
    assert!(!request.chunked);
    assert_eq!(request.header("content-length"), Some("22"));
    assert_eq!(
        request.header("content-type"),
        Some("text/plain; charset=utf-8")
    );
    assert_eq!(request.header("accept"), Some("application/json"));
    assert_eq!(request.header("x-title"), Some("Backup"));
    assert_eq!(request.header("x-priority"), Some("4"));
    assert_eq!(request.header("x-tags"), Some("warning,skull"));
    assert_eq!(request.header("x-cache"), Some("no"));
    assert_eq!(
        request.header("authorization"),
        Some("Bearer synthetic-token-not-real")
    );

    // The documented publish answer.
    assert_eq!(published.id, EventId::new("xE73Iyuabi").unwrap());
    assert_eq!(published.time, 1_673_542_291);
    assert_eq!(published.kind, EventKind::Message);
    assert_eq!(published.topic, "mytopic");
    assert_eq!(published.message.as_deref(), Some("Downloading file..."));
    assert_eq!((published.expires, published.priority), (None, None));
}

#[tokio::test]
async fn publish_without_options_sends_no_optional_header() {
    let server = TestServer::start(|_| Reply::json(200, PUBLISH_RESPONSE)).await;
    alerts(&server)
        .publish(MessageText::new("").unwrap())
        .send()
        .await
        .unwrap();
    let request = server.single();
    for name in [
        "authorization",
        "x-title",
        "x-priority",
        "x-tags",
        "x-cache",
    ] {
        assert_eq!(request.header(name), None, "{name}");
    }
    assert_eq!((request.body_len, request.chunked), (0, false));
}

#[test]
fn subscribe_and_poll_targets_and_accept() {
    let ntfy = concord_sdk_ntfy::Ntfy::new(concord::reqwest::Client::new());
    let alerts = ntfy.topic(Topic::new("alerts").unwrap());
    let url = |p: &concord::reqwest::Request| p.url().as_str().to_owned();

    let sse = alerts.subscribe_sse().prepare().unwrap();
    assert_eq!(url(sse.request()), "https://ntfy.sh/alerts/sse");
    assert_eq!(sse.request().method().as_str(), "GET");
    assert_eq!(sse.request().headers()["accept"], "text/event-stream");

    let json = alerts.subscribe_json().prepare().unwrap();
    assert_eq!(url(json.request()), "https://ntfy.sh/alerts/json");
    assert_eq!(json.request().headers()["accept"], "application/x-ndjson");

    let poll = alerts.poll().prepare().unwrap();
    assert_eq!(url(poll.request()), "https://ntfy.sh/alerts/json?poll=1");

    let id = EventId::new("nFS3knfcQ1xe").unwrap();
    for (since, query) in [
        (Since::All, "since=all"),
        (Since::Latest, "since=latest"),
        (Since::Id(id), "since=nFS3knfcQ1xe"),
        (Since::Time(1_635_528_757), "since=1635528757"),
        (Since::Ago(600), "since=600s"),
    ] {
        let poll = alerts.poll().set_since(since.clone()).prepare().unwrap();
        assert_eq!(
            poll.request().url().query(),
            Some(&*format!("{query}&poll=1"))
        );
        let sse = alerts.subscribe_sse().set_since(since).prepare().unwrap();
        assert_eq!(sse.request().url().query(), Some(query));
    }
}

#[test]
fn domain_values_are_checked_before_any_request() {
    for bad in ["", "a/b", "a,b", "caf\u{e9}", "a b", "..", &"x".repeat(65)] {
        assert!(Topic::new(bad).is_err(), "{bad:?}");
    }
    for good in ["alerts", "my-topic_2", &"x".repeat(64)] {
        assert_eq!(Topic::new(good).unwrap().as_str(), good);
    }

    assert!(MessageText::new("x".repeat(MessageText::MAX_BYTES)).is_ok());
    let long = MessageText::new("x".repeat(MessageText::MAX_BYTES + 1)).unwrap_err();
    assert_eq!(long.bytes, 4097);

    assert_eq!(Tags::new(["a", "b,c"]).unwrap_err().index, 1);
    assert_eq!(Tags::new(["a", ""]).unwrap_err().index, 1);
    assert!(Tags::new(Vec::<String>::new()).is_err());
    assert_eq!(Tags::new(["a", "b"]).unwrap().to_string(), "a,b");

    for bad in ["", "two words", "tab\t", "caf\u{e9}"] {
        assert!(AccessToken::new(bad).is_err(), "{bad:?}");
    }
    for bad in ["", "all", "latest", "none", "a-b", "a_b", &"a".repeat(65)] {
        assert!(EventId::new(bad).is_err(), "{bad:?}");
    }
    assert_eq!(
        Priority::try_from(0),
        Err("a priority is a number from 1 to 5")
    );
    assert_eq!(Priority::try_from(5), Ok(Priority::Max));
}

#[test]
fn the_token_is_withheld_from_every_debug() {
    let ntfy = concord_sdk_ntfy::Ntfy::new(concord::reqwest::Client::new()).with_token(&token());
    let alerts = ntfy.topic(Topic::new("alerts").unwrap());
    let publish = alerts.publish(MessageText::new("hi").unwrap());
    let prepared = publish.clone().prepare().unwrap();
    assert!(prepared.request().headers()["authorization"].is_sensitive());
    let shown = format!("{ntfy:?} {alerts:?} {publish:?} {prepared:?} {:?}", token());
    assert!(!shown.contains(TOKEN), "{shown}");
}

// ---------------------------------------------------------------------------------------------
// SSE: documented example and perturbations
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn the_documented_sse_example_decodes() {
    let server = TestServer::start(|_| sse(cut(SSE_DOCUMENTED, &[]))).await;
    let mut events = alerts(&server).subscribe_sse().send().await.unwrap();
    assert_eq!(events.mode(), Mode::Sse);
    assert!(!events.truncated());
    let (got, error) = drain(&mut events).await;
    assert!(error.is_none());
    let kinds: Vec<_> = got.iter().map(|e| e.kind.clone()).collect();
    assert_eq!(
        kinds,
        [EventKind::Open, EventKind::Message, EventKind::Keepalive]
    );
    assert_eq!(got[1].id.as_str(), "p0M5y6gcCY");
    assert_eq!(got[1].message.as_deref(), Some("Hi!"));
    assert_eq!(got[2].topic, "test"); // as documented
    assert_eq!(events.completion(), Completion::EndOfInput);
    assert_eq!(events.position().records, 3);
    assert_eq!(events.position().bytes, SSE_DOCUMENTED.len() as u64);
}

fn check_perturbed(got: &[Event]) {
    let kinds: Vec<_> = got.iter().map(|e| e.kind.clone()).collect();
    assert_eq!(
        kinds,
        [
            EventKind::Open,
            EventKind::Message,
            EventKind::Keepalive,
            EventKind::Message
        ]
    );
    let first = &got[1];
    assert_eq!(
        first.message.as_deref(),
        Some("Backup successful \u{1F600}")
    );
    assert_eq!(first.title.as_deref(), Some("Backup"));
    assert_eq!(first.tags, ["floppy_disk"]);
    assert_eq!(first.priority, Some(Priority::High));
    assert_eq!(first.expires, Some(1_767_268_801));
    assert_eq!(got[3].message.as_deref(), Some("Disk full"));
}

#[tokio::test]
async fn sse_events_survive_any_chunk_boundary() {
    let f = SSE_PERTURBED;
    let emoji = after(f, "successful ".as_bytes()) + 2; // inside the 4-byte character
    let mid_json = after(f, b"\"title\":\"Ba");
    let between_lfs = after(f, b"\"nested\":[1,2]}}\n"); // the delimiter's second LF follows
    let mid_field = after(f, b"event: keep");
    let bodies = [
        cut(f, &[mid_json, emoji, between_lfs, mid_field]),
        cut(f, &[between_lfs]),
        bytewise(f),
    ];
    for (n, body) in bodies.into_iter().enumerate() {
        let body = std::sync::Mutex::new(Some(body));
        let server = TestServer::start(move |_| sse(body.lock().unwrap().take().unwrap())).await;
        let mut events = alerts(&server).subscribe_sse().send().await.unwrap();
        let (got, error) = drain(&mut events).await;
        assert!(error.is_none(), "body {n}: {error:?}");
        check_perturbed(&got);
        assert_eq!(events.completion(), Completion::EndOfInput, "body {n}");
    }
}

#[tokio::test]
async fn sse_line_ends_byte_order_mark_comments_and_unknown_fields() {
    let server = TestServer::start(|_| sse(bytewise(SSE_CRLF_BOM))).await;
    let mut events = alerts(&server).subscribe_sse().send().await.unwrap();
    let (got, error) = drain(&mut events).await;
    assert!(error.is_none(), "{error:?}");
    let kinds: Vec<_> = got.iter().map(|e| e.kind.clone()).collect();
    assert_eq!(
        kinds,
        [EventKind::Open, EventKind::Message, EventKind::Keepalive]
    );
    // Two `data:` lines are joined with LF (JSON whitespace); `id:` is not the replay cursor.
    assert_eq!(got[1].id.as_str(), "synMsg000003");
    assert_eq!(got[1].message.as_deref(), Some("two data lines"));
    assert_eq!(events.completion(), Completion::EndOfInput);
}

#[tokio::test]
async fn an_unknown_event_kind_is_kept_and_ntfy_omits_the_sse_event_line() {
    // `message_delete` has no `event:` line on the wire; the JSON `event` member decides.
    let body = b"data: {\"id\":\"synDel000001\",\"time\":1767225800,\"event\":\"message_delete\",\"topic\":\"alerts\",\"sequence_id\":\"seq-1\"}\n\nevent: future\ndata: {\"id\":\"synFut000001\",\"time\":1767225801,\"event\":\"future_event\",\"topic\":\"alerts\"}\n\n";
    let server = TestServer::start(move |_| sse(cut(body, &[]))).await;
    let mut events = alerts(&server).subscribe_sse().send().await.unwrap();
    let (got, error) = drain(&mut events).await;
    assert!(error.is_none());
    assert_eq!(got[0].kind, EventKind::MessageDelete);
    assert_eq!(got[0].sequence_id.as_deref(), Some("seq-1"));
    assert_eq!(got[1].kind, EventKind::Other("future_event".into()));
}

#[tokio::test]
async fn a_body_ending_inside_an_event_is_not_a_clean_end() {
    // The connection ends cleanly, but the last event has no terminating empty line.
    let body = after(SSE_DOCUMENTED, b"\"message\":\"Hi!\"}\n");
    let server = TestServer::start(move |_| sse(cut(&SSE_DOCUMENTED[..body], &[]))).await;
    let mut events = alerts(&server).subscribe_sse().send().await.unwrap();
    let (got, error) = drain(&mut events).await;
    assert_eq!(got.len(), 1, "only the open event was complete");
    let error = error.expect("a truncated event is an error");
    assert_eq!(error.kind(), ReadErrorKind::Format);
    // Concord's SSE reader's label: it tells an end inside a line from this end, after the
    // complete `data` line of an event without its blank line.
    assert_eq!(
        error.codec().unwrap().label(),
        "the input ends inside an event, before the blank line that dispatches it"
    );
    assert_eq!(events.completion(), Completion::Failed);
}

#[tokio::test]
async fn a_broken_connection_is_a_transport_error_naming_the_endpoint() {
    let prefix = after(SSE_DOCUMENTED, b"\"message\":\"Hi!\"}\n\n");
    let server = TestServer::start(move |_| {
        sse(Body::FailAfter(vec![Bytes::copy_from_slice(
            &SSE_DOCUMENTED[..prefix],
        )]))
    })
    .await;
    let mut events = alerts(&server).subscribe_sse().send().await.unwrap();
    let (got, error) = drain(&mut events).await;
    assert_eq!(got.len(), 2);
    let error = error.expect("a broken body is an error");
    assert_eq!(error.kind(), ReadErrorKind::Transport);
    assert_eq!(error.operation(), Some("Ntfy.topic.subscribe_sse"));
    assert_eq!(error.position().records, 2);
    assert_eq!(
        error.transport().map(concord::Error::kind),
        Some(ErrorKind::Transport)
    );
    let shown = error.to_string();
    assert!(
        shown.starts_with("Ntfy.topic.subscribe_sse: reading the events failed"),
        "{shown}"
    );
    assert!(!shown.contains("127.0.0.1"), "{shown}");
    assert_eq!(events.completion(), Completion::Failed);
}

#[tokio::test]
async fn malformed_or_foreign_data_fails_without_quoting_it() {
    // Malformed JSON and a foreign shape: the first event, ended by the empty line 2, with the
    // parser's error underneath. Invalid UTF-8 is refused by Concord's SSE reader on the line that
    // holds it (line 1), before any event exists: no event number, no underlying error.
    // The body, the kind, the `event` and `line` positions, whether an underlying error is kept.
    type Case = (
        &'static [u8],
        ReadErrorKind,
        (Option<u64>, Option<u64>),
        bool,
    );
    let first_event = (Some(1), Some(2));
    let cases: [Case; 3] = [
        (
            b"data: {\"SECRETPAYLOAD\n\n",
            ReadErrorKind::Format,
            first_event,
            true,
        ),
        (
            b"data: {\"id\":\"a1\",\"event\":\"message\",\"message\":\"SECRETPAYLOAD\"}\n\n",
            ReadErrorKind::Decode,
            first_event,
            true,
        ),
        (
            b"data: \xff\"SECRETPAYLOAD\"\n\n",
            ReadErrorKind::Format,
            (None, Some(1)),
            false,
        ),
    ];
    for (body, kind, at, underlying) in cases {
        let server = TestServer::start(move |_| sse(cut(body, &[]))).await;
        let mut events = alerts(&server).subscribe_sse().send().await.unwrap();
        let (got, error) = drain(&mut events).await;
        assert!(got.is_empty());
        let error = error.unwrap();
        assert_eq!(error.kind(), kind, "{error}");
        let shown = format!("{error} {error:?}");
        assert!(!shown.contains("SECRETPAYLOAD"), "{shown}");
        let codec = error.codec().unwrap();
        assert_eq!((codec.get("event"), codec.get("line")), at, "{error}");
        assert_eq!(error.source_unredacted().is_some(), underlying, "{error}");
    }
}

#[tokio::test]
async fn a_payload_error_names_its_line_across_chunks() {
    // The SDK counts the lines itself (the SSE reader does not say which line dispatched an
    // event): CR LF, LF and lone CR ends, a CR LF split between two chunks counted once. The
    // second event is ended by the empty line 6.
    let body: &[u8] = b"event: open\r\ndata: {\"id\":\"synOpen00001\",\"time\":1,\"event\":\"open\",\"topic\":\"alerts\"}\r\n\r\n: comment\rdata: {\"broken\r\n\n";
    let mut gated = Vec::new();
    let mut start = 0;
    for end in [
        after(body, b"\"alerts\"}\r"),
        after(body, b"comment\r"),
        after(body, b"broken\r"),
        body.len(),
    ] {
        gated.push((
            Duration::from_millis(20),
            Bytes::copy_from_slice(&body[start..end]),
        ));
        start = end;
    }
    let bodies = [cut(body, &[]), Body::Chunks(gated), bytewise(body)];
    for (n, chunks) in bodies.into_iter().enumerate() {
        let chunks = std::sync::Mutex::new(Some(chunks));
        let server = TestServer::start(move |_| sse(chunks.lock().unwrap().take().unwrap())).await;
        let mut events = alerts(&server).subscribe_sse().send().await.unwrap();
        let (got, error) = drain(&mut events).await;
        assert_eq!(got.len(), 1, "body {n}");
        let error = error.unwrap();
        assert_eq!(error.kind(), ReadErrorKind::Format, "body {n}");
        let codec = error.codec().unwrap();
        assert_eq!(
            (codec.get("event"), codec.get("line")),
            (Some(2), Some(6)),
            "body {n}"
        );
        assert_eq!(events.position().records, 1, "body {n}");
    }
}

#[tokio::test]
async fn an_oversized_event_is_a_limit_error() {
    // One `data:` line longer than the bound, generated lazily (never materialized).
    let server = TestServer::start(move |_| {
        let mut sent_head = false;
        let mut total = 0usize;
        sse(Body::Generate(
            Box::new(move || {
                if !sent_head {
                    sent_head = true;
                    return Some(Bytes::from_static(b"data: \""));
                }
                if total > MAX_EVENT_BYTES {
                    return Some(Bytes::from_static(b"\"\n\n"));
                }
                total += 64 * 1024;
                Some(Bytes::from(vec![b'a'; 64 * 1024]))
            }),
            None,
        ))
    })
    .await;
    let mut events = alerts(&server).subscribe_sse().send().await.unwrap();
    let error = events.next().await.unwrap().unwrap_err();
    assert_eq!(error.kind(), ReadErrorKind::Limit);
    assert!(events.next().await.is_none());
    assert!(events.position().bytes <= MAX_EVENT_BYTES as u64);
}

#[tokio::test]
async fn the_sse_head_is_checked_before_the_body_is_read() {
    for (content_type, accepted) in [
        ("text/event-stream", true),
        ("Text/Event-Stream; charset=\"UTF-8\"", true),
        ("application/json", false),
        ("text/event-stream; charset=iso-8859-1", false),
        ("text/event-stream, text/html", false),
    ] {
        let server = TestServer::start(move |_| {
            Reply::new(200)
                .header("content-type", content_type)
                .body(SSE_DOCUMENTED)
        })
        .await;
        let result = alerts(&server).subscribe_sse().send().await;
        if accepted {
            assert!(result.is_ok(), "{content_type}");
            continue;
        }
        let NtfyError::Request(error) = result.unwrap_err() else {
            panic!("{content_type}: a refused head is Concord's Metadata error");
        };
        assert_eq!(error.kind(), ErrorKind::Metadata, "{content_type}");
        let body = error.into_response().unwrap().bytes().await.unwrap();
        assert_eq!(
            body.as_ref(),
            SSE_DOCUMENTED,
            "the refused response stays unread"
        );
    }
    // No `Content-Type` at all.
    let server = TestServer::start(|_| Reply::new(200).body(SSE_DOCUMENTED)).await;
    let error = alerts(&server).subscribe_sse().send().await.unwrap_err();
    assert_eq!(
        error.request().map(concord::Error::kind),
        Some(ErrorKind::Metadata)
    );
}

#[tokio::test]
async fn a_pending_read_can_be_cancelled_without_losing_the_event() {
    let split = after(SSE_DOCUMENTED, b"\"time\":16355");
    let server = TestServer::start(move |_| {
        sse(Body::Chunks(vec![
            (
                Duration::ZERO,
                Bytes::copy_from_slice(&SSE_DOCUMENTED[..split]),
            ),
            (
                Duration::from_millis(300),
                Bytes::copy_from_slice(&SSE_DOCUMENTED[split..]),
            ),
        ]))
    })
    .await;
    let mut events = alerts(&server).subscribe_sse().send().await.unwrap();
    let pending = tokio::time::timeout(Duration::from_millis(50), events.next()).await;
    assert!(pending.is_err(), "the first event was still incomplete");
    assert_eq!(events.completion(), Completion::Reading);
    let open = events.next().await.unwrap().unwrap();
    assert_eq!(
        (open.kind, open.id.as_str()),
        (EventKind::Open, "weSj9RtNkj")
    );
    let (rest, error) = drain(&mut events).await;
    assert!(error.is_none());
    assert_eq!(rest.len(), 2);
}

// ---------------------------------------------------------------------------------------------
// JSON stream and poll
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn the_documented_json_stream_example_decodes() {
    let mid_line = after(JSON_DOCUMENTED, b"\"message\":\"Disk");
    let server = TestServer::start(move |_| ndjson(cut(JSON_DOCUMENTED, &[mid_line]))).await;
    let mut events = alerts(&server).subscribe_json().send().await.unwrap();
    assert_eq!(events.mode(), Mode::Json);
    let (got, error) = drain(&mut events).await;
    assert!(error.is_none());
    let kinds: Vec<_> = got.iter().map(|e| e.kind.clone()).collect();
    assert_eq!(
        kinds,
        [EventKind::Open, EventKind::Message, EventKind::Keepalive]
    );
    assert_eq!(got[1].message.as_deref(), Some("Disk full"));
    assert_eq!(events.completion(), Completion::EndOfInput);
    assert_eq!(server.single().target, "/alerts/json");
}

#[tokio::test]
async fn a_poll_reports_truncation_and_its_end() {
    let lines = b"{\"id\":\"synMsg000010\",\"time\":1767226000,\"event\":\"message\",\"topic\":\"alerts\",\"message\":\"kept\",\"unknown_future_field\":true}\n\n";
    for truncated in [false, true] {
        let server = TestServer::start(move |_| {
            let reply = ndjson(cut(lines, &[7]));
            if truncated {
                reply.header("x-messages-truncated", "1")
            } else {
                reply
            }
        })
        .await;
        let mut events = alerts(&server)
            .poll()
            .set_since(Since::All)
            .send()
            .await
            .unwrap();
        assert_eq!(events.truncated(), truncated);
        let (got, error) = drain(&mut events).await;
        assert!(error.is_none());
        assert_eq!(got.len(), 1, "the blank line is skipped (NDJSON)");
        assert_eq!(events.completion(), Completion::EndOfInput);
        assert_eq!(server.single().target, "/alerts/json?since=all&poll=1");
    }
}

#[tokio::test]
async fn json_stream_failures_keep_their_kind_and_endpoint() {
    // A truncated last line, a wrong head, a broken connection.
    let server = TestServer::start(|_| ndjson(cut(b"{\"id\":\"a1\",\"time\":1,", &[]))).await;
    let mut events = alerts(&server).poll().send().await.unwrap();
    let (_, error) = drain(&mut events).await;
    let error = error.unwrap();
    assert_eq!(error.kind(), ReadErrorKind::Format);
    assert_eq!(error.operation(), Some("Ntfy.topic.poll"));
    assert!(error.read_error().is_some());
    assert_eq!(events.completion(), Completion::Failed);

    let server = TestServer::start(|_| Reply::json(200, JSON_DOCUMENTED)).await;
    let error = alerts(&server).subscribe_json().send().await.unwrap_err();
    assert_eq!(
        error.request().map(concord::Error::kind),
        Some(ErrorKind::Metadata)
    );

    let server = TestServer::start(|_| {
        ndjson(Body::FailAfter(vec![Bytes::copy_from_slice(
            &JSON_DOCUMENTED[..after(JSON_DOCUMENTED, b"\n")],
        )]))
    })
    .await;
    let mut events = alerts(&server).subscribe_json().send().await.unwrap();
    let (got, error) = drain(&mut events).await;
    assert_eq!(got.len(), 1);
    let error = error.unwrap();
    assert_eq!(error.kind(), ReadErrorKind::Transport);
    assert_eq!(error.operation(), Some("Ntfy.topic.subscribe_json"));
    assert!(!error.to_string().contains("127.0.0.1"));
}

// ---------------------------------------------------------------------------------------------
// Error documents
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn error_documents_are_typed_with_their_metadata() {
    let server = TestServer::start(|request| match request.path() {
        "/alerts" => Reply::json(403, ERROR_FORBIDDEN),
        "/alerts/sse" => Reply::json(401, ERROR_UNAUTHORIZED),
        _ => Reply::json(429, ERROR_RATE_LIMITED).header("retry-after", "30"),
    })
    .await;
    let topic = alerts_with_token(&server);

    let forbidden = api_error(
        topic
            .publish(MessageText::new("hi").unwrap())
            .send()
            .await
            .unwrap_err(),
    );
    assert_eq!(forbidden.status().as_u16(), 403);
    assert_eq!(forbidden.code(), Some(40301));
    assert!(forbidden.is_auth() && !forbidden.is_rate_limited());
    let document = forbidden.document().unwrap();
    assert_eq!((document.http, document.error.as_str()), (403, "forbidden"));
    assert_eq!(
        document.link.as_deref(),
        Some("https://ntfy.sh/docs/publish/#authentication")
    );
    assert_eq!(
        forbidden.to_string(),
        "POST Ntfy.topic.publish: ntfy answered 403 (code 40301)"
    );

    // A stream endpoint's non-2xx is the same typed error (the body is an error document).
    let unauthorized = api_error(topic.subscribe_sse().send().await.unwrap_err());
    assert_eq!(unauthorized.status().as_u16(), 401);
    assert_eq!(unauthorized.code(), Some(40101));
    assert_eq!(
        unauthorized.operation().map(|op| op.name()),
        Some("Ntfy.topic.subscribe_sse")
    );

    let limited = api_error(topic.poll().send().await.unwrap_err());
    assert!(limited.is_rate_limited());
    assert_eq!(limited.code(), Some(42901));
    assert_eq!(limited.retry_after(), Some(Duration::from_secs(30)));
    assert!(!format!("{limited} {limited:?}").contains("too many requests"));
}

#[tokio::test]
async fn retry_after_requires_one_unsigned_decimal_field() {
    for (values, expected) in [
        (vec![], None),
        (vec![" 0 \t"], Some(Duration::ZERO)),
        (vec!["+30"], None),
        (vec!["30", "30"], None),
        (vec!["18446744073709551616"], None),
    ] {
        let server = TestServer::start(move |_| {
            let mut reply = Reply::json(429, ERROR_RATE_LIMITED);
            for value in &values {
                reply = reply.header("retry-after", *value);
            }
            reply
        })
        .await;
        let error = api_error(alerts(&server).poll().send().await.unwrap_err());
        assert_eq!(error.status().as_u16(), 429);
        assert_eq!(error.retry_after(), expected);
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn an_error_body_that_is_not_a_document_keeps_the_status() {
    let server = TestServer::start(|request| match request.path() {
        "/alerts" => Reply::text(502, "text/html", "<html>bad gateway</html>"),
        _ => Reply::json(500, vec![b' '; concord_sdk_ntfy::ERROR_BODY_LIMIT + 1]),
    })
    .await;
    let topic = alerts(&server);
    let html = api_error(
        topic
            .publish(MessageText::new("hi").unwrap())
            .send()
            .await
            .unwrap_err(),
    );
    assert_eq!(html.status().as_u16(), 502);
    assert!(html.document().is_none());
    assert_eq!(
        html.to_string(),
        "POST Ntfy.topic.publish: ntfy answered 502"
    );
    let large = api_error(topic.poll().send().await.unwrap_err());
    assert_eq!(large.status().as_u16(), 500);
    assert!(large.document().is_none());
}

#[tokio::test]
async fn no_response_is_concords_transport_error() {
    let server = TestServer::start(|_| Reply::hang_up()).await;
    let error = alerts(&server).subscribe_sse().send().await.unwrap_err();
    let error = error.request().expect("not an API error");
    assert_eq!(error.kind(), ErrorKind::Transport);
    assert!(!error.to_string().contains("127.0.0.1"));
}
