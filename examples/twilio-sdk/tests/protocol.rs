//! Offline wire assertions and failure-shape checks; fixtures are not vendor conformance.

mod support;

use concord::ErrorKind;
use concord::bytes::Bytes;
use concord_example_testserver::{Body, Reply, TestServer};
use concord_sdk_twilio::{
    AuthError, BasicAuth, Error, ErrorDocumentProblem, MAX_ERROR_BODY, MessageStatus, Twilio,
    TwilioError,
};
use support::*;

#[tokio::test]
async fn create_sends_exact_form_bytes_and_configured_basic_auth() {
    let (server, script, client) =
        scripted([Reply::json(201, CREATED).header("twilio-request-id", "fixture-create-request")])
            .await;
    let response = client.create_message(message()).await.unwrap();
    let request = server.single();
    assert_eq!(request.method, "POST");
    assert_eq!(request.target, CREATE_PATH);
    assert_eq!(request.header_all("authorization"), [AUTHORIZATION]);
    assert_eq!(
        request.header_all("content-type"),
        ["application/x-www-form-urlencoded"]
    );
    assert_eq!(request.header_all("accept"), ["application/json"]);
    assert_eq!(
        request.header("content-length"),
        Some(FORM.len().to_string().as_str())
    );
    assert_eq!(request.body, FORM);
    assert_eq!(request.body_len, FORM.len() as u64);
    assert_eq!(response.value.sid, SID);
    assert_eq!(response.value.account_sid, ACCOUNT);
    assert_eq!(response.value.status, MessageStatus::Queued);
    assert_eq!(response.value.body.as_deref(), Some("Hello café & + 🌍"));
    assert_eq!(response.value.error_code, None);
    assert_eq!(response.meta.status.as_u16(), 201);
    assert_eq!(
        response.meta.headers()["twilio-request-id"],
        "fixture-create-request"
    );
    assert_eq!(
        response.meta.operation.unwrap().name(),
        "TwilioApi.create_message"
    );
    assert_eq!(script.remaining(), 0);
    server.shutdown().await;
}

#[tokio::test]
async fn fetch_has_no_form_body_and_exposes_message_processing_failure() {
    let (server, _, client) = scripted([Reply::json(200, UNDELIVERED)]).await;
    let response = client.get_message(SID).await.unwrap();
    let request = server.single();
    assert_eq!(request.method, "GET");
    assert_eq!(request.target, FETCH_PATH);
    assert_eq!(request.header_all("authorization"), [AUTHORIZATION]);
    assert_eq!(request.header_all("accept"), ["application/json"]);
    assert_eq!(request.header("content-type"), None);
    assert_eq!(request.body, b"");
    assert_eq!(response.value.status, MessageStatus::Undelivered);
    assert_eq!(response.value.error_code, Some(30007));
    assert_eq!(
        response.value.error_message.as_deref(),
        Some("Synthetic carrier filtering failure")
    );
    assert_eq!(response.meta.status.as_u16(), 200);
    assert_eq!(
        response.meta.operation.unwrap().name(),
        "TwilioApi.get_message"
    );
    server.shutdown().await;
}

#[tokio::test]
async fn optional_unicode_field_changes_only_its_own_form() {
    let (server, _, client) = scripted([
        Reply::json(201, CREATED),
        Reply::json(201, CREATED),
        Reply::json(200, DELIVERED),
    ])
    .await;
    client
        .create_message(message().with_message_intent("通知 & + café"))
        .await
        .unwrap();
    client.create_message(message()).await.unwrap();
    client.get_message(SID).await.unwrap();
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[0].body, INTENT_FORM);
    assert_eq!(requests[1].body, FORM);
    for request in &requests[..2] {
        assert_eq!(request.method, "POST");
        assert_eq!(request.target, CREATE_PATH);
        assert_eq!(
            request.header_all("content-type"),
            ["application/x-www-form-urlencoded"]
        );
    }
    for request in &requests {
        assert_eq!(request.header_all("authorization"), [AUTHORIZATION]);
        assert_eq!(request.header_all("accept"), ["application/json"]);
    }
    assert_eq!(requests[2].method, "GET");
    assert_eq!(requests[2].target, FETCH_PATH);
    assert!(requests[2].body.is_empty());
    assert_eq!(requests[2].header("content-type"), None);
    server.shutdown().await;
}

#[tokio::test]
async fn present_empty_intent_is_distinct_from_omission() {
    let (server, _, client) = scripted([Reply::json(201, CREATED)]).await;
    client
        .create_message(message().with_message_intent(""))
        .await
        .unwrap();
    assert_eq!(
        server.single().body,
        include_bytes!("../fixtures/create-empty-intent.form")
    );
    server.shutdown().await;
}

#[tokio::test]
async fn status_callback_preserves_unicode_reserved_values_and_unrelated_calls() {
    let callback = "https://callbacks.example.invalid/通知?label=café&tag=a+b";
    let cases: [(concord_sdk_twilio::CreateMessage, &[u8]); 5] = [
        (
            message().with_status_callback(callback),
            include_bytes!("../fixtures/create-callback.form"),
        ),
        (message(), FORM),
        (
            message()
                .with_message_intent("通知 & + café")
                .with_status_callback(callback),
            include_bytes!("../fixtures/create-intent-callback.form"),
        ),
        (message().with_message_intent("通知 & + café"), INTENT_FORM),
        (
            message().with_status_callback(""),
            include_bytes!("../fixtures/create-empty-callback.form"),
        ),
    ];
    let replies = (0..cases.len())
        .map(|_| Reply::json(201, CREATED))
        .chain([Reply::json(200, DELIVERED)]);
    let (server, script, client) = scripted(replies).await;
    for (input, _) in &cases {
        client.create_message(input.clone()).await.unwrap();
    }
    client.get_message(SID).await.unwrap();

    let requests = server.requests();
    assert_eq!(requests.len(), cases.len() + 1, "one request per call");
    for (request, (_, expected)) in requests.iter().zip(&cases) {
        assert_eq!(request.method, "POST");
        assert_eq!(request.target, CREATE_PATH);
        assert_eq!(request.body, *expected);
        assert_eq!(request.body_len, expected.len() as u64);
        assert_eq!(
            request.header("content-length"),
            Some(expected.len().to_string().as_str())
        );
        assert_eq!(
            request.header_all("content-type"),
            ["application/x-www-form-urlencoded"]
        );
    }
    assert_eq!(
        requests[0].form_pairs(),
        [
            ("To".to_owned(), "+15005550006".to_owned()),
            ("From".to_owned(), "+15005550001".to_owned()),
            ("Body".to_owned(), "Hello café & + 🌍".to_owned()),
            ("StatusCallback".to_owned(), callback.to_owned()),
        ]
    );
    for request in &requests {
        assert_eq!(request.header_all("authorization"), [AUTHORIZATION]);
        assert_eq!(request.header_all("accept"), ["application/json"]);
    }
    let fetched = &requests[cases.len()];
    assert_eq!(fetched.method, "GET");
    assert_eq!(fetched.target, FETCH_PATH);
    assert!(fetched.body.is_empty());
    assert_eq!(fetched.header("content-type"), None);
    assert_eq!(script.remaining(), 0);
    server.shutdown().await;
}

#[tokio::test]
async fn path_account_is_independent_of_auth_and_ids_are_single_segments() {
    let server = TestServer::start(|_| Reply::json(200, DELIVERED)).await;
    let client = Twilio::new(
        http(),
        "AC fixture/child?",
        BasicAuth::new("fixture-user", "fixture-password").unwrap(),
    )
    .with_base_url(server.url())
    .unwrap();
    client.get_message("SM fixture/next?#").await.unwrap();
    let request = server.single();
    assert_eq!(
        request.target,
        "/2010-04-01/Accounts/AC%20fixture%2Fchild%3F/Messages/SM%20fixture%2Fnext%3F%23.json"
    );
    assert_eq!(request.header("authorization"), Some(AUTHORIZATION));
    assert_eq!(request.query(), None);
    server.shutdown().await;
}

#[test]
fn credential_structure_and_diagnostics_are_safe() {
    for username in ["", "fixture:user", "fixture\r\nuser"] {
        assert_eq!(
            BasicAuth::new(username, "fixture-password").unwrap_err(),
            AuthError::Username
        );
    }
    for password in ["", "fixture\npassword", "fixture\0password"] {
        assert_eq!(
            BasicAuth::new("fixture-user", password).unwrap_err(),
            AuthError::Password
        );
    }
    assert!(BasicAuth::new("fixture-user", "fixture:password").is_ok());
    let auth = BasicAuth::new("fixture-user", "fixture-password").unwrap();
    let auth_debug = format!("{auth:?}");
    let client = client("http://127.0.0.1:1");
    let input =
        message().with_status_callback("https://callbacks.example.invalid/通知?label=café&tag=a+b");
    let prepared = client.prepare_create_message(input.clone()).unwrap();
    assert!(prepared.request().headers()["authorization"].is_sensitive());
    for debug in [
        auth_debug,
        format!("{client:?}"),
        format!("{prepared:?}"),
        format!("{input:?}"),
    ] {
        for private in [
            "fixture-user",
            "fixture-password",
            AUTHORIZATION,
            ACCOUNT,
            "Hello café",
            "+15005550006",
            "callbacks.example.invalid",
        ] {
            assert!(!debug.contains(private), "diagnostic leaked {private}");
        }
    }
}

#[tokio::test]
async fn bad_request_is_typed_and_preserves_metadata_without_echoing_input() {
    let (server, _, client) = scripted([Reply::json(400, BAD_REQUEST)
        .header("twilio-request-id", "fixture-private-header")
        .header("x-fixture", "one")
        .header("x-fixture", "two")])
    .await;
    let error = client.create_message(message()).await.unwrap_err();
    let document = error.provider_error().unwrap();
    assert_eq!(document.code, Some(21201));
    assert_eq!(document.status, 400);
    assert!(document.message.contains("fixture-private-text"));
    assert_eq!(
        document.more_info.as_deref(),
        Some("https://www.twilio.com/docs/errors/21201")
    );
    assert_eq!(error.meta().unwrap().status.as_u16(), 400);
    assert_eq!(
        error
            .meta()
            .unwrap()
            .headers()
            .get_all("x-fixture")
            .iter()
            .count(),
        2
    );
    assert_eq!(error.operation().unwrap().method(), Some("POST"));
    assert_eq!(server.requests().len(), 1);
    for diagnostic in [format!("{error}"), format!("{error:?}")] {
        assert!(!diagnostic.contains("fixture-private-text"));
        assert!(!diagnostic.contains("fixture-private-header"));
        assert!(!diagnostic.contains("fixture-password"));
        assert!(diagnostic.contains("21201"));
    }
    server.shutdown().await;
}

#[tokio::test]
async fn an_empty_sid_is_refused_before_sending() {
    let (server, script, client) = scripted([Reply::json(200, DELIVERED)]).await;
    for error in [
        client.get_message("").await.unwrap_err(),
        client.prepare_get_message(String::new()).unwrap_err(),
    ] {
        assert!(matches!(error, TwilioError::EmptySid { .. }), "{error:?}");
        assert_eq!(error.operation().unwrap().name(), "TwilioApi.get_message");
        assert!(error.meta().is_none());
        assert_eq!(
            error.to_string(),
            "GET TwilioApi.get_message: empty message SID (nothing sent)"
        );
    }
    assert!(server.requests().is_empty());
    // A one-character SID is still sent: only emptiness is checked locally.
    client.get_message("x").await.unwrap();
    assert_eq!(
        server.single().target,
        "/2010-04-01/Accounts/AC_fixture_account/Messages/x.json"
    );
    assert_eq!(script.remaining(), 0);
    server.shutdown().await;
}

#[tokio::test]
async fn message_debug_shows_identity_and_outcome_but_not_contents() {
    let (server, _, client) = scripted([Reply::json(200, UNDELIVERED)]).await;
    let fetched = client.get_message(SID).await.unwrap();
    let message = format!("{:?}", fetched.value);
    assert_eq!(
        message,
        "Message { sid: \"SM_fixture_message\", status: Undelivered, error_code: Some(30007), .. }"
    );
    let response = format!("{fetched:?}");
    assert!(response.starts_with(&format!("Response {{ value: {message}, meta: ")));
    let prepared = format!("{:?}", client.prepare_get_message(SID).unwrap());
    assert!(prepared.contains("SM_fixture_message"), "{prepared}");
    for shown in [message, response, prepared] {
        for private in [
            ACCOUNT,
            "+15005550001",
            "+15005550006",
            "Hello café",
            "Synthetic carrier filtering failure",
            "fixture-password",
        ] {
            assert!(!shown.contains(private), "{private} in {shown}");
        }
    }
    server.shutdown().await;
}

#[tokio::test]
async fn unauthorized_get_keeps_http_status_and_provider_code() {
    let (server, _, client) = scripted([Reply::json(401, UNAUTHORIZED)]).await;
    let error = client.get_message(SID).await.unwrap_err();
    assert_eq!(error.meta().unwrap().status.as_u16(), 401);
    assert_eq!(error.provider_error().unwrap().code, Some(20003));
    assert_eq!(error.operation().unwrap().name(), "TwilioApi.get_message");
    assert_eq!(server.single().target, FETCH_PATH);
    server.shutdown().await;
}

#[tokio::test]
async fn not_found_can_omit_optional_exception_fields() {
    let (server, _, client) = scripted([Reply::json(404, NOT_FOUND)]).await;
    let error = client.get_message(SID).await.unwrap_err();
    let document = error.provider_error().unwrap();
    assert_eq!(document.code, None);
    assert_eq!(document.more_info, None);
    assert_eq!(error.meta().unwrap().status.as_u16(), 404);
    server.shutdown().await;
}

#[tokio::test]
async fn rate_limit_keeps_retry_advice_and_does_not_repeat_post() {
    let (server, _, client) =
        scripted([Reply::json(429, RATE_LIMITED).header("retry-after", "7")]).await;
    let error = client.create_message(message()).await.unwrap_err();
    assert_eq!(error.provider_error().unwrap().code, Some(20429));
    assert_eq!(error.meta().unwrap().status.as_u16(), 429);
    assert_eq!(error.meta().unwrap().headers()["retry-after"], "7");
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
}

#[tokio::test]
async fn actual_status_is_not_replaced_by_status_in_error_body() {
    let (server, _, client) = scripted([Reply::json(503, BAD_REQUEST)]).await;
    let error = client.get_message(SID).await.unwrap_err();
    assert_eq!(error.meta().unwrap().status.as_u16(), 503);
    assert_eq!(error.provider_error().unwrap().status, 400);
    server.shutdown().await;
}

#[tokio::test]
async fn malformed_or_wrong_shape_success_keeps_head_and_decode_kind() {
    let (server, _, client) = scripted(
        [MALFORMED, MISSING_STATUS, WRONG_ERROR_CODE]
            .map(|body| Reply::json(200, body).header("twilio-request-id", "fixture-decode")),
    )
    .await;
    for _ in 0..3 {
        let error = client.get_message(SID).await.unwrap_err();
        assert_eq!(error.concord().unwrap().kind(), ErrorKind::Decode);
        assert_eq!(error.meta().unwrap().status.as_u16(), 200);
        assert_eq!(
            error.meta().unwrap().headers()["twilio-request-id"],
            "fixture-decode"
        );
        assert_eq!(error.operation().unwrap().name(), "TwilioApi.get_message");
    }
    assert_eq!(server.requests().len(), 3);
    server.shutdown().await;
}

#[tokio::test]
async fn success_requires_one_json_content_type() {
    let (server, _, client) = scripted([
        Reply::text(200, "text/html", CREATED),
        Reply::new(200).body(CREATED),
        Reply::json(200, CREATED).header("content-type", "application/json"),
    ])
    .await;
    for _ in 0..3 {
        let error = client.get_message(SID).await.unwrap_err();
        assert_eq!(error.concord().unwrap().kind(), ErrorKind::Metadata);
        assert!(error.concord().unwrap().response().is_some());
        assert_eq!(error.meta().unwrap().status.as_u16(), 200);
    }
    server.shutdown().await;
}

#[tokio::test]
async fn malformed_error_documents_remain_http_failures() {
    let (server, _, client) = scripted([
        Reply::json(400, MALFORMED),
        Reply::json(400, NOT_AN_ERROR),
        Reply::text(502, "text/html", HTML),
        Reply::new(500).body(BAD_REQUEST),
        Reply::json(400, BAD_REQUEST).header("content-type", "application/json"),
    ])
    .await;
    for (status, expected) in [
        (400, ErrorDocumentProblem::Decode),
        (400, ErrorDocumentProblem::Decode),
        (502, ErrorDocumentProblem::ContentType),
        (500, ErrorDocumentProblem::ContentType),
        (400, ErrorDocumentProblem::ContentType),
    ] {
        let error = client.get_message(SID).await.unwrap_err();
        assert_eq!(error.meta().unwrap().status.as_u16(), status);
        assert!(
            matches!(error, Error::Provider { document: Err(problem), .. } if problem == expected)
        );
    }
    server.shutdown().await;
}

#[tokio::test]
async fn error_body_limit_accepts_exact_boundary_and_rejects_one_more_byte() {
    let mut exact = BAD_REQUEST.to_vec();
    exact.resize(MAX_ERROR_BODY, b' '); // Valid trailing JSON whitespace.
    let mut over = exact.clone();
    over.push(b' ');
    let (server, _, client) = scripted([
        Reply::json(400, exact),
        Reply::new(400)
            .header("content-type", "application/json")
            .with_body(Body::Chunks(vec![
                (
                    std::time::Duration::ZERO,
                    Bytes::copy_from_slice(&over[..MAX_ERROR_BODY]),
                ),
                (
                    std::time::Duration::ZERO,
                    Bytes::copy_from_slice(&over[MAX_ERROR_BODY..]),
                ),
            ])),
    ])
    .await;
    assert!(
        client
            .get_message(SID)
            .await
            .unwrap_err()
            .provider_error()
            .is_some()
    );
    let error = client.get_message(SID).await.unwrap_err();
    assert_eq!(error.meta().unwrap().status.as_u16(), 400);
    assert!(matches!(
        error,
        Error::Provider {
            document: Err(ErrorDocumentProblem::TooLarge),
            ..
        }
    ));
    server.shutdown().await;
}

#[tokio::test]
async fn complete_json_prefix_is_not_success_without_transport_eof() {
    let (server, _, client) = scripted([
        Reply::new(201)
            .header("content-type", "application/json")
            .with_body(Body::FailAfter(vec![Bytes::from_static(CREATED)])),
        Reply::new(400)
            .header("content-type", "application/json")
            .with_body(Body::FailAfter(vec![Bytes::from_static(BAD_REQUEST)])),
    ])
    .await;
    let error = client.create_message(message()).await.unwrap_err();
    assert_eq!(error.concord().unwrap().kind(), ErrorKind::Transport);
    assert_eq!(error.meta().unwrap().status.as_u16(), 201);
    let error = client.create_message(message()).await.unwrap_err();
    assert_eq!(error.meta().unwrap().status.as_u16(), 400);
    assert!(matches!(
        error,
        Error::Provider {
            document: Err(ErrorDocumentProblem::Read),
            ..
        }
    ));
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}

#[tokio::test]
async fn lost_post_response_has_no_fabricated_status_and_is_not_retried() {
    let (server, _, client) = scripted([Reply::hang_up()]).await;
    let error = client.create_message(message()).await.unwrap_err();
    assert_eq!(error.concord().unwrap().kind(), ErrorKind::Transport);
    assert!(error.meta().is_none());
    assert!(error.concord().unwrap().status().is_none());
    assert_eq!(
        error.operation().unwrap().name(),
        "TwilioApi.create_message"
    );
    assert_eq!(server.single().body, FORM);
    server.shutdown().await;
}

#[tokio::test]
async fn unknown_status_and_missing_nullable_fields_survive() {
    let (server, _, client) = scripted([
        Reply::json(200, UNKNOWN_STATUS),
        Reply::json(200, DELIVERED),
    ])
    .await;
    let future = client.get_message(SID).await.unwrap().value;
    assert_eq!(
        future.status,
        MessageStatus::Unknown("future_fixture_status".into())
    );
    assert_eq!(future.body, None);
    assert_eq!(future.error_code, None);
    assert_eq!(future.error_message, None);
    let delivered = client.get_message(SID).await.unwrap().value;
    assert_eq!(delivered.status, MessageStatus::Delivered);
    assert_eq!(delivered.body, None);
    server.shutdown().await;
}
