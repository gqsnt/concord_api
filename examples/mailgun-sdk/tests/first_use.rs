//! Fresh-consumer application written from the README and rustdoc only.
//!
//! Task 1: send a text email with one file attachment and keep the returned ID.
//! Task 2: search and page the domain's events for that ID, then inspect delivery.
//! Every request goes to a loopback fixture; the real Mailgun API is never called.

use concord_example_testserver::{Body, Recorded, Reply, TestServer, paused};
use concord_sdk_mailgun::concord::ErrorKind;
use concord_sdk_mailgun::concord::bytes::Bytes;
use concord_sdk_mailgun::pagination::{PagesError, Resume};
use concord_sdk_mailgun::{
    Attachment, DeliveryStatus, Domain, Error, EventRange, HeaderNumber, HttpClass, InputError,
    LinkError, Mailgun, MailgunDomain, MessageId, PageSize, Region, TextEmail, delivery_status,
};
use futures_util::StreamExt;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, SystemTime};

const KEY: &str = "key-synthetic-0f1e2d3c4b5a69788796a5b4c3d2e1f0";
// base64("api:" + KEY), computed independently with coreutils `base64`.
const BASIC: &str = "Basic YXBpOmtleS1zeW50aGV0aWMtMGYxZTJkM2M0YjVhNjk3ODg3OTZhNWI0YzNkMmUxZjA=";
const DOMAIN: &str = "mg.example.test";
const SENDER: &str = "Quarterly Reports <reports@mg.example.test>";
const ALICE: &str = "alice@example.test";
const BOB: &str = "bob@example.test";
const WIRE_ID: &str = "<20261107090000.1.4F2A8C@mg.example.test>";
const BARE_ID: &str = "20261107090000.1.4F2A8C@mg.example.test";
const BEGIN: u64 = 1_794_042_000; // 2026-11-07T09:00:00Z
const END: u64 = 1_794_045_600; // 2026-11-07T10:00:00Z
const REPORT: &[u8] = b"region,revenue\nnorth,1200\nsouth,980\n";
const CURSOR_2: &str = "W3siYiI6IjE3OTQwNDIwMDAiLCJlIjoiMTc5NDA0NTYwMCJ9LHsicCI6ImYifSwyXQ==";
const CURSOR_3: &str = "W3siYiI6IjE3OTQwNDIwMDAiLCJlIjoiMTc5NDA0NTYwMCJ9LHsicCI6ImYifSw0XQ==";
const CURSOR_4: &str = "W3siYiI6IjE3OTQwNDIwMDAiLCJlIjoiMTc5NDA0NTYwMCJ9LHsicCI6ImYifSw2XQ==";

// ---------------------------------------------------------------- fixtures

fn accepted_body() -> String {
    format!(r#"{{"id":"{WIRE_ID}","message":"Queued. Thank you."}}"#)
}

/// One event in Mailgun's documented shape, with additive fields the SDK must tolerate.
fn event(id: &str, kind: &str, timestamp: &str, recipient: &str, extra: &str) -> String {
    format!(
        r#"{{"id":"{id}","event":"{kind}","timestamp":{timestamp},"log-level":"info",
"recipient":"{recipient}","recipient-domain":"example.test",
"envelope":{{"sender":"reports@mg.example.test","targets":"{recipient}","transport":"smtp"}},
"flags":{{"is-authenticated":true,"is-routed":false,"is-amp":false,"is-test-mode":false}},
"message":{{"headers":{{"to":"{ALICE}, {BOB}","message-id":"{BARE_ID}",
"from":"{SENDER}","subject":"Q3 regional report"}},
"attachments":[{{"filename":"q3-report.csv","content-type":"text/csv","size":37}}],"size":1874}},
"tags":[],"user-variables":{{}}{extra}}}"#
    )
}

fn accepted_event() -> String {
    event(
        "czsjqFATSlC3QtAK-C80nw",
        "accepted",
        "1794042005.123456",
        ALICE,
        "",
    )
}

fn temporary_failure_event() -> String {
    event(
        "Fs7-5t81S2ms-bvF7Bm5ww",
        "failed",
        "1794042065.402011",
        ALICE,
        r#","severity":"temporary","reason":"generic","delivery-status":{"code":421,"message":"4.7.0 Try again later","attempt-no":1}"#,
    )
}

fn delivered_event() -> String {
    event(
        "W3X4JOhFT-OZidZGKKr9iA",
        "delivered",
        "1794042365.901344",
        ALICE,
        r#","delivery-status":{"code":250,"message":"OK","attempt-no":2,"tls":true}"#,
    )
}

fn page(host: &str, items: &[String], next: &str) -> String {
    format!(
        r#"{{"items":[{}],"paging":{{
"first":"http://{host}/v3/{DOMAIN}/events/{CURSOR_2}f",
"last":"http://{host}/v3/{DOMAIN}/events/{CURSOR_2}l",
"next":"{next}",
"previous":"http://{host}/v3/{DOMAIN}/events/{CURSOR_2}p"}}}}"#,
        items.join(",")
    )
}

fn link(host: &str, cursor: &str) -> String {
    format!("http://{host}/v3/{DOMAIN}/events/{cursor}")
}

// ---------------------------------------------------------------- helpers

/// A fixture reply chosen per case.
type ScriptedReply = fn() -> Reply;
/// A `paging.next` value built from the fixture's `host:port`.
type NextLink = fn(&str) -> String;

fn client(server: &TestServer, timeout: Duration) -> MailgunDomain {
    let api = Mailgun::local(&server.url(), KEY, timeout).expect("loopback fixture client");
    api.domain(Domain::new(DOMAIN).expect("valid sending domain"))
}

fn report_email(attachment: Attachment) -> TextEmail {
    TextEmail::new(
        SENDER,
        vec![ALICE.into(), BOB.into()],
        "Q3 regional report",
        "Hello,\n\nThe Q3 regional figures are attached.\n",
        attachment,
    )
    .expect("valid email")
}

fn buffered_report() -> Attachment {
    Attachment::bytes("q3-report.csv", "text/csv", REPORT).expect("valid attachment")
}

fn search(
    domain: &MailgunDomain,
    id: &MessageId,
    limit: u16,
) -> concord_sdk_mailgun::MailgunDomainEvents {
    domain
        .events(EventRange::new(BEGIN, END).expect("finite range"))
        .set_message_id(id.clone())
        .set_recipient(ALICE)
        .set_limit(PageSize::new(limit).expect("page size"))
}

fn cap(pages: usize) -> NonZeroUsize {
    NonZeroUsize::new(pages).expect("nonzero cap")
}

/// No replay arrives within a window after the call returned.
async fn assert_no_second_request(server: &TestServer, expected: usize) {
    let replayed = server
        .wait_until(Duration::from_millis(400), |s| {
            s.connections() > expected || s.requests().len() > expected
        })
        .await;
    assert!(!replayed, "the SDK sent a request the caller did not make");
    assert_eq!(server.requests().len(), expected);
}

fn assert_redacted(rendered: &str) {
    for secret in [
        KEY,
        "YXBpOmtleS1zeW50aGV0aWM",
        BARE_ID,
        ALICE,
        CURSOR_2,
        "127.0.0.1",
    ] {
        assert!(
            !rendered.contains(secret),
            "rendered output discloses a secret: {rendered}"
        );
    }
}

#[derive(Debug)]
struct Part {
    headers: String,
    body: Vec<u8>,
}

impl Part {
    fn name(&self) -> &str {
        let start = self.headers.find("name=\"").expect("named part") + 6;
        let end = self.headers[start..].find('"').expect("closed name") + start;
        &self.headers[start..end]
    }
}

fn find(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    haystack[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|i| i + from)
}

/// Parse a multipart/form-data body with the boundary from its Content-Type.
fn multipart(request: &Recorded) -> Vec<Part> {
    let content_type = request.header("content-type").expect("content-type");
    let boundary = content_type
        .strip_prefix("multipart/form-data; boundary=")
        .expect("multipart/form-data content type");
    let delimiter = format!("\r\n--{boundary}");
    // The first delimiter has no leading CRLF.
    let mut body = b"\r\n".to_vec();
    body.extend_from_slice(&request.body);
    let mut parts = Vec::new();
    let mut at = find(&body, delimiter.as_bytes(), 0).expect("first delimiter") + delimiter.len();
    loop {
        if body[at..].starts_with(b"--") {
            break;
        }
        assert!(
            body[at..].starts_with(b"\r\n"),
            "delimiter line ends with CRLF"
        );
        let head_start = at + 2;
        let head_end = find(&body, b"\r\n\r\n", head_start).expect("part headers end");
        let next = find(&body, delimiter.as_bytes(), head_end + 4).expect("next delimiter");
        parts.push(Part {
            headers: String::from_utf8(body[head_start..head_end].to_vec()).expect("ascii headers"),
            body: body[head_end + 4..next].to_vec(),
        });
        at = next + delimiter.len();
    }
    parts
}

// ======================================================== Task 1: send and keep the ID

#[tokio::test]
async fn send_report_with_file_attachment_and_keep_message_id() {
    let server = TestServer::start(|_: &Recorded| {
        Reply::json(200, accepted_body())
            .header("x-ratelimit-limit", "300")
            .header("x-ratelimit-remaining", "299")
            .header("x-ratelimit-reset", "1794042060000")
    })
    .await;
    let domain = client(&server, Duration::from_secs(10));

    // The application keeps the report on disk; the wire filename is chosen separately.
    let path = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("first-use-2026-q3.csv");
    std::fs::write(&path, REPORT).expect("write local report");
    let file = Attachment::from_file(&path, "q3-report.csv", "text/csv").expect("read report");

    let accepted = domain
        .send_message(report_email(file))
        .send()
        .await
        .expect("accepted");

    // Completion: one 200 acknowledgement, which means queued, not delivered.
    assert_eq!(accepted.meta.status(), 200);
    assert_eq!(accepted.value.id.as_str(), WIRE_ID);
    assert_eq!(accepted.value.message, "Queued. Thank you.");
    assert_eq!(accepted.meta.rate_limit(), HeaderNumber::Value(300));
    assert_eq!(accepted.meta.rate_remaining(), HeaderNumber::Value(299));
    assert_eq!(
        accepted.meta.rate_reset_millis(),
        HeaderNumber::Value(1_794_042_060_000)
    );
    assert_eq!(accepted.meta.retry_after(SystemTime::now()), Ok(None));
    // Observed: Display is the exact provider spelling, angle brackets included.
    assert_eq!(accepted.value.id.to_string(), WIRE_ID);
    assert_redacted(&format!("{accepted:?}"));

    // The retained value round-trips through application storage.
    let stored = accepted.value.id.as_str().to_owned();
    assert_eq!(
        MessageId::new(stored).expect("stored ID"),
        accepted.value.id
    );

    // Wire request.
    let request = server.single();
    assert_eq!(request.method, "POST");
    assert_eq!(request.target, "/v3/mg.example.test/messages");
    assert_eq!(request.header("authorization"), Some(BASIC));
    assert_eq!(request.header("accept"), Some("application/json"));
    assert!(!request.chunked, "a buffered attachment has a known length");
    assert_eq!(
        request.header("content-length"),
        Some(request.body_len.to_string().as_str())
    );
    let parts = multipart(&request);
    let names: Vec<&str> = parts.iter().map(Part::name).collect();
    assert_eq!(names, ["from", "to", "to", "subject", "text", "attachment"]);
    assert_eq!(parts[0].body, SENDER.as_bytes());
    assert_eq!(parts[1].body, ALICE.as_bytes());
    assert_eq!(parts[2].body, BOB.as_bytes());
    assert_eq!(parts[3].body, b"Q3 regional report");
    assert_eq!(
        parts[4].body,
        b"Hello,\n\nThe Q3 regional figures are attached.\n"
    );
    assert!(
        parts[5].headers.contains(r#"filename="q3-report.csv""#),
        "{}",
        parts[5].headers
    );
    assert!(
        parts[5].headers.contains("Content-Type: text/csv"),
        "{}",
        parts[5].headers
    );
    assert!(
        !parts[5].headers.contains("first-use-2026-q3"),
        "local path is not sent"
    );
    assert_eq!(parts[5].body, REPORT);
    assert_no_second_request(&server, 1).await;
}

#[tokio::test]
async fn attachment_that_contains_the_boundary_text_is_framed_correctly() {
    let server = TestServer::start(|_: &Recorded| Reply::json(200, accepted_body())).await;
    let domain = client(&server, Duration::from_secs(10));
    // A forwarded multipart message saved as an attachment: it quotes Concord's boundary.
    let tricky = b"--concord-boundary-0\r\nContent-Disposition: form-data; name=\"to\"\r\n\r\nmallory@example.test\r\n--concord-boundary-0--\r\n".to_vec();
    let file = Attachment::bytes("forwarded.eml.txt", "text/plain", tricky.clone()).unwrap();
    domain
        .send_message(report_email(file))
        .send()
        .await
        .expect("accepted");
    let request = server.single();
    let content_type = request.header("content-type").unwrap();
    assert_ne!(
        content_type,
        "multipart/form-data; boundary=concord-boundary-0"
    );
    let parts = multipart(&request);
    let names: Vec<&str> = parts.iter().map(Part::name).collect();
    assert_eq!(names, ["from", "to", "to", "subject", "text", "attachment"]);
    assert_eq!(parts[5].body, tricky);
}

/// Send one email against a scripted reply and return the error with the requests seen.
async fn send_failure(reply: fn() -> Reply, timeout: Duration) -> (Error, TestServer) {
    let server = TestServer::start(move |_: &Recorded| reply()).await;
    let domain = client(&server, timeout);
    let error = domain
        .send_message(report_email(buffered_report()))
        .send()
        .await
        .expect_err("scripted failure");
    (error, server)
}

#[tokio::test]
async fn provider_statuses_are_typed_and_never_replayed() {
    // 400 with Mailgun's documented JSON error document.
    let (error, server) = send_failure(
        || Reply::json(400, r#"{"message":"'to' parameter is not a valid address. please check documentation"}"#),
        Duration::from_secs(10),
    )
    .await;
    assert_redacted(&format!("{error:?} {error}"));
    assert!(!format!("{error:?} {error}").contains("not a valid address"));
    let Error::Provider(provider) = error else {
        panic!("expected Provider, got {error:?}")
    };
    assert_eq!(provider.class(), HttpClass::BadRequest);
    assert_eq!(provider.meta.status(), 400);
    let problem = provider.document.as_ref().expect("JSON error document");
    assert!(
        problem
            .message
            .starts_with("'to' parameter is not a valid address")
    );
    assert_no_second_request(&server, 1).await;

    // 401: Mailgun documents no body; its plain-text answer is not JSON.
    let (error, server) = send_failure(
        || Reply::text(401, "text/plain", "Forbidden"),
        Duration::from_secs(10),
    )
    .await;
    let Error::Provider(provider) = error else {
        panic!("expected Provider, got {error:?}")
    };
    assert_eq!(provider.class(), HttpClass::Unauthorized);
    assert_eq!(provider.meta.status(), 401);
    assert!(
        provider.document.is_err(),
        "status survives an unusable error document"
    );
    assert_no_second_request(&server, 1).await;

    // 429 with hints: the hint is readable and nothing is scheduled.
    let (error, server) = send_failure(
        || {
            Reply::json(429, r#"{"message":"Too many requests"}"#)
                .header("retry-after", "30")
                .header("x-ratelimit-remaining", "0")
        },
        Duration::from_secs(10),
    )
    .await;
    let Error::Provider(provider) = error else {
        panic!("expected Provider, got {error:?}")
    };
    assert_eq!(provider.class(), HttpClass::RateLimited);
    assert_eq!(
        provider.meta.retry_after(SystemTime::now()),
        Ok(Some(Duration::from_secs(30)))
    );
    assert_eq!(provider.meta.rate_remaining(), HeaderNumber::Value(0));
    assert_no_second_request(&server, 1).await;

    // 503 from a load balancer with an HTML page.
    let (error, server) = send_failure(
        || {
            Reply::text(
                503,
                "text/html",
                "<html><body>Service Unavailable</body></html>",
            )
        },
        Duration::from_secs(10),
    )
    .await;
    let Error::Provider(provider) = error else {
        panic!("expected Provider, got {error:?}")
    };
    assert_eq!(provider.class(), HttpClass::Server);
    assert!(provider.document.is_err());
    assert_no_second_request(&server, 1).await;

    // A redirect is not followed.
    let (error, server) = send_failure(
        || Reply::new(302).header("location", "http://127.0.0.1:9/v3/mg.example.test/messages"),
        Duration::from_secs(10),
    )
    .await;
    let Error::Provider(provider) = error else {
        panic!("expected Provider, got {error:?}")
    };
    assert_eq!(provider.class(), HttpClass::Unexpected);
    assert_no_second_request(&server, 1).await;
}

#[tokio::test]
async fn ambiguous_sends_are_reported_and_never_replayed() {
    // How the 500 ms timeout may fire, on the paused clock: never (the answer ends the send), once
    // the server holds the send, or once it has written the head of a stalled body.
    #[derive(Clone, Copy, PartialEq)]
    enum Late {
        Never,
        Head,
        Body,
    }
    let cases: [(&str, ScriptedReply, ErrorKind, Late); 5] = [
        (
            "connection closed after the request",
            Reply::hang_up,
            ErrorKind::Transport,
            Late::Never,
        ),
        (
            "no response head before the timeout",
            || Reply::json(200, accepted_body()).delay(Duration::from_secs(5)),
            ErrorKind::Transport,
            Late::Head,
        ),
        (
            "200 body stalls",
            || {
                Reply::new(200)
                    .header("content-type", "application/json")
                    .with_body(Body::Stall)
            },
            ErrorKind::Transport,
            Late::Body,
        ),
        (
            "200 body is truncated JSON",
            || Reply::json(200, r#"{"id":"<2026110709"#),
            ErrorKind::Decode,
            Late::Never,
        ),
        (
            "200 body has no id",
            || Reply::json(200, r#"{"message":"Queued. Thank you."}"#),
            ErrorKind::Decode,
            Late::Never,
        ),
    ];
    for (label, reply, kind, late) in cases {
        let timeout = Duration::from_millis(500);
        let server = TestServer::start(move |_: &Recorded| reply()).await;
        let domain = client(&server, timeout);
        tokio::time::pause();
        let send = domain.send_message(report_email(buffered_report())).send();
        let error = match late {
            Late::Never => paused::drive(send).await,
            Late::Head => paused::time_out(&server, |_| true, timeout, send).await,
            Late::Body => paused::time_out_in_body(&server, |_| true, timeout, send).await,
        }
        .expect_err("scripted failure");
        tokio::time::resume();
        assert_redacted(&format!("{error:?} {error}"));
        let Error::AmbiguousSend(inner) = &error else {
            panic!("{label}: expected AmbiguousSend, got {error:?}")
        };
        assert_eq!(inner.kind(), kind, "{label}");
        assert_eq!(
            error.to_string(),
            "Mailgun send outcome unknown; do not automatically replay"
        );
        assert!(error.concord_error().is_some(), "{label}: diagnostic kept");
        assert_no_second_request(&server, 1).await;
    }
}

#[tokio::test]
async fn streamed_attachment_is_sent_once_and_never_rewound() {
    // Success: the stream is consumed exactly by the one send.
    let server = TestServer::start(|_: &Recorded| Reply::json(200, accepted_body())).await;
    let domain = client(&server, Duration::from_secs(10));
    let polls = Arc::new(AtomicUsize::new(0));
    let counter = polls.clone();
    let chunks = vec![
        Ok::<_, std::io::Error>(Bytes::from_static(b"region,revenue\n")),
        Ok(Bytes::from_static(b"north,1200\nsouth,980\n")),
    ];
    let source = futures_util::stream::iter(chunks).inspect(move |_| {
        counter.fetch_add(1, Ordering::SeqCst);
    });
    let file = Attachment::stream("q3-report.csv", "text/csv", source).expect("lazy attachment");
    let email = report_email(file);
    assert_eq!(
        polls.load(Ordering::SeqCst),
        0,
        "construction does not poll the source"
    );
    let accepted = domain.send_message(email).send().await.expect("accepted");
    assert_eq!(accepted.value.id.as_str(), WIRE_ID);
    assert_eq!(polls.load(Ordering::SeqCst), 2);
    let request = server.single();
    assert!(request.chunked, "a lazy attachment is sent chunked");
    assert_eq!(request.header("content-length"), None);
    let parts = multipart(&request);
    let names: Vec<&str> = parts.iter().map(Part::name).collect();
    assert_eq!(names, ["from", "to", "to", "subject", "text", "attachment"]);
    assert!(
        parts[5].headers.contains(r#"filename="q3-report.csv""#),
        "{}",
        parts[5].headers
    );
    assert!(
        parts[5].headers.contains("Content-Type: text/csv"),
        "{}",
        parts[5].headers
    );
    assert_eq!(parts[5].body, REPORT);

    // prepare() alone polls nothing.
    let polls = Arc::new(AtomicUsize::new(0));
    let counter = polls.clone();
    let source = futures_util::stream::iter([Ok::<_, std::io::Error>(Bytes::from_static(REPORT))])
        .inspect(move |_| {
            counter.fetch_add(1, Ordering::SeqCst);
        });
    let file = Attachment::stream("q3-report.csv", "text/csv", source).unwrap();
    let prepared = domain
        .send_message(report_email(file))
        .prepare()
        .expect("prepared");
    drop(prepared);
    assert_eq!(
        polls.load(Ordering::SeqCst),
        0,
        "prepare does not poll the source"
    );

    // The source fails after its first chunk: the send may or may not have taken effect.
    let server = TestServer::start(|_: &Recorded| Reply::json(200, accepted_body())).await;
    let domain = client(&server, Duration::from_secs(10));
    let chunks = vec![
        Ok(Bytes::from_static(b"region,revenue\n")),
        Err(std::io::Error::other("disk read failed")),
    ];
    let file = Attachment::stream(
        "q3-report.csv",
        "text/csv",
        futures_util::stream::iter(chunks),
    )
    .unwrap();
    let error = domain
        .send_message(report_email(file))
        .send()
        .await
        .expect_err("source failed");
    assert!(matches!(error, Error::AmbiguousSend(_)), "{error:?}");
    assert_redacted(&format!("{error:?} {error}"));
    let replayed = server
        .wait_until(Duration::from_millis(400), |s| s.connections() > 1)
        .await;
    assert!(!replayed, "a failed stream is not replayed");
    // One connection carried a partial body; the fixture never saw a complete request,
    // so it recorded none. The provider could still have acted on what it received.
    assert_eq!(server.connections(), 1);
    assert!(server.requests().is_empty());
    assert!(
        server.request_body_bytes() > 0,
        "part of the message reached the server"
    );

    // A provider refusal of a streamed send is still a single attempt.
    let server =
        TestServer::start(|_: &Recorded| Reply::json(503, r#"{"message":"Service Unavailable"}"#))
            .await;
    let domain = client(&server, Duration::from_secs(10));
    let source = futures_util::stream::iter([Ok::<_, std::io::Error>(Bytes::from_static(REPORT))]);
    let file = Attachment::stream("q3-report.csv", "text/csv", source).unwrap();
    let error = domain
        .send_message(report_email(file))
        .send()
        .await
        .expect_err("503");
    let Error::Provider(provider) = error else {
        panic!("expected Provider, got {error:?}")
    };
    assert_eq!(provider.class(), HttpClass::Server);
    assert_no_second_request(&server, 1).await;
}

#[test]
fn local_refusals_happen_before_any_io() {
    let attachment = || buffered_report();
    let no_recipients = TextEmail::new(SENDER, vec![], "Q3", "text", attachment());
    assert!(
        matches!(no_recipients, Err(Error::Input(InputError::Message))),
        "{no_recipients:?}"
    );
    let header_injection = TextEmail::new(
        SENDER,
        vec![ALICE.into()],
        "Q3\r\nBcc: eve@example.test",
        "text",
        attachment(),
    );
    assert!(
        matches!(header_injection, Err(Error::Input(InputError::Message))),
        "{header_injection:?}"
    );
    let slash = Attachment::bytes("../q3-report.csv", "text/csv", REPORT);
    assert!(
        matches!(slash, Err(Error::Input(InputError::Attachment))),
        "{slash:?}"
    );
    let missing = Attachment::from_file(
        "/nonexistent/first-use/report.csv",
        "report.csv",
        "text/csv",
    );
    assert!(
        matches!(missing, Err(Error::Input(InputError::File))),
        "{missing:?}"
    );
    let remote = Mailgun::local("https://api.mailgun.net", KEY, Duration::from_secs(5));
    assert!(
        matches!(remote, Err(Error::Input(InputError::BaseUrl))),
        "{remote:?}"
    );
    let zero = Mailgun::local("http://127.0.0.1:9", KEY, Duration::ZERO);
    assert!(
        matches!(zero, Err(Error::Input(InputError::Timeout))),
        "{zero:?}"
    );
    let reversed = EventRange::new(END, BEGIN);
    assert!(
        matches!(reversed, Err(Error::Input(InputError::Search))),
        "{reversed:?}"
    );
    let too_large = PageSize::new(301);
    assert!(
        matches!(too_large, Err(Error::Input(InputError::Search))),
        "{too_large:?}"
    );
}

#[test]
fn regional_clients_target_the_regional_origin_without_io() {
    assert_eq!(Region::Us.base_url(), "https://api.mailgun.net");
    assert_eq!(Region::Eu.base_url(), "https://api.eu.mailgun.net");
    // Construction does no I/O; prepare() is used only to inspect the built request.
    let api = Mailgun::connect(Region::Eu, KEY, Duration::from_secs(30)).expect("EU client");
    let domain = api.domain(Domain::new(DOMAIN).unwrap());
    let prepared = domain
        .events(EventRange::new(BEGIN, END).unwrap())
        .prepare()
        .expect("built request");
    let url = prepared.request().url().as_str();
    assert!(
        url.starts_with("https://api.eu.mailgun.net/v3/mg.example.test/events?"),
        "{url}"
    );
    assert_eq!(
        prepared
            .request()
            .headers()
            .get("authorization")
            .map(|v| v.to_str().unwrap_or_default()),
        Some(BASIC)
    );
    let prepared = domain
        .send_message(report_email(buffered_report()))
        .prepare()
        .unwrap();
    assert_eq!(
        prepared.request().url().as_str(),
        "https://api.eu.mailgun.net/v3/mg.example.test/messages"
    );
    assert!(matches!(
        Mailgun::connect(Region::Us, "", Duration::from_secs(30)),
        Err(Error::Input(InputError::Credential))
    ));
}

// ======================================================== Task 2: page events, inspect delivery

/// The connected workflow: send, keep the ID, then follow three provider pages.
#[tokio::test]
async fn send_then_page_events_and_inspect_delivery() {
    let server = TestServer::start(|request: &Recorded| {
        let host = &request.host;
        match (request.method.as_str(), request.path()) {
            ("POST", "/v3/mg.example.test/messages") => Reply::json(200, accepted_body()),
            ("GET", "/v3/mg.example.test/events") => Reply::json(
                200,
                page(
                    host,
                    &[accepted_event(), temporary_failure_event()],
                    &link(host, CURSOR_2),
                ),
            )
            .header("x-ratelimit-remaining", "298"),
            ("GET", path) if path.ends_with(CURSOR_2) => {
                Reply::json(200, page(host, &[delivered_event()], &link(host, CURSOR_3)))
            }
            ("GET", path) if path.ends_with(CURSOR_3) => {
                Reply::json(200, page(host, &[], &link(host, CURSOR_4)))
            }
            _ => Reply::json(404, r#"{"message":"not found"}"#),
        }
    })
    .await;
    let domain = client(&server, Duration::from_secs(10));

    let accepted = domain
        .send_message(report_email(buffered_report()))
        .send()
        .await
        .expect("accepted");
    let message_id = accepted.value.id;

    let events = search(&domain, &message_id, 2)
        .pages(cap(10))
        .expect("traversal")
        .items()
        .collect_complete()
        .await
        .expect("complete window");

    let kinds: Vec<&str> = events.iter().map(|e| e.event.as_str()).collect();
    assert_eq!(kinds, ["accepted", "failed", "delivered"]);
    assert_eq!(events[1].severity.as_deref(), Some("temporary"));
    assert_eq!(events[0].timestamp, 1_794_042_005.123_456);
    assert_eq!(
        events[2]
            .message
            .as_ref()
            .and_then(|m| m.headers.as_ref())
            .and_then(|h| h.message_id.as_deref()),
        Some(BARE_ID)
    );
    assert_eq!(
        events[2].flags.as_ref().and_then(|f| f.is_test_mode),
        Some(false)
    );
    assert_redacted(&format!("{events:?}"));

    // Delivery inspection over the complete window, and over the first page alone.
    assert_eq!(
        delivery_status(&message_id, ALICE, &events),
        DeliveryStatus::Delivered
    );
    assert_eq!(
        delivery_status(&message_id, ALICE, &events[..2]),
        DeliveryStatus::TemporaryFailure
    );
    assert_eq!(
        delivery_status(&message_id, ALICE, &events[..1]),
        DeliveryStatus::Accepted
    );
    assert_eq!(
        delivery_status(&message_id, BOB, &events),
        DeliveryStatus::Unknown
    );
    let other = MessageId::new("<20261107090000.2.77AA01@mg.example.test>").unwrap();
    assert_eq!(
        delivery_status(&other, ALICE, &events),
        DeliveryStatus::Unknown
    );

    // Wire: one POST, then three GETs (the empty terminal page counts).
    let requests = server.requests();
    assert_eq!(requests.len(), 4);
    for request in &requests {
        assert_eq!(request.header("authorization"), Some(BASIC));
    }
    let first = &requests[1];
    assert_eq!(first.method, "GET");
    assert_eq!(first.path(), "/v3/mg.example.test/events");
    // Observed order and spelling. The message ID is sent exactly as the Messages API
    // returned it, angle brackets included (percent-encoded).
    assert_eq!(
        first.target,
        "/v3/mg.example.test/events?begin=1794042000&end=1794045600&ascending=yes&limit=2\
         &message-id=%3C20261107090000.1.4F2A8C%40mg.example.test%3E&recipient=alice%40example.test"
    );
    assert_eq!(first.query_all("message-id"), [WIRE_ID]);
    // Continuations are the provider's URLs verbatim: no original query appended.
    assert_eq!(
        requests[2].target,
        format!("/v3/mg.example.test/events/{CURSOR_2}")
    );
    assert_eq!(
        requests[3].target,
        format!("/v3/mg.example.test/events/{CURSOR_3}")
    );
    assert_no_second_request(&server, 4).await;
}

#[tokio::test]
async fn failed_page_keeps_prefix_and_resumes_only_when_asked() {
    let page_two_calls = Arc::new(AtomicUsize::new(0));
    let calls = page_two_calls.clone();
    let server = TestServer::start(move |request: &Recorded| {
        let host = &request.host;
        match request.path() {
            "/v3/mg.example.test/events" => Reply::json(
                200,
                page(
                    host,
                    &[accepted_event(), temporary_failure_event()],
                    &link(host, CURSOR_2),
                ),
            ),
            path if path.ends_with(CURSOR_2) => {
                if calls.fetch_add(1, Ordering::SeqCst) == 0 {
                    Reply::json(502, r#"{"message":"Bad Gateway"}"#)
                } else {
                    Reply::json(200, page(host, &[delivered_event()], &link(host, CURSOR_3)))
                }
            }
            path if path.ends_with(CURSOR_3) => {
                Reply::json(200, page(host, &[], &link(host, CURSOR_4)))
            }
            _ => Reply::json(404, r#"{"message":"not found"}"#),
        }
    })
    .await;
    let domain = client(&server, Duration::from_secs(10));
    let message_id = MessageId::new(WIRE_ID).unwrap(); // loaded from application storage
    let query = search(&domain, &message_id, 2);

    let incomplete = query
        .clone()
        .pages(cap(10))
        .unwrap()
        .items()
        .collect_complete()
        .await
        .expect_err("page two failed");
    assert_eq!(
        incomplete.collected.len(),
        2,
        "page one is kept as a prefix"
    );
    assert_redacted(&format!("{incomplete:?} {incomplete}"));
    // Partial evidence only: the prefix says "temporary failure", the window is not complete.
    assert_eq!(
        delivery_status(&message_id, ALICE, &incomplete.collected),
        DeliveryStatus::TemporaryFailure
    );
    let resume = incomplete.error.resume().expect("resume position").clone();
    assert!(matches!(resume, Resume::At(_)));
    let Some(Error::Provider(provider)) = incomplete.error.provider_error() else {
        panic!("expected a provider failure, got {:?}", incomplete.error)
    };
    assert_eq!(provider.class(), HttpClass::Server);
    assert_eq!(provider.meta.status(), 502);
    assert!(matches!(incomplete.error, PagesError::Provider { .. }));
    // No automatic retry of the failed read.
    assert_no_second_request(&server, 2).await;

    // Explicit resumption with the same generated search.
    let rest = query
        .pages_from(resume, cap(10))
        .unwrap()
        .items()
        .collect_complete()
        .await
        .expect("resumed to the end");
    assert_eq!(rest.len(), 1);
    let mut window = incomplete.collected;
    window.extend(rest);
    assert_eq!(
        delivery_status(&message_id, ALICE, &window),
        DeliveryStatus::Delivered
    );
    let requests = server.requests();
    assert_eq!(requests.len(), 4);
    assert_eq!(
        requests[2].target, requests[1].target,
        "the failed page is requested again, verbatim"
    );
    assert_eq!(
        requests[2].target,
        format!("/v3/mg.example.test/events/{CURSOR_2}")
    );
    assert_eq!(
        requests[3].target,
        format!("/v3/mg.example.test/events/{CURSOR_3}")
    );
}

#[tokio::test]
async fn resume_position_refuses_a_different_search() {
    let server = TestServer::start(|request: &Recorded| {
        let host = &request.host;
        match request.path() {
            "/v3/mg.example.test/events" => {
                Reply::json(200, page(host, &[accepted_event()], &link(host, CURSOR_2)))
            }
            _ => Reply::json(503, r#"{"message":"Service Unavailable"}"#),
        }
    })
    .await;
    let domain = client(&server, Duration::from_secs(10));
    let message_id = MessageId::new(WIRE_ID).unwrap();
    let incomplete = search(&domain, &message_id, 2)
        .pages(cap(10))
        .unwrap()
        .items()
        .collect_complete()
        .await
        .expect_err("page two failed");
    let resume = incomplete.error.resume().unwrap().clone();
    // Same domain, different recipient: the cursor belongs to another search.
    let other = search(&domain, &message_id, 2).set_recipient(BOB);
    // Refused synchronously by pages_from, before any request.
    let failure = other
        .pages_from(resume, cap(10))
        .expect_err("foreign cursor");
    assert!(
        matches!(failure, Error::Link(LinkError::Context)),
        "{failure:?}"
    );
    assert_no_second_request(&server, 2).await;
}

#[tokio::test]
async fn untrusted_next_links_are_refused_before_dispatch() {
    let cases: [(&str, NextLink); 3] = [
        ("real provider origin", |_| {
            format!("https://api.mailgun.net/v3/{DOMAIN}/events/{CURSOR_2}")
        }),
        ("another domain's events", |host| {
            format!("http://{host}/v3/other.example.test/events/{CURSOR_2}")
        }),
        ("credentials in the link", |host| {
            format!("http://api:stolen@{host}/v3/{DOMAIN}/events/{CURSOR_2}")
        }),
    ];
    for (label, next) in cases {
        let server = TestServer::start(move |request: &Recorded| {
            let host = &request.host;
            Reply::json(200, page(host, &[accepted_event()], &next(host)))
        })
        .await;
        let domain = client(&server, Duration::from_secs(10));
        let message_id = MessageId::new(WIRE_ID).unwrap();
        let incomplete = search(&domain, &message_id, 2)
            .pages(cap(10))
            .unwrap()
            .items()
            .collect_complete()
            .await
            .expect_err("link refused");
        // The page carrying the bad link fails as a whole: its own (valid) item is not
        // kept, and the position is that page, not a later one.
        assert!(incomplete.collected.is_empty(), "{label}");
        assert!(
            matches!(incomplete.error.resume(), Some(Resume::Start)),
            "{label}"
        );
        assert!(
            matches!(
                incomplete.error.provider_error(),
                Some(Error::Link(LinkError::Untrusted))
            ),
            "{label}: {:?}",
            incomplete.error
        );
        assert_redacted(&format!("{incomplete:?} {incomplete}"));
        assert_no_second_request(&server, 1).await;
    }
}

#[tokio::test]
async fn a_next_link_back_to_a_fetched_page_is_a_cycle() {
    let server = TestServer::start(|request: &Recorded| {
        let host = &request.host;
        if request.path().ends_with(CURSOR_2) {
            // A provider bug: page two points at itself.
            Reply::json(200, page(host, &[delivered_event()], &link(host, CURSOR_2)))
        } else {
            Reply::json(200, page(host, &[accepted_event()], &link(host, CURSOR_2)))
        }
    })
    .await;
    let domain = client(&server, Duration::from_secs(10));
    let message_id = MessageId::new(WIRE_ID).unwrap();
    let incomplete = search(&domain, &message_id, 2)
        .pages(cap(10))
        .unwrap()
        .items()
        .collect_complete()
        .await
        .expect_err("cycle");
    assert!(
        matches!(
            incomplete.error.provider_error(),
            Some(Error::Link(LinkError::Cycle))
        ),
        "{:?}",
        incomplete.error
    );
    // Observed: unlike an untrusted link, the page carrying the cycle is kept. The
    // refusal is the duplicate request, which is never sent.
    assert_eq!(incomplete.collected.len(), 2);
    assert!(matches!(incomplete.error.resume(), Some(Resume::At(_))));
    assert_no_second_request(&server, 2).await;
    // The prefix says "delivered", but it is a prefix: the window was not completed.
    assert_eq!(
        delivery_status(&message_id, ALICE, &incomplete.collected),
        DeliveryStatus::Delivered
    );
    // Resuming at the cycle position is a fresh traversal: it fetches page two again
    // (its item comes back a second time; no deduplication) and meets the same cycle.
    let resume = incomplete.error.resume().unwrap().clone();
    let again = search(&domain, &message_id, 2)
        .pages_from(resume, cap(10))
        .unwrap()
        .items()
        .collect_complete()
        .await
        .expect_err("same cycle");
    assert_eq!(again.collected.len(), 1);
    assert!(matches!(
        again.error.provider_error(),
        Some(Error::Link(LinkError::Cycle))
    ));
    assert_eq!(server.requests().len(), 3);
    assert_eq!(server.requests()[2].target, server.requests()[1].target);
}

#[tokio::test]
async fn page_cap_and_bad_pages_are_not_completion() {
    // Cap reached while the provider has a next link.
    let server = TestServer::start(|request: &Recorded| {
        let host = &request.host;
        Reply::json(
            200,
            page(
                host,
                &[accepted_event()],
                &link(host, &format!("{CURSOR_2}{}", request.target.len())),
            ),
        )
    })
    .await;
    let domain = client(&server, Duration::from_secs(10));
    let message_id = MessageId::new(WIRE_ID).unwrap();
    let incomplete = search(&domain, &message_id, 1)
        .pages(cap(2))
        .unwrap()
        .items()
        .collect_complete()
        .await
        .expect_err("bounded");
    assert!(
        matches!(incomplete.error, PagesError::Bounded { .. }),
        "{:?}",
        incomplete.error
    );
    assert_eq!(incomplete.collected.len(), 2);
    assert!(matches!(incomplete.error.resume(), Some(Resume::At(_))));
    assert_no_second_request(&server, 2).await;

    // A page without `paging` does not decode.
    let server = TestServer::start(|_: &Recorded| {
        Reply::json(200, format!(r#"{{"items":[{}]}}"#, accepted_event()))
    })
    .await;
    let domain = client(&server, Duration::from_secs(10));
    let incomplete = search(&domain, &message_id, 2)
        .pages(cap(10))
        .unwrap()
        .items()
        .collect_complete()
        .await
        .expect_err("malformed page");
    let Some(Error::Concord(inner)) = incomplete.error.provider_error() else {
        panic!("expected Concord error, got {:?}", incomplete.error)
    };
    assert_eq!(inner.kind(), ErrorKind::Decode);
    assert!(incomplete.collected.is_empty());
    assert_no_second_request(&server, 1).await;

    // A sending-only key cannot read events.
    let server =
        TestServer::start(|_: &Recorded| Reply::text(401, "text/plain", "Forbidden")).await;
    let domain = client(&server, Duration::from_secs(10));
    let incomplete = search(&domain, &message_id, 2)
        .pages(cap(10))
        .unwrap()
        .items()
        .collect_complete()
        .await
        .expect_err("unauthorized");
    let Some(Error::Provider(provider)) = incomplete.error.provider_error() else {
        panic!("expected Provider, got {:?}", incomplete.error)
    };
    assert_eq!(provider.class(), HttpClass::Unauthorized);
    // EventCursor has no PartialEq, so positions are compared by shape.
    assert!(matches!(incomplete.error.resume(), Some(Resume::Start)));
    assert_no_second_request(&server, 1).await;
}

#[tokio::test]
async fn page_metadata_is_available_page_by_page() {
    let server = TestServer::start(|request: &Recorded| {
        let host = &request.host;
        if request.path().ends_with(CURSOR_2) {
            Reply::json(200, page(host, &[], &link(host, CURSOR_3)))
                .header("x-ratelimit-remaining", "41")
        } else {
            Reply::json(200, page(host, &[delivered_event()], &link(host, CURSOR_2)))
                .header("x-ratelimit-remaining", "42")
        }
    })
    .await;
    let domain = client(&server, Duration::from_secs(10));
    let message_id = MessageId::new(WIRE_ID).unwrap();
    let mut pages = search(&domain, &message_id, 2).pages(cap(10)).unwrap();
    let mut remaining = Vec::new();
    let mut events = Vec::new();
    while let Some(page) = pages.next_page().await.expect("page") {
        assert_eq!(page.meta.status(), 200);
        remaining.push(page.meta.rate_remaining());
        events.extend(page.items);
    }
    assert_eq!(
        remaining,
        [HeaderNumber::Value(42), HeaderNumber::Value(41)]
    );
    assert_eq!(
        delivery_status(&message_id, ALICE, &events),
        DeliveryStatus::Delivered
    );
    assert!(
        pages.next_page().await.expect("ended").is_none(),
        "end stays the end"
    );
    assert_no_second_request(&server, 2).await;
}

#[test]
fn delivery_interpretation_does_not_overclaim() {
    let decode = |items: &[String]| -> Vec<concord_sdk_mailgun::Event> {
        let page: concord_sdk_mailgun::EventPage =
            serde_json::from_str(&page("127.0.0.1:1", items, "http://127.0.0.1:1/x"))
                .expect("page");
        page.items
    };
    let id = MessageId::new(WIRE_ID).unwrap();
    let test_mode = event(
        "TmTmTmTmTmTmTmTmTmTmTm",
        "delivered",
        "1794042400.0",
        ALICE,
        "",
    )
    .replace(r#""is-test-mode":false"#, r#""is-test-mode":true"#);
    assert_eq!(
        delivery_status(&id, ALICE, &decode(&[accepted_event(), test_mode])),
        DeliveryStatus::TestMode
    );
    let permanent = event(
        "PpPpPpPpPpPpPpPpPpPpPp",
        "failed",
        "1794042400.0",
        ALICE,
        r#","severity":"permanent","reason":"bounce","delivery-status":{"code":550,"message":"5.1.1 mailbox unavailable"}"#,
    );
    assert_eq!(
        delivery_status(&id, ALICE, &decode(&[accepted_event(), permanent.clone()])),
        DeliveryStatus::PermanentFailure
    );
    let opened = event(
        "OoOoOoOoOoOoOoOoOoOoOo",
        "opened",
        "1794042500.0",
        ALICE,
        "",
    );
    assert_eq!(
        delivery_status(&id, ALICE, &decode(&[accepted_event(), opened])),
        DeliveryStatus::Accepted,
        "an engagement event is not delivery evidence"
    );
    let same_time_delivery = event(
        "SsSsSsSsSsSsSsSsSsSsSs",
        "delivered",
        "1794042400.0",
        ALICE,
        "",
    );
    assert_eq!(
        delivery_status(&id, ALICE, &decode(&[permanent, same_time_delivery])),
        DeliveryStatus::Conflicting
    );
    assert_eq!(delivery_status(&id, ALICE, &[]), DeliveryStatus::Unknown);

    // The API reference lists no top-level `recipient` item field (only envelope.targets).
    // Without it the SDK concludes nothing, rather than reading envelope.targets.
    let no_recipient = delivered_event().replace(r#""recipient":"alice@example.test","#, "");
    assert_eq!(
        delivery_status(&id, ALICE, &decode(&[no_recipient])),
        DeliveryStatus::Unknown
    );
    // The API reference describes `timestamp` in nanoseconds; an integer decodes and is
    // kept raw, with no unit conversion.
    let nanos = delivered_event().replace("1794042365.901344", "1794042365901344000");
    assert_eq!(decode(&[nanos])[0].timestamp, 1_794_042_365_901_344_000.0);
}
