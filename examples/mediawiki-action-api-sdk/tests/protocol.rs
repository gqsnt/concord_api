//! Wire contract, bounded failure classes and secret-safe diagnostics on local fixtures.

mod support;

use std::error::Error as _;

use concord_sdk_mediawiki_action_api::{
    BODY_LIMIT, Continuation, ERROR_BODY_LIMIT, Error, MediaWiki,
    concord::{ErrorKind, bytes::Bytes},
};
use concord_testserver::{Body, Recorded, Reply, TestServer};
use support::*;

const SECRET: &str = "synthetic-secret-from-provider";

fn no_secret(value: impl std::fmt::Debug + std::fmt::Display) {
    assert!(!format!("{value:?} {value}").contains(SECRET));
}

fn core(error: &Error, expected: ErrorKind) {
    let Error::Concord(error) = error else {
        panic!("expected Concord failure: {error}")
    };
    assert_eq!(error.kind(), expected);
    assert_eq!(error.endpoint(), Some("MediaWiki.query.search"));
    no_secret(error);
}

fn query_is(request: &Recorded, expected: &[(&str, &str)]) {
    let mut actual = request.query_pairs();
    actual.sort();
    let mut expected: Vec<_> = expected
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    expected.sort();
    assert_eq!(actual, expected); // Includes pair count: duplicates cannot disappear into a map.
}

#[tokio::test]
async fn all_three_endpoints_have_documented_query_shapes_and_identifying_header() {
    let server = TestServer::start(|request: &Recorded| {
        assert_eq!(request.method, "GET");
        assert_eq!(request.path(), "/w/api.php");
        assert!(request.body.is_empty());
        assert_eq!(
            request
                .headers
                .iter()
                .filter(|(k, _)| k == "user-agent")
                .count(),
            1
        );
        assert!(
            request
                .headers
                .iter()
                .any(|(k, v)| k == "user-agent" && v == UA)
        );
        assert!(
            request
                .headers
                .iter()
                .any(|(k, v)| k == "accept" && v == "application/json")
        );
        assert!(
            !request
                .headers
                .iter()
                .any(|(k, _)| k == "authorization" || k == "content-type")
        );
        match param(request, "list").as_deref() {
            Some("search") => {
                query_is(
                    request,
                    &[
                        ("action", "query"),
                        ("format", "json"),
                        ("formatversion", "2"),
                        ("srsearch", "Rust & café/?"),
                        ("srlimit", "10"),
                        ("list", "search"),
                        ("srprop", ""),
                    ],
                );
                assert!(request.target.contains("Rust+%26+caf%C3%A9%2F%3F"));
                Reply::json(200, SEARCH)
            }
            Some("categorymembers") => {
                query_is(
                    request,
                    &[
                        ("action", "query"),
                        ("format", "json"),
                        ("formatversion", "2"),
                        ("cmtitle", "Category:Rust"),
                        ("cmlimit", "50"),
                        ("list", "categorymembers"),
                    ],
                );
                Reply::json(200, CATEGORY_LAST)
            }
            _ => {
                query_is(
                    request,
                    &[
                        ("action", "query"),
                        ("format", "json"),
                        ("formatversion", "2"),
                        ("pageids", "2"),
                        ("prop", "revisions"),
                        ("rvprop", "ids|content"),
                        ("rvslots", "main"),
                        ("rvlimit", "1"),
                    ],
                );
                Reply::json(200, CONTENT)
            }
        }
    })
    .await;
    let query = api(&server.url()).query();
    query.search("Rust & café/?").send().await.unwrap();
    query
        .category_members("Category:Rust")
        .send()
        .await
        .unwrap();
    query.fetch_page(2).await.unwrap();
    assert_eq!(server.requests().len(), 3);
    server.shutdown().await;
}

#[tokio::test]
async fn build_errors_and_client_request_diagnostics_withhold_user_agent() {
    MediaWiki::validate_header_names().unwrap();
    let server = TestServer::start(|_: &Recorded| panic!("invalid header must not send")).await;
    let client = api_with_user_agent(&server.url(), SECRET);
    let scope = client.query();
    let operation = scope.search("Rust");
    let prepared = operation.clone().prepare().unwrap();
    for shown in [
        format!("{client:?}"),
        format!("{scope:?}"),
        format!("{operation:?}"),
        format!("{prepared:?}"),
    ] {
        assert!(!shown.contains(SECRET));
    }
    assert!(prepared.request().headers()["user-agent"].is_sensitive());
    let error = api_with_user_agent(&server.url(), &format!("{SECRET}\n"))
        .query()
        .search("Rust")
        .send()
        .await
        .unwrap_err();
    core(&error, ErrorKind::Build);
    no_secret(error);
    assert!(server.requests().is_empty());
    server.shutdown().await;
}

#[tokio::test]
async fn transport_failures_before_head_and_mid_body_are_distinct_from_json_truncation() {
    for mid_body in [false, true] {
        let server = TestServer::start(move |_: &Recorded| {
            if mid_body {
                Reply::new(200)
                    .header("content-type", "application/json")
                    .with_body(Body::FailAfter(vec![Bytes::from_static(b"{\"query\":")]))
            } else {
                Reply::hang_up()
            }
        })
        .await;
        let error = api(&server.url())
            .query()
            .search(SECRET)
            .send()
            .await
            .unwrap_err();
        core(&error, ErrorKind::Transport);
        let Error::Concord(ref detail) = error else {
            unreachable!()
        };
        assert_eq!(detail.head().is_some(), mid_body);
        assert!(error.source().is_none());
        no_secret(error);
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn malformed_truncated_empty_and_wrong_shape_json_are_decode_failures() {
    for body in [
        include_str!("../fixtures/malformed.json"),
        include_str!("../fixtures/truncated.json"),
        include_str!("../fixtures/wrong-shape.json"),
        "",
        "null",
    ] {
        let server = TestServer::start(move |_: &Recorded| Reply::json(200, body)).await;
        let error = api(&server.url())
            .query()
            .search("Rust")
            .send()
            .await
            .unwrap_err();
        core(&error, ErrorKind::Decode);
        let Error::Concord(ref detail) = error else {
            unreachable!()
        };
        assert_eq!(detail.head().unwrap().status().as_u16(), 200);
        no_secret(error);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn wrong_missing_duplicate_and_wrong_charset_media_types_are_refused() {
    for mode in 0..5 {
        let server = TestServer::start(move |_: &Recorded| {
            let reply = Reply::new(200).body(SEARCH);
            match mode {
                0 => reply.header("content-type", "text/html"),
                1 => reply,
                2 => reply
                    .header("content-type", "application/json")
                    .header("content-type", "application/json"),
                3 => reply.header("content-type", "application/json; charset=iso-8859-1"),
                _ => reply.header("content-type", "not a media type"),
            }
        })
        .await;
        let error = api(&server.url())
            .query()
            .search("Rust")
            .send()
            .await
            .unwrap_err();
        core(&error, ErrorKind::Metadata);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn success_limit_is_enforced_by_policy_and_prepared_send() {
    let server =
        TestServer::start(|_: &Recorded| Reply::json(200, vec![b' '; BODY_LIMIT + 1])).await;
    let query = api(&server.url()).query();
    let error = query.search("Rust").send().await.unwrap_err();
    core(&error, ErrorKind::Limit);
    let error = query
        .search("Rust")
        .prepare()
        .unwrap()
        .send()
        .await
        .err()
        .unwrap();
    assert_eq!(error.kind(), ErrorKind::Limit);
    server.shutdown().await;
}

#[tokio::test]
async fn http_status_stays_authoritative_for_every_error_body_failure() {
    // 429 and 5xx are final outcomes, not automatic retries. All bodies are synthetic.
    for mode in 0..7 {
        let status = if mode == 0 { 429 } else { 503 };
        let server = TestServer::start(move |_: &Recorded| {
            let reply = Reply::new(status);
            match mode {
                0 => Reply::json(status, API_ERROR).header("retry-after", "3"),
                1 => Reply::json(status, include_str!("../fixtures/malformed.json")),
                2 => Reply::text(status, "text/html", SECRET),
                3 => Reply::json(status, vec![b' '; ERROR_BODY_LIMIT + 1]),
                4 => reply
                    .header("content-type", "application/json")
                    .with_body(Body::FailAfter(vec![Bytes::from_static(b"{\"error\":")])),
                5 => Reply::json(status, "{}"),
                _ => Reply::json(status, ""),
            }
        })
        .await;
        let error = api(&server.url())
            .query()
            .search(SECRET)
            .send()
            .await
            .unwrap_err();
        let Error::Http(ref failure) = error else {
            panic!("HTTP failure retained")
        };
        assert_eq!(failure.meta.status().as_u16(), status);
        assert_eq!(
            failure.meta.operation().unwrap().name(),
            "MediaWiki.query.search"
        );
        if mode == 0 {
            assert_eq!(
                failure.document.as_ref().unwrap().error.code,
                "synthetic-failure"
            );
            assert_eq!(failure.meta.headers()["retry-after"], "3");
        } else {
            let expected = match mode {
                2 => ErrorKind::Metadata,
                3 => ErrorKind::Limit,
                4 => ErrorKind::Transport,
                _ => ErrorKind::Decode,
            };
            assert_eq!(failure.document.as_ref().unwrap_err().kind(), expected);
        }
        assert!(error.source().is_none());
        no_secret(error);
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn api_errors_warnings_and_missing_query_are_never_success() {
    for mode in 0..3 {
        let server = TestServer::start(move |_: &Recorded| {
            Reply::json(
                200,
                match mode {
                    0 => API_ERROR,
                    1 => include_str!("../fixtures/warnings.json"),
                    _ => include_str!("../fixtures/missing-query.json"),
                },
            )
            .header("set-cookie", "synthetic-secret-from-provider")
        })
        .await;
        let error = api(&server.url())
            .query()
            .search("Rust")
            .send()
            .await
            .unwrap_err();
        match (&error, mode) {
            (Error::Api { meta, problem }, 0) => {
                assert_eq!(meta.status().as_u16(), 200);
                assert_eq!(problem.info, SECRET); // Explicit access works; formatting is redacted.
            }
            (Error::Warnings(_), 1) | (Error::InvalidResponse(_), 2) => {}
            _ => panic!("wrong envelope classification"),
        }
        no_secret(error);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn malformed_continuations_fail_instead_of_silently_truncating() {
    for token in [
        "{}",
        "null",
        r#"{"cmcontinue":"x"}"#,
        r#"{"continue":1}"#,
        r#"{"continue":"-||","cmcontinue":true}"#,
        r#"{"continue":"-||","cmcontinue":{}}"#,
        r#"{"continue":"-||","futureoffset":1.5}"#,
        r#"{"continue":"-||","action":"edit"}"#,
        r#"{"continue":"-||","":1}"#,
    ] {
        // A null continuation needs a wire-presence check; it must not mean exhaustion.
        let body = format!(r#"{{"continue":{token},"query":{{"search":[]}}}}"#);
        let server = TestServer::start(move |_: &Recorded| Reply::json(200, &body)).await;
        let error = api(&server.url())
            .query()
            .search("Rust")
            .send()
            .await
            .unwrap_err();
        core(&error, ErrorKind::Decode);
        server.shutdown().await;
    }
}

#[test]
fn continuation_diagnostics_withhold_values_and_accept_future_integer_fields() {
    let value: Continuation = serde_json::from_value(serde_json::json!({
        "continue": "-||", "cmcontinue": SECRET, "futureoffset": 7
    }))
    .unwrap();
    assert_eq!(value.fields().len(), 3);
    assert_eq!(value.fields()["futureoffset"], "7");
    no_secret(value);
}

#[tokio::test]
async fn missing_suppressed_or_inconsistent_content_is_refused_and_empty_content_is_valid() {
    for mode in 0..7 {
        let body = match mode {
            0 => include_str!("../fixtures/missing-page.json").to_owned(),
            1 => include_str!("../fixtures/hidden-content.json").to_owned(),
            2 => r#"{"query":{"pages":[]}}"#.to_owned(),
            3 => CONTENT.replace("\"pageid\": 2", "\"pageid\": 99"),
            4 => r#"{"query":{"pages":[{"title":"Invalid","invalid":true}]}}"#.to_owned(),
            5 => r#"{"query":{"pages":[{"pageid":2,"title":"No revision"}]}}"#.to_owned(),
            _ => CONTENT.replace("== Rust ==\\nA synthetic [[programming language]].\\n", ""),
        };
        let server = TestServer::start(move |_: &Recorded| Reply::json(200, &body)).await;
        let result = api(&server.url()).query().fetch_page(2).await;
        if mode == 6 {
            assert_eq!(result.unwrap().content, "");
        } else if mode == 2 {
            assert!(matches!(result, Err(Error::InvalidResponse(_))));
        } else {
            assert!(matches!(result, Err(Error::ContentUnavailable)));
        }
        server.shutdown().await;
    }
}
