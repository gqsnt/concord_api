//! Protocol expectations are authored from the cited Mailgun contracts; fixture mutations are synthetic.
mod support;
use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use concord_sdk_mailgun::{
    concord::{Encode, ErrorKind, bytes::Bytes},
    pagination::PagesError,
    *,
};
use std::{
    error::Error as _,
    time::{Duration, SystemTime},
};
use support::*;

#[tokio::test]
async fn buffered_send_uses_basic_multipart_and_preserves_acknowledgement() {
    let server = TestServer::start(|_: &Recorded| Reply::json(200, ACCEPTED)).await;
    let api = client(&server);
    let operation = domain(&api).send_message(email());
    let response = operation.send().await.unwrap();
    assert_eq!(response.value.id.as_str(), "<fixture-27@mg.example.test>");
    assert_eq!(response.value.message, "Queued. Thank you.");
    assert_eq!(response.meta.status().as_u16(), 200);
    assert_eq!(
        response.meta.0.operation().unwrap().name(),
        "Mailgun.domain.send_message"
    );
    let wire = server.single();
    assert_email_wire(&wire);
    assert!(!wire.chunked);
    assert_eq!(
        wire.header("content-length")
            .unwrap()
            .parse::<usize>()
            .unwrap(),
        wire.body.len()
    );
    server.shutdown().await;
}

#[tokio::test]
async fn event_filters_and_regional_paths_are_declarative() {
    let server = TestServer::start(|request: &Recorded| page(EMPTY, request)).await;
    let api = client(&server);
    let id = MessageId::new("<m+7@mg.example.test>").unwrap();
    domain(&api)
        .events(range())
        .set_limit(PageSize::new(300).unwrap())
        .set_message_id(id)
        .set_recipient("a+b@example.test")
        .set_event("delivered OR failed")
        .send()
        .await
        .unwrap();
    let request = server.single();
    assert_eq!(request.method, "GET");
    assert_eq!(request.path(), "/v3/mg.example.test/events");
    assert_eq!(request.query_all("begin"), ["1700000000"]);
    assert_eq!(request.query_all("end"), ["1700000100"]);
    assert_eq!(request.query_all("ascending"), ["yes"]);
    assert_eq!(request.query_all("limit"), ["300"]);
    assert_eq!(request.query_all("message-id"), ["<m+7@mg.example.test>"]);
    assert_eq!(request.query_all("recipient"), ["a+b@example.test"]);
    assert_eq!(request.query_all("event"), ["delivered OR failed"]);
    assert_eq!(request.header("authorization"), Some(BASIC));
    assert!(request.body.is_empty());
    assert!(request.header("content-type").is_none());
    for (region, origin) in [
        (Region::Us, "https://api.mailgun.net"),
        (Region::Eu, "https://api.eu.mailgun.net"),
    ] {
        let api = Mailgun::connect(region, KEY, Duration::from_secs(5)).unwrap();
        let prepared = domain(&api).events(range()).prepare().unwrap();
        assert!(
            prepared
                .request()
                .url()
                .as_str()
                .starts_with(&format!("{origin}/v3/mg.example.test/events?"))
        );
    }
    Mailgun::validate_header_names().unwrap();
    server.shutdown().await;
}

#[tokio::test]
async fn every_documented_http_error_class_survives_with_bounded_document() {
    for (status, expected) in [
        (400, HttpClass::BadRequest),
        (401, HttpClass::Unauthorized),
        (403, HttpClass::Forbidden),
        (404, HttpClass::NotFound),
        (429, HttpClass::RateLimited),
        (500, HttpClass::Server),
        (503, HttpClass::Server),
        (418, HttpClass::Unexpected),
    ] {
        let server = TestServer::start(move |_: &Recorded| Reply::json(status, PROBLEM)).await;
        let api = client(&server);
        let outcomes = [
            domain(&api).send_message(email()).send().await.unwrap_err(),
            domain(&api).events(range()).send().await.unwrap_err(),
        ];
        for error in outcomes {
            assert!(!format!("{error:?} {error}").contains("synthetic-secret"));
            assert!(error.source().is_none());
            let Error::Provider(provider) = error else {
                panic!("HTTP status lost")
            };
            assert_eq!(provider.class(), expected);
            assert_eq!(provider.meta.status().as_u16(), status);
            assert!(
                provider
                    .document
                    .unwrap()
                    .message
                    .contains("synthetic-secret")
            );
        }
        assert_eq!(server.requests().len(), 2); // one execution per explicit call, no retry
        server.shutdown().await;
    }
}

// Each body variant is exercised on both a success and an HTTP failure, for both endpoints.
#[tokio::test]
async fn malformed_truncated_oversized_wrong_media_and_mid_body_failures() {
    for write in [false, true] {
        for status in [200, 400] {
            for (case, expected) in [
                (0, ErrorKind::Decode),
                (1, ErrorKind::Decode),
                (2, ErrorKind::Limit),
                (3, ErrorKind::Metadata),
                (4, ErrorKind::Transport),
                (5, ErrorKind::Metadata),
                (6, ErrorKind::Metadata),
                (7, ErrorKind::Decode),
                (8, ErrorKind::Decode),
                (9, ErrorKind::Metadata),
            ] {
                let server = TestServer::start(move |_: &Recorded| match case {
                    0 => Reply::json(status, MALFORMED),
                    1 => Reply::json(status, TRUNCATED),
                    2 => Reply::json(status, "x".repeat(EVENT_BODY_LIMIT + 1)),
                    3 => Reply::text(status, "text/html", "synthetic-secret"),
                    4 => Reply::json(status, "")
                        .with_body(Body::FailAfter(vec![Bytes::from_static(b"{\"secret\":")])),
                    5 => Reply::new(status).body("synthetic-secret"),
                    6 => Reply::json(status, ACCEPTED).header("content-type", "application/json"),
                    7 => Reply::json(status, "{}"),
                    8 => Reply::json(status, ""),
                    _ => Reply::text(status, "application/json; charset=iso-8859-1", ACCEPTED),
                })
                .await;
                let api = client(&server);
                let error = if write {
                    domain(&api).send_message(email()).send().await.unwrap_err()
                } else {
                    domain(&api).events(range()).send().await.unwrap_err()
                };
                assert!(!format!("{error:?} {error}").contains("synthetic-secret"));
                let underlying = match &error {
                    Error::Provider(provider) if status == 400 => {
                        assert_eq!(provider.class(), HttpClass::BadRequest);
                        provider.document.as_ref().unwrap_err().as_ref()
                    }
                    Error::AmbiguousSend(error) if write && status == 200 => error,
                    Error::Concord(error) if !write && status == 200 => error,
                    _ => panic!("wrong failure: {error:?}"),
                };
                assert_eq!(
                    underlying.kind(),
                    expected,
                    "case {case}, status {status}, write {write}"
                );
                assert_eq!(underlying.head().unwrap().status().as_u16(), status);
                assert_eq!(server.requests().len(), 1);
                server.shutdown().await;
            }
        }
    }
}

#[tokio::test]
async fn rate_hints_are_exposed_but_never_replay_a_send() {
    let server = TestServer::start(|_: &Recorded| {
        Reply::json(429, PROBLEM)
            .header("x-ratelimit-limit", "100")
            .header("x-ratelimit-remaining", "0")
            .header("x-ratelimit-reset", "1700000123456")
            .header("retry-after", "2")
            .header("set-cookie", "synthetic-secret")
    })
    .await;
    let error = domain(&client(&server))
        .send_message(email())
        .send()
        .await
        .unwrap_err();
    assert!(!format!("{error:?} {error}").contains("synthetic-secret"));
    let Error::Provider(provider) = error else {
        panic!("expected provider error")
    };
    assert_eq!(provider.meta.rate_limit(), HeaderNumber::Value(100));
    assert_eq!(provider.meta.rate_remaining(), HeaderNumber::Value(0));
    assert_eq!(
        provider.meta.rate_reset_millis(),
        HeaderNumber::Value(1700000123456)
    );
    assert_eq!(
        provider.meta.retry_after(SystemTime::UNIX_EPOCH).unwrap(),
        Some(Duration::from_secs(2))
    );
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;

    let server = TestServer::start(|request: &Recorded| {
        page(EMPTY, request)
            .header("x-ratelimit-limit", "1")
            .header("x-ratelimit-limit", "2")
            .header("x-ratelimit-remaining", "18446744073709551616")
            .header("x-ratelimit-reset", "-1")
            .header("retry-after", "1")
            .header("retry-after", "2")
    })
    .await;
    let meta = domain(&client(&server))
        .events(range())
        .send()
        .await
        .unwrap()
        .meta;
    assert_eq!(meta.rate_limit(), HeaderNumber::Invalid);
    assert_eq!(meta.rate_remaining(), HeaderNumber::Invalid);
    assert_eq!(meta.rate_reset_millis(), HeaderNumber::Invalid);
    assert_eq!(
        meta.retry_after(SystemTime::UNIX_EPOCH),
        Err(RetryAfterError::Repeated)
    );
    server.shutdown().await;
}

#[tokio::test]
async fn redirects_are_not_followed_for_either_endpoint() {
    let sink = TestServer::start(|_: &Recorded| Reply::json(200, ACCEPTED)).await;
    for status in [302, 307, 308] {
        let location = format!("{}/stolen", sink.url());
        let server = TestServer::start(move |_: &Recorded| {
            Reply::json(status, PROBLEM).header("location", &location)
        })
        .await;
        let api = client(&server);
        for error in [
            domain(&api).send_message(email()).send().await.unwrap_err(),
            domain(&api).events(range()).send().await.unwrap_err(),
        ] {
            let Error::Provider(provider) = error else {
                panic!("redirect was not retained")
            };
            assert_eq!(provider.class(), HttpClass::Unexpected);
            assert_eq!(provider.meta.status().as_u16(), status);
        }
        assert_eq!(server.requests().len(), 2);
        assert_eq!(sink.connections(), 0);
        server.shutdown().await;
    }
    sink.shutdown().await;
}

#[tokio::test]
async fn hostile_provider_links_never_receive_credentials() {
    for bad in [
        "http://example.invalid/v3/mg.example.test/events/x",
        "__ORIGIN__/v3/other.test/events/x",
        "__ORIGIN__/v3/mg.example.test/messages",
        "__ORIGIN__/v3/mg.example.test/events-evil/x",
        "__ORIGIN__/v3/mg.example.test/events/x#fragment",
        "__USERINFO__/v3/mg.example.test/events/x",
        "__ORIGIN__/v3/mg.example.test/events/%2fsecret",
        "__ORIGIN__/v3/mg.example.test/events/a/b",
        "__ORIGIN__/v3/mg.example.test/events/..%2fmessages",
        "//evil.invalid/x",
        "not a URL",
    ] {
        let server = TestServer::start(move |request: &Recorded| {
            let mut value: serde_json::Value = serde_json::from_str(FIRST).unwrap();
            value["paging"]["next"] = bad
                .replace("__ORIGIN__", &format!("http://{}", request.host))
                .replace(
                    "__USERINFO__",
                    &format!("http://api:synthetic-secret@{}", request.host),
                )
                .into();
            Reply::json(200, value.to_string())
        })
        .await;
        let error = domain(&client(&server))
            .events(range())
            .pages(cap(4))
            .unwrap()
            .items()
            .collect_complete()
            .await
            .unwrap_err();
        assert!(matches!(
            error.error.provider_error(),
            Some(Error::Link(LinkError::Untrusted))
        ));
        assert!(!format!("{:?}", error.error).contains("synthetic-secret"));
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn redaction_covers_clients_operations_payloads_responses_and_errors() {
    let server = TestServer::start(|_: &Recorded| {
        Reply::json(
            200,
            r#"{"id":"<synthetic-secret>","message":"synthetic-secret"}"#,
        )
        .header("x-private", "synthetic-secret")
    })
    .await;
    let api = Mailgun::local(&server.url(), "synthetic-secret", Duration::from_secs(1)).unwrap();
    let scope = domain(&api);
    let op = scope
        .events(range())
        .set_message_id(MessageId::new("synthetic-secret").unwrap())
        .set_recipient("synthetic-secret");
    let shown = format!(
        "{api:?} {scope:?} {op:?} {:?}",
        op.clone().prepare().unwrap()
    );
    assert!(!shown.contains("synthetic-secret"));
    assert!(!shown.contains("YXBpOnN5bnRoZXRpYy1zZWNyZXQ="));
    let payload = TextEmail::new(
        "synthetic-secret",
        vec!["synthetic-secret".into()],
        "synthetic-secret",
        "synthetic-secret",
        Attachment::bytes("synthetic-secret", "text/plain", "synthetic-secret").unwrap(),
    )
    .unwrap();
    assert!(!format!("{payload:?}").contains("synthetic-secret"));
    let request = scope.send_message(payload).prepare().unwrap();
    assert!(request.request().headers()["authorization"].is_sensitive());
    let config = MailgunConfig {
        authorization: request.request().headers()["authorization"].clone(),
    };
    assert!(!format!("{config:?}").contains("YXBpOnN5bnRoZXRpYy1zZWNyZXQ="));
    assert!(!format!("{request:?}").contains("synthetic-secret"));
    let response = scope.send_message(email()).send().await.unwrap();
    assert!(
        !format!("{response:?} {:?} {:?}", response.value, response.value.id)
            .contains("synthetic-secret")
    );
    assert_eq!(response.value.id.as_str(), "<synthetic-secret>");
    // A Build error occurs before any wire activity.
    let build = Mailgun::local(
        "http://127.0.0.1:0",
        "synthetic-secret",
        Duration::from_secs(1),
    )
    .unwrap_err();
    assert_eq!(build.concord_error().unwrap().kind(), ErrorKind::Build);
    assert!(!format!("{build:?} {build}").contains("synthetic-secret"));
    server.shutdown().await;
}

#[test]
fn input_validation_and_encode_boundary_errors_are_value_free() {
    for name in [
        "",
        ".",
        "..",
        "a/b",
        "a?b",
        "-bad.test",
        "bad-.test",
        "a..test",
    ] {
        assert!(matches!(
            Domain::new(name),
            Err(Error::Input(InputError::Domain))
        ));
    }
    assert!(PageSize::new(0).is_err());
    assert!(PageSize::new(301).is_err());
    assert!(EventRange::new(2, 1).is_err());
    for id in ["", "<>", "a\nb", "<unclosed", "a b"] {
        assert!(MessageId::new(id).is_err());
    }
    for name in ["", "a\"b", "a\\b", "a\nb", "a/b"] {
        assert!(Attachment::bytes(name, "text/plain", FILE).is_err());
    }
    for media in ["no media", "text/*", "text/plain\r\nsecret"] {
        assert!(Attachment::bytes("x", media, FILE).is_err());
    }
    assert!(matches!(
        Attachment::from_file("/nonexistent/sdk27-secret", "x", "text/plain"),
        Err(Error::Input(InputError::File))
    ));
    assert!(matches!(
        Attachment::bytes("x", "text/plain", vec![0; MAX_ATTACHMENT_BYTES + 1]),
        Err(Error::Input(InputError::Size))
    ));
    let attachment = || Attachment::bytes("x", "text/plain", FILE).unwrap();
    assert!(TextEmail::new("from", vec![], "s", "t", attachment()).is_err());
    assert!(TextEmail::new("from\r\nsecret", vec!["to".into()], "s", "t", attachment()).is_err());
    assert!(
        TextEmail::new(
            "from",
            vec!["to".into()],
            "s",
            "x".repeat(MAX_TEXT_BYTES + 1),
            attachment()
        )
        .is_err()
    );
    let error = MailgunMultipart::encode(email()).unwrap_err();
    assert!(!format!("{error:?} {error}").contains("report"));
    for key in ["", "secret\n", " secret"] {
        let error = Mailgun::connect(Region::Us, key, Duration::from_secs(1)).unwrap_err();
        assert!(!format!("{error:?} {error}").contains("secret"));
    }
    assert!(Mailgun::connect(Region::Us, KEY, Duration::ZERO).is_err());
    assert!(Mailgun::local("https://api.mailgun.net", KEY, Duration::from_secs(1)).is_err());
}

#[tokio::test]
async fn schema_failure_is_not_an_empty_page_or_a_valid_send() {
    for body in [
        r#"{"items":[],"paging":{}}"#,
        r#"{"id":"","message":"ok"}"#,
        r#"{"items":[{"event":"delivered"}],"paging":{"next":"x","previous":"y"}}"#,
    ] {
        let server = TestServer::start(move |_: &Recorded| Reply::json(200, body)).await;
        let api = client(&server);
        let mut pages = domain(&api).events(range()).pages(cap(1)).unwrap();
        let error = pages.next_page().await.unwrap_err();
        assert_eq!(
            error
                .provider_error()
                .unwrap()
                .concord_error()
                .unwrap()
                .kind(),
            ErrorKind::Decode
        );
        assert!(matches!(
            pages.next_page().await,
            Err(PagesError::Ended { .. })
        ));
        let error = domain(&api).send_message(email()).send().await.unwrap_err();
        assert!(matches!(error, Error::AmbiguousSend(_)));
        server.shutdown().await;
    }
}

#[tokio::test]
async fn a_refused_connection_on_a_send_stays_ambiguous() {
    let reserved = tokio::net::TcpSocket::new_v4().unwrap();
    reserved.bind(([127, 0, 0, 1], 0).into()).unwrap();
    let origin = format!("http://{}", reserved.local_addr().unwrap());
    let api = Mailgun::local(&origin, KEY, Duration::from_secs(2)).unwrap();
    let error = domain(&api).send_message(email()).send().await.unwrap_err();
    let Error::AmbiguousSend(error) = error else {
        panic!("{error:?}")
    };
    // Concord cannot prove a refused connection sent nothing (a redirect may follow an accepted send).
    assert_eq!(error.delivery(), concord::Delivery::Unknown);
    assert!(error.transport().is_some_and(|e| e.is_connect()));
    drop(reserved);
}
