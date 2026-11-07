//! Token-counting regressions. All counts, keys and request IDs are synthetic loopback data.

mod support;

use std::time::{Duration, SystemTime};

use concord::{Delivery, ErrorKind};
use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_anthropic_messages::{API_VERSION, ApiErrorType, Error, MAX_MESSAGE_BYTES};
use serde_json::{Value, json};
use support::{KEY, api, assert_no_second_request, error_document, hello};

#[tokio::test]
async fn counting_omits_generation_settings_and_accepts_zero_tokens() {
    let server = TestServer::start(|_: &Recorded| -> Reply {
        Reply::json(200, r#"{"input_tokens":0,"future_field":true}"#)
            .header("request-id", "req_count_zero")
    })
    .await;
    let request = hello();
    let response = api(&server)
        .messages()
        .count_tokens(request.count_tokens())
        .send()
        .await
        .unwrap();
    assert_eq!(response.value.input_tokens, 0);
    assert_eq!(response.request_id(), Some("req_count_zero"));
    assert_eq!(response.meta.status(), 200);
    assert!(response.meta.operation().is_some());
    let recorded = server.single();
    assert_eq!(recorded.method, "POST");
    assert_eq!(recorded.target, "/v1/messages/count_tokens");
    assert_eq!(recorded.header_all("x-api-key"), [KEY]);
    assert_eq!(recorded.header_all("anthropic-version"), [API_VERSION]);
    assert_eq!(recorded.header("content-type"), Some("application/json"));
    assert_eq!(recorded.header("accept"), Some("application/json"));
    assert_eq!(recorded.header("authorization"), None);
    assert_eq!(
        serde_json::from_slice::<Value>(&recorded.body).unwrap(),
        json!({
            "model":"claude-opus-5-5",
            "messages":[{"role":"user","content":"Hello"}]
        })
    );
    assert_eq!(request.max_tokens(), 256); // counting did not consume or modify the request
    assert_no_second_request(&server).await;
    server.shutdown().await;
}

#[test]
fn counting_keeps_a_snapshot_of_system_and_tools_without_generation_fields() {
    // Both documented system shapes; explicit empty tools differs from an omitted tools field.
    for system in [
        json!("Be concise."),
        json!([{"type":"text","text":"Be concise."}]),
    ] {
        let request = hello().with_system(system.clone()).with_tools(vec![]);
        let snapshot = request.count_tokens();
        let changed = request.with_system("A different instruction.");
        assert_eq!(
            serde_json::to_value(snapshot).unwrap(),
            json!({
                "model":"claude-opus-5-5",
                "messages":[{"role":"user","content":"Hello"}],
                "system":system,
                "tools":[]
            })
        );
        assert_eq!(changed.system(), Some(&json!("A different instruction.")));
        assert_eq!(changed.tools(), Some([].as_slice()));
    }
}

#[tokio::test]
async fn count_errors_use_the_existing_documents_and_ids_without_falling_back() {
    for (status, kind, expected) in [
        (400, "invalid_request_error", ApiErrorType::InvalidRequest),
        (401, "authentication_error", ApiErrorType::Authentication),
        (404, "not_found_error", ApiErrorType::NotFound),
        (429, "rate_limit_error", ApiErrorType::RateLimit),
        (529, "overloaded_error", ApiErrorType::Overloaded),
    ] {
        // Simulates unsupported counting, rejected tools, and transient errors. A successful
        // generation response on another path would expose an unsafe fallback.
        let document = error_document(kind, &format!("synthetic refusal: {KEY}"), "req_count_body");
        let server = TestServer::start(move |request: &Recorded| -> Reply {
            if request.path() != "/v1/messages/count_tokens" {
                return Reply::json(200, support::fixture("message.json"));
            }
            let reply = Reply::json(status, &document).header("retry-after", "2");
            if status == 404 {
                reply // exercise body request_id fallback
            } else {
                reply.header("request-id", "req_count_head")
            }
        })
        .await;
        let error = api(&server)
            .messages()
            .count_tokens(
                hello()
                    .with_tools(vec![
                        json!({"type":"web_search_20250305","name":"web_search"}),
                    ])
                    .count_tokens(),
            )
            .send()
            .await
            .unwrap_err();
        assert_eq!(error.status().unwrap(), status);
        assert_eq!(
            error.request_id(),
            Some(if status == 404 {
                "req_count_body"
            } else {
                "req_count_head"
            })
        );
        assert!(!format!("{error:?} {error}").contains(KEY));
        let Error::Api(error) = error else {
            panic!("expected API error")
        };
        assert_eq!(error.error_type(), Some(&expected));
        assert_eq!(
            error.message(),
            Some(format!("synthetic refusal: {KEY}").as_str())
        );
        assert!(error.document_error().is_none());
        assert_eq!(
            error.retry_after(SystemTime::now()).unwrap(),
            Some(Duration::from_secs(2))
        );
        assert_eq!(server.single().target, "/v1/messages/count_tokens");
        assert_no_second_request(&server).await;
        server.shutdown().await;
    }
}

#[tokio::test]
async fn unreadable_count_errors_keep_status_and_request_id() {
    let server = TestServer::start(|_: &Recorded| -> Reply {
        Reply::text(500, "text/html", "<h1>Proxy failure</h1>")
            .header("request-id", "req_count_proxy")
    })
    .await;
    let error = api(&server)
        .messages()
        .count_tokens(hello().count_tokens())
        .send()
        .await
        .unwrap_err();
    assert_eq!(error.request_id(), Some("req_count_proxy"));
    assert_eq!(error.status().unwrap(), 500);
    let Error::Api(error) = error else {
        panic!("expected API error")
    };
    assert_eq!(error.error_type(), None);
    assert_eq!(error.document_error().unwrap().kind(), ErrorKind::Metadata);
    assert_no_second_request(&server).await;
    server.shutdown().await;
}

#[tokio::test]
async fn invalid_count_successes_are_errors_and_never_become_generations() {
    for (media, body, expected) in [
        ("application/json", "{}".to_owned(), ErrorKind::Decode),
        (
            "application/json",
            support::fixture("message.json"),
            ErrorKind::Decode,
        ),
        (
            "application/json",
            r#"{"input_tokens":-1}"#.to_owned(),
            ErrorKind::Decode,
        ),
        (
            "application/json",
            r#"{"input_tokens":1.5}"#.to_owned(),
            ErrorKind::Decode,
        ),
        (
            "application/json",
            r#"{"input_tokens":"12"}"#.to_owned(),
            ErrorKind::Decode,
        ),
        ("application/json", "{oops".to_owned(), ErrorKind::Decode),
        (
            "text/event-stream",
            support::fixture("stream-basic.sse"),
            ErrorKind::Metadata,
        ),
        (
            "application/json",
            " ".repeat(MAX_MESSAGE_BYTES + 1),
            ErrorKind::Limit,
        ),
    ] {
        let server = TestServer::start(move |_: &Recorded| -> Reply {
            Reply::text(200, media, &body).header("request-id", "req_count_invalid")
        })
        .await;
        let error = api(&server)
            .messages()
            .count_tokens(hello().count_tokens())
            .send()
            .await
            .unwrap_err();
        assert_eq!(error.request_id(), Some("req_count_invalid"));
        assert_eq!(error.status().unwrap(), 200);
        let Error::Concord(error) = error else {
            panic!("expected decode/metadata/limit error")
        };
        assert_eq!(error.kind(), expected);
        assert_eq!(error.delivery(), Delivery::Answered);
        assert_eq!(error.endpoint(), Some("Anthropic.messages.count_tokens"));
        assert_no_second_request(&server).await;
        server.shutdown().await;
    }
}
