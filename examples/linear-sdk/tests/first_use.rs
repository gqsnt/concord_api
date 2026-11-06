//! Independent application consumer; provenance and predictions are in ../FIRST_USE.md.

use std::collections::VecDeque;
use std::num::NonZeroUsize;
use std::sync::Mutex;
use std::time::Duration;

use concord::{ErrorKind, reqwest};
use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use concord_pagination::{Outcome, PagesError, Resume};
use concord_sdk_linear::{Error, Linear};
use serde_json::{Value, json};

const KEY: &str = "synthetic-first-use-api-key";
const CURSOR: &str = "synthetic cursor/+==?&";
const PAGE_ONE: &[u8] = include_bytes!("../fixtures/first-use-page-one.json");
const PAGE_EMPTY: &[u8] = include_bytes!("../fixtures/first-use-page-empty.json");
const PAGE_LAST: &[u8] = include_bytes!("../fixtures/first-use-page-last.json");
const PAGE_PARTIAL: &[u8] = include_bytes!("../fixtures/first-use-page-partial.json");
const ERRORS: &[u8] = include_bytes!("../fixtures/first-use-errors.json");
const CONNECTION_NULL: &[u8] = include_bytes!("../fixtures/first-use-connection-null.json");
const ISSUE: &[u8] = include_bytes!("../fixtures/first-use-issue.json");
const ISSUE_PARTIAL: &[u8] = include_bytes!("../fixtures/first-use-issue-partial.json");
const ISSUE_ERRORS: &[u8] = include_bytes!("../fixtures/first-use-issue-errors.json");
const ISSUE_MISSING_DESCRIPTION: &[u8] =
    include_bytes!("../fixtures/first-use-issue-missing-description.json");
const MALFORMED: &[u8] = include_bytes!("../fixtures/first-use-malformed.json");

fn http() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap()
}

fn client(server: &TestServer) -> Linear {
    Linear::api_key(http(), KEY)
        .unwrap()
        .with_base_url(&server.url())
        .unwrap()
}

async fn fake(replies: Vec<Reply>) -> TestServer {
    let replies = Mutex::new(VecDeque::from(replies));
    TestServer::start(move |_| {
        replies
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected request: the SDK must not retry or over-fetch")
    })
    .await
}

fn json_reply(body: &[u8]) -> Reply {
    Reply::json(200, body).header("x-synthetic-request-id", "synthetic-response")
}

fn compact(document: &str) -> String {
    document
        .chars()
        .filter(|c| !c.is_whitespace() && *c != ',')
        .collect()
}

fn wire(request: &Recorded, auth: &str, expected_document_tail: &str) -> Value {
    assert_eq!(request.method, "POST");
    assert_eq!(request.target, "/graphql");
    assert_eq!(request.header("authorization"), Some(auth));
    assert_eq!(request.header("accept"), Some("application/json"));
    assert_eq!(request.header("content-type"), Some("application/json"));
    let body: Value = serde_json::from_slice(&request.body).unwrap();
    let query = body["query"].as_str().expect("GraphQL query string");
    // Operation names are an implementation detail. Check the entire declaration,
    // arguments, and selection after the optional name, ignoring GraphQL whitespace/commas.
    let (operation, document) = query.split_once('(').unwrap();
    let mut words = operation.split_whitespace();
    assert_eq!(words.next(), Some("query"));
    let name = words.next();
    assert!(words.next().is_none());
    if let Some(operation_name) = body.get("operationName") {
        assert_eq!(operation_name.as_str(), name);
    }
    assert_eq!(compact(document), compact(expected_document_tail));
    body["variables"].clone()
}

fn assert_list_requests(server: &TestServer, cursors: &[Option<&str>]) {
    let requests = server.requests();
    assert_eq!(requests.len(), cursors.len());
    for (request, after) in requests.iter().zip(cursors) {
        let variables = wire(
            request,
            KEY,
            "$first: Int!, $after: String) {
                issues(first: $first, after: $after) {
                    nodes { id title description dueDate }
                    pageInfo { hasNextPage endCursor }
                }
            }",
        );
        assert_eq!(variables["first"], 2);
        assert_eq!(variables["after"], json!(after));
        assert!(
            variables
                .as_object()
                .unwrap()
                .keys()
                .all(|key| { key == "first" || key == "after" })
        );
    }
}

#[tokio::test]
async fn pages_forward_opaque_cursors_through_empty_pages_and_exhaust_at_exact_cap() {
    let server = fake(vec![
        json_reply(PAGE_ONE),
        json_reply(PAGE_EMPTY),
        json_reply(PAGE_LAST),
    ])
    .await;
    let mut pages = client(&server)
        .issues(2, Resume::Start)
        .max_pages(NonZeroUsize::new(3).unwrap());
    assert!(server.requests().is_empty());

    let first = pages.next_page().await.unwrap().unwrap();
    assert_eq!(first.items.len(), 1);
    assert_eq!(first.items[0].id, "synthetic-issue-1");
    assert_eq!(first.items[0].title, "Review the draft");
    assert_eq!(
        first.items[0].description.as_deref(),
        Some("Consumer-owned fixture")
    );
    assert_eq!(first.next.as_deref(), Some(CURSOR));
    assert!(first.meta.errors.is_empty());
    assert_eq!(first.meta.http.status, 200);
    assert_eq!(
        first.meta.http.headers["x-synthetic-request-id"],
        "synthetic-response"
    );
    assert!(first.meta.http.operation.is_some());
    assert_eq!(first.meta.extensions.unwrap()["syntheticTrace"], "page-one");

    let empty = pages.next_page().await.unwrap().unwrap();
    assert!(empty.items.is_empty());
    assert_eq!(empty.next.as_deref(), Some("synthetic-cursor-2"));
    let last = pages.next_page().await.unwrap().unwrap();
    assert_eq!(last.items.len(), 1);
    assert_eq!(last.items[0].id, "synthetic-issue-2");
    assert_eq!(last.items[0].description, None);
    assert_eq!(last.next, None); // Ignore endCursor when hasNextPage is false.
    assert!(pages.checkpoint().is_none());
    assert!(pages.next_page().await.unwrap().is_none());
    assert!(pages.next_page().await.unwrap().is_none());
    let report = pages.finish();
    assert_eq!(report.outcome, Outcome::Exhausted);
    assert_eq!(report.progress.admitted, 3);
    assert_eq!(report.progress.succeeded, 3);
    assert_eq!(report.progress.failed, 0);
    assert_eq!(report.resume, None);
    assert_list_requests(&server, &[None, Some(CURSOR), Some("synthetic-cursor-2")]);
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn cap_reports_incomplete_then_saved_cursor_resumes_the_listing() {
    let server = fake(vec![json_reply(PAGE_ONE), json_reply(PAGE_LAST)]).await;
    let api = client(&server);
    let mut pages = api
        .issues(2, Resume::Start)
        .max_pages(NonZeroUsize::new(1).unwrap());
    assert_eq!(pages.next_page().await.unwrap().unwrap().items.len(), 1);
    let error = pages.next_page().await.unwrap_err();
    assert!(matches!(error, PagesError::Bounded { .. }));
    assert_eq!(error.outcome(), Outcome::Bounded);
    assert_eq!(error.progress().admitted, 1);
    let resume = error.resume().unwrap().clone();
    assert_eq!(resume, Resume::At(CURSOR.to_owned()));
    assert!(matches!(
        pages.next_page().await,
        Err(PagesError::Ended {
            outcome: Outcome::Bounded,
            ..
        })
    ));
    assert_eq!(server.requests().len(), 1);
    assert_eq!(pages.finish().outcome, Outcome::Bounded);

    let remaining = api.issues(2, resume).collect_complete().await.unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].items[0].id, "synthetic-issue-2");
    assert_list_requests(&server, &[None, Some(CURSOR)]);
    server.shutdown().await;
}

#[tokio::test]
async fn failed_page_preserves_collected_prefix_http_document_and_retry_position() {
    let server = fake(vec![
        json_reply(PAGE_ONE),
        Reply::json(503, ERRORS).header("retry-after", "7"),
    ])
    .await;
    let incomplete = client(&server)
        .issues(2, Resume::Start)
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected.len(), 1);
    assert_eq!(incomplete.collected[0].items[0].id, "synthetic-issue-1");
    assert_eq!(incomplete.error.outcome(), Outcome::Failed);
    assert_eq!(
        incomplete.error.resume(),
        Some(&Resume::At(CURSOR.to_owned()))
    );
    assert_eq!(incomplete.error.progress().admitted, 2);
    assert_eq!(incomplete.error.progress().succeeded, 1);
    assert_eq!(incomplete.error.progress().failed, 1);
    let PagesError::Provider {
        error: Error::Http { meta, body, errors },
        ..
    } = incomplete.error
    else {
        panic!("expected typed HTTP failure");
    };
    assert_eq!(meta.status, 503);
    assert_eq!(meta.headers["retry-after"], "7");
    assert!(meta.operation.is_some());
    assert_eq!(body, ERRORS);
    assert_eq!(errors[0].message, "Synthetic provider rejection");
    assert_eq!(errors[0].path, Some(vec![json!("issues")]));
    assert_list_requests(&server, &[None, Some(CURSOR)]);
    server.shutdown().await;
}

#[tokio::test]
async fn graphql_errors_without_data_are_page_failure_and_never_exhaustion() {
    let server = fake(vec![json_reply(ERRORS)]).await;
    let mut pages = client(&server).issues(2, Resume::Start);
    let error = pages.next_page().await.unwrap_err();
    assert_eq!(error.resume(), Some(&Resume::Start));
    let PagesError::Provider {
        error: Error::Page { response, .. },
        ..
    } = error
    else {
        panic!("expected retained GraphQL envelope");
    };
    assert_eq!(response.meta.status, 200);
    assert!(response.envelope.data.is_none());
    assert_eq!(
        response.envelope.errors[0].message,
        "Synthetic provider rejection"
    );
    assert_eq!(
        response.envelope.extensions.unwrap()["syntheticTrace"],
        "rejected"
    );
    assert!(matches!(
        pages.next_page().await,
        Err(PagesError::Ended {
            outcome: Outcome::Failed,
            ..
        })
    ));
    let report = pages.finish();
    assert_eq!(report.outcome, Outcome::Failed);
    assert_eq!(report.failed, Some(Resume::Start));
    assert_list_requests(&server, &[None]);
    server.shutdown().await;
}

#[tokio::test]
async fn partial_page_keeps_diagnostics_for_application_acceptance_policy() {
    let server = fake(vec![json_reply(PAGE_PARTIAL)]).await;
    let mut pages = client(&server).issues(2, Resume::Start);
    let page = pages.next_page().await.unwrap().unwrap();
    assert_eq!(page.items[0].id, "synthetic-issue-1");
    assert_eq!(page.items[0].description, None);
    let diagnostics = &page.meta.errors;
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(
        diagnostics[0].path,
        Some(vec![
            json!("issues"),
            json!("nodes"),
            json!(0),
            json!("description")
        ])
    );
    assert_eq!(
        diagnostics[0].locations,
        Some(vec![json!({"line":1,"column":95})])
    );
    assert_eq!(
        diagnostics[0].extensions.as_ref().unwrap()["code"],
        "SYNTHETIC_FIELD_FAILURE"
    );
    // This application's policy rejects a partial page. Traversal completion alone
    // does not certify that the data can be published.
    let accepted = if diagnostics.is_empty() {
        page.items
    } else {
        vec![]
    };
    assert!(accepted.is_empty());
    assert!(pages.next_page().await.unwrap().is_none());
    assert_eq!(pages.finish().outcome, Outcome::Exhausted);
    assert_list_requests(&server, &[None]);
    server.shutdown().await;
}

#[tokio::test]
async fn missing_connections_are_decode_failures_without_the_graphql_envelope() {
    let mut missing: Value = serde_json::from_slice(CONNECTION_NULL).unwrap();
    missing["data"].as_object_mut().unwrap().remove("issues");
    for body in [
        CONNECTION_NULL.to_vec(),
        serde_json::to_vec(&missing).unwrap(),
    ] {
        let server = fake(vec![json_reply(&body)]).await;
        let error = client(&server).issue_page(2, None).await.unwrap_err();
        // Observed contract gap: the README predicts Page for a missing connection.
        // Record the actual behavior; do not reconstruct or re-execute the request.
        let Error::Response { meta, source } = error else {
            panic!("expected the observed missing-connection decode failure");
        };
        assert_eq!(meta.status, 200);
        assert_eq!(meta.headers["x-synthetic-request-id"], "synthetic-response");
        assert_eq!(source.kind(), ErrorKind::Decode);
        assert_eq!(source.operation(), meta.operation);
        assert!(source.response().is_none());
        assert_list_requests(&server, &[None]);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn invalid_continuations_keep_the_received_page_and_stop() {
    for (after, next) in [
        (None, Value::Null),
        (None, json!("")),
        (Some(CURSOR), json!(CURSOR)),
    ] {
        let mut body: Value = serde_json::from_slice(PAGE_ONE).unwrap();
        body["data"]["issues"]["pageInfo"]["endCursor"] = next;
        let server = fake(vec![json_reply(&serde_json::to_vec(&body).unwrap())]).await;
        let from = after.map_or(Resume::Start, |s| Resume::At(s.to_owned()));
        let mut pages = client(&server).issues(2, from.clone());
        let error = pages.next_page().await.unwrap_err();
        assert_eq!(error.resume(), Some(&from));
        let PagesError::Provider {
            error: Error::Page { response, .. },
            ..
        } = error
        else {
            panic!("expected unusable cursor error");
        };
        assert_eq!(
            response.envelope.data.unwrap().issues.nodes[0].id,
            "synthetic-issue-1"
        );
        assert!(matches!(
            pages.next_page().await,
            Err(PagesError::Ended {
                outcome: Outcome::Failed,
                ..
            })
        ));
        assert_list_requests(&server, &[after]);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn application_stop_is_distinct_from_provider_exhaustion_and_does_not_prefetch() {
    let server = fake(vec![json_reply(PAGE_ONE)]).await;
    let mut pages = client(&server).issues(2, Resume::Start);
    pages.next_page().await.unwrap().unwrap();
    let report = pages.finish();
    assert_eq!(report.outcome, Outcome::Stopped);
    assert_eq!(report.resume, Some(Resume::At(CURSOR.to_owned())));
    assert_eq!(report.progress.admitted, 1);
    assert_list_requests(&server, &[None]);
    server.shutdown().await;
}

#[tokio::test]
async fn interrupted_page_body_is_not_success_even_when_json_is_complete() {
    let server = fake(vec![
        json_reply(PAGE_ONE),
        Reply::new(200)
            .header("content-type", "application/json")
            .header("x-synthetic-request-id", "synthetic-cutoff")
            .with_body(Body::FailAfter(vec![PAGE_LAST.to_vec().into()])),
    ])
    .await;
    let incomplete = client(&server)
        .issues(2, Resume::Start)
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected.len(), 1);
    assert_eq!(
        incomplete.error.resume(),
        Some(&Resume::At(CURSOR.to_owned()))
    );
    let PagesError::Provider {
        error: Error::Response { meta, source },
        ..
    } = incomplete.error
    else {
        panic!("expected interrupted response body");
    };
    assert_eq!(meta.status, 200);
    assert_eq!(meta.headers["x-synthetic-request-id"], "synthetic-cutoff");
    assert_eq!(source.kind(), ErrorKind::Transport);
    assert_eq!(source.operation(), meta.operation);
    assert_list_requests(&server, &[None, Some(CURSOR)]);
    server.shutdown().await;
}

#[tokio::test]
async fn invalid_page_sizes_fail_locally_without_a_request() {
    let server = fake(vec![]).await;
    let api = client(&server);
    for first in [0, 251] {
        assert!(matches!(
            api.issue_page(first, None).await,
            Err(Error::Invalid(_))
        ));
    }
    assert!(server.requests().is_empty());
    server.shutdown().await;
}

fn assert_issue_requests(server: &TestServer, auth: &str, ids: &[&str]) {
    let requests = server.requests();
    assert_eq!(requests.len(), ids.len());
    for (request, id) in requests.iter().zip(ids) {
        let variables = wire(
            request,
            auth,
            "$id: String!) { issue(id: $id) { id title description dueDate } }",
        );
        assert_eq!(variables, json!({"id": id}));
    }
}

#[tokio::test]
async fn selected_issue_fields_use_oauth_and_identifiers_stay_in_json_variables() {
    let server = fake(vec![json_reply(ISSUE), json_reply(ISSUE)]).await;
    let api = Linear::oauth(http(), "synthetic-first-use-oauth-token")
        .unwrap()
        .with_base_url(&server.url())
        .unwrap();
    // The second value is an adversarial encoding probe, not a valid Linear ID.
    let ids = ["SYNTHETIC-1", "synthetic-\"\\\n) { viewer { id } }"];
    for id in ids {
        let answer = api.issue(id).await.unwrap();
        assert!(answer.is_success());
        assert_eq!(answer.meta.status, 200);
        assert_eq!(
            answer.meta.headers["x-synthetic-request-id"],
            "synthetic-response"
        );
        assert!(answer.meta.operation.is_some());
        assert!(answer.envelope.errors.is_empty());
        assert_eq!(
            answer.envelope.extensions.unwrap()["syntheticTrace"],
            "issue"
        );
        let issue = answer.envelope.data.unwrap().issue.unwrap();
        assert_eq!(issue.id, "synthetic-issue-1");
        assert_eq!(issue.title, "Review the draft");
        assert_eq!(
            issue.description.as_deref(),
            Some("A selected description\nwith a second line.")
        );
    }
    assert_issue_requests(&server, "Bearer synthetic-first-use-oauth-token", &ids);
    let requests = server.requests();
    let first: Value = serde_json::from_slice(&requests[0].body).unwrap();
    let second: Value = serde_json::from_slice(&requests[1].body).unwrap();
    assert_eq!(first["query"], second["query"]);
    server.shutdown().await;
}

#[tokio::test]
async fn partial_issue_preserves_data_error_paths_locations_and_extensions() {
    let server = fake(vec![json_reply(ISSUE_PARTIAL)]).await;
    let answer = client(&server).issue("SYNTHETIC-1").await.unwrap();
    assert!(!answer.is_success());
    assert_eq!(answer.meta.status, 200);
    let issue = answer.envelope.data.unwrap().issue.unwrap();
    assert_eq!(issue.id, "synthetic-issue-1");
    assert_eq!(issue.title, "Review the draft");
    assert_eq!(issue.description, None);
    assert_eq!(answer.envelope.errors.len(), 1);
    let diagnostic = &answer.envelope.errors[0];
    assert_eq!(diagnostic.message, "Synthetic description unavailable");
    assert_eq!(
        diagnostic.path,
        Some(vec![json!("issue"), json!("description")])
    );
    assert_eq!(
        diagnostic.locations,
        Some(vec![json!({"line":1,"column":55})])
    );
    assert_eq!(
        diagnostic.extensions.as_ref().unwrap()["code"],
        "SYNTHETIC_FIELD_FAILURE"
    );
    assert_eq!(diagnostic.extra["syntheticExtra"], "retained");
    assert_eq!(
        answer.envelope.extensions.unwrap()["syntheticTrace"],
        "partial-issue"
    );
    assert_issue_requests(&server, KEY, &["SYNTHETIC-1"]);
    server.shutdown().await;
}

#[tokio::test]
async fn successful_envelope_does_not_mean_a_nullable_issue_exists() {
    let body = serde_json::to_vec(&json!({"data":{"issue":null}})).unwrap();
    let server = fake(vec![json_reply(&body)]).await;
    let answer = client(&server).issue("SYNTHETIC-ABSENT").await.unwrap();
    assert!(answer.is_success()); // Documented envelope policy, not resource existence.
    assert!(answer.envelope.data.unwrap().issue.is_none());
    assert_issue_requests(&server, KEY, &["SYNTHETIC-ABSENT"]);
    server.shutdown().await;
}

#[tokio::test]
async fn issue_error_envelopes_keep_diagnostics_for_absent_and_null_data() {
    let mut null_data: Value = serde_json::from_slice(ISSUE_ERRORS).unwrap();
    null_data["data"] = Value::Null;
    for body in [
        ISSUE_ERRORS.to_vec(),
        serde_json::to_vec(&null_data).unwrap(),
    ] {
        let server = fake(vec![json_reply(&body)]).await;
        let answer = client(&server).issue("SYNTHETIC-1").await.unwrap();
        assert!(!answer.is_success());
        assert!(answer.envelope.data.is_none());
        assert_eq!(answer.envelope.errors.len(), 1);
        assert_eq!(
            answer.envelope.errors[0].message,
            "Synthetic issue unavailable"
        );
        assert_eq!(answer.envelope.errors[0].path, Some(vec![json!("issue")]));
        assert_eq!(
            answer.envelope.extensions.unwrap()["syntheticTrace"],
            "issue-error"
        );
        assert_issue_requests(&server, KEY, &["SYNTHETIC-1"]);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn http_rejections_keep_status_headers_and_exact_body_without_retrying() {
    for (status, content_type, body, expected_errors) in [
        (429, "application/json", ISSUE_ERRORS, 1),
        (
            502,
            "text/plain",
            b"synthetic gateway failure".as_slice(),
            0,
        ),
    ] {
        let server = fake(vec![
            Reply::text(status, content_type, body)
                .header("retry-after", "11")
                .header("x-synthetic-request-id", "synthetic-http-failure"),
        ])
        .await;
        let error = client(&server).issue("SYNTHETIC-1").await.unwrap_err();
        let Error::Http {
            meta,
            body: received,
            errors,
        } = error
        else {
            panic!("expected typed HTTP rejection");
        };
        assert_eq!(meta.status.as_u16(), status);
        assert_eq!(meta.headers["retry-after"], "11");
        assert_eq!(
            meta.headers["x-synthetic-request-id"],
            "synthetic-http-failure"
        );
        assert!(meta.operation.is_some());
        assert_eq!(received, body);
        assert_eq!(errors.len(), expected_errors);
        if let Some(diagnostic) = errors.first() {
            assert_eq!(diagnostic.message, "Synthetic issue unavailable");
            assert_eq!(
                diagnostic.extensions.as_ref().unwrap()["code"],
                "SYNTHETIC_ISSUE_FAILURE"
            );
        }
        assert_issue_requests(&server, KEY, &["SYNTHETIC-1"]);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn bad_media_json_or_missing_selected_fields_fail_with_response_metadata() {
    for (content_type, body, kind) in [
        ("text/html", ISSUE, ErrorKind::Metadata),
        ("application/json", MALFORMED, ErrorKind::Decode),
        (
            "application/json",
            ISSUE_MISSING_DESCRIPTION,
            ErrorKind::Decode,
        ),
    ] {
        let server =
            fake(vec![Reply::text(200, content_type, body).header(
                "x-synthetic-request-id",
                "synthetic-invalid-response",
            )])
            .await;
        let error = client(&server).issue("SYNTHETIC-1").await.unwrap_err();
        let Error::Response { meta, source } = error else {
            panic!("expected a response error, not a successful issue");
        };
        assert_eq!(meta.status, 200);
        assert_eq!(
            meta.headers["x-synthetic-request-id"],
            "synthetic-invalid-response"
        );
        assert!(meta.operation.is_some());
        assert_eq!(source.kind(), kind);
        assert_eq!(source.operation(), meta.operation);
        if kind == ErrorKind::Metadata {
            assert_eq!(source.response().unwrap().status(), 200);
        }
        assert_issue_requests(&server, KEY, &["SYNTHETIC-1"]);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn interrupted_issue_body_cannot_publish_a_complete_json_prefix() {
    let server = fake(vec![
        Reply::new(200)
            .header("content-type", "application/json")
            .header("x-synthetic-request-id", "synthetic-cutoff")
            .with_body(Body::FailAfter(vec![ISSUE.to_vec().into()])),
    ])
    .await;
    let error = client(&server).issue("SYNTHETIC-1").await.unwrap_err();
    let Error::Response { meta, source } = error else {
        panic!("HTTP framing must complete before returning an issue");
    };
    assert_eq!(meta.status, 200);
    assert_eq!(meta.headers["x-synthetic-request-id"], "synthetic-cutoff");
    assert_eq!(source.kind(), ErrorKind::Transport);
    assert_eq!(source.operation(), meta.operation);
    assert_issue_requests(&server, KEY, &["SYNTHETIC-1"]);
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn oversized_success_and_error_bodies_fail_instead_of_returning_truncated_data() {
    for (status, limit) in [(200, 1024 * 1024), (429, 64 * 1024)] {
        // Valid JSON, padded with legal JSON whitespace to one byte beyond the
        // documented bound. Generated sizes, rather than large checked-in fixtures.
        let mut body = if status == 200 {
            ISSUE.to_vec()
        } else {
            ISSUE_ERRORS.to_vec()
        };
        body.resize(limit + 1, b' ');
        let server = fake(vec![
            Reply::json(status, body).header("x-synthetic-request-id", "synthetic-too-large"),
        ])
        .await;
        let error = client(&server).issue("SYNTHETIC-1").await.unwrap_err();
        let Error::Response { meta, source } = error else {
            panic!("expected the documented bounded-body failure");
        };
        assert_eq!(meta.status.as_u16(), status);
        assert_eq!(
            meta.headers["x-synthetic-request-id"],
            "synthetic-too-large"
        );
        assert_eq!(source.kind(), ErrorKind::Limit);
        assert_eq!(source.operation(), meta.operation);
        assert_issue_requests(&server, KEY, &["SYNTHETIC-1"]);
        assert_eq!(server.shutdown().await.open_after, 0);
    }
}
