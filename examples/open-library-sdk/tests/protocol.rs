//! Wire expectations come from the cited endpoint contracts; failure bodies are synthetic.

mod support;

use std::{error::Error as _, num::NonZeroU32, time::Duration};

use concord_example_testserver::{Body, Recorded, Reply, Script, TestServer, paused};
use concord_sdk_open_library::{
    AuthorId, ERROR_BODY_LIMIT, Error, OpenLibrary, RESPONSE_BODY_LIMIT, RecordKind, WorkId,
    concord::{ErrorKind, bytes::Bytes, reqwest::StatusCode},
};

const REQUEST_SECRET: &str = "synthetic-request-secret";
const RESPONSE_SECRET: &str = "synthetic-response-secret";

#[tokio::test]
async fn native_send_keeps_the_declared_success_body_limit() {
    let server = TestServer::start(|_| Reply::json(200, vec![b' '; RESPONSE_BODY_LIMIT + 1])).await;
    let error = support::client(&server)
        .search()
        .works("fixture")
        .prepare()
        .unwrap()
        .send()
        .await
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Limit);
    assert_eq!(error.head().unwrap().status(), StatusCode::OK);
    server.shutdown().await;
}

fn assert_redacted(value: impl std::fmt::Debug) {
    let shown = format!("{value:?}");
    assert!(!shown.contains(REQUEST_SECRET), "request leaked: {shown}");
    assert!(!shown.contains(RESPONSE_SECRET), "response leaked: {shown}");
}

fn concord_error(error: &Error, expected: ErrorKind) -> &concord_sdk_open_library::concord::Error {
    let Error::Concord(inner) = error else {
        panic!("expected Concord error: {error:?}")
    };
    assert_eq!(inner.kind(), expected);
    assert_eq!(inner.endpoint(), Some("OpenLibrary.search.works"));
    assert_redacted(error);
    assert!(!error.to_string().contains(REQUEST_SECRET));
    assert!(!error.to_string().contains(RESPONSE_SECRET));
    assert!(error.source().is_none());
    inner
}

async fn search_error(reply: Reply) -> Error {
    let script = Script::new([reply]);
    let server = TestServer::start(move |_: &Recorded| script.next()).await;
    let error = support::client(&server)
        .search()
        .works(REQUEST_SECRET)
        .send()
        .await
        .unwrap_err();
    assert_eq!(server.requests().len(), 1, "no policy retries");
    assert_eq!(server.shutdown().await.open_after, 0);
    error
}

#[tokio::test]
async fn wire_shapes_are_endpoint_specific_and_queries_are_encoded() {
    OpenLibrary::validate_header_names().unwrap();
    let server = TestServer::start(|request: &Recorded| match request.path() {
        "/prefix/search.json" => {
            Reply::json(200, include_str!("../fixtures/work-search-bare.json"))
        }
        "/prefix/search/authors.json" => {
            Reply::json(200, include_str!("../fixtures/author-search.json"))
        }
        "/prefix/works/OL27448W.json" => {
            Reply::json(200, include_str!("../fixtures/work-rich.json"))
        }
        "/prefix/authors/OL23919A/works.json" => {
            Reply::json(200, include_str!("../fixtures/author-works.json"))
        }
        _ => Reply::new(500),
    })
    .await;
    let api = support::client(&server)
        .with_base_url(format!("{}/prefix", server.url()))
        .unwrap();
    let page = api.search().works("a &/é?=").send().await.unwrap();
    assert_eq!(page.num_found, Some(1), "snake-case count alias");
    let work = api
        .works()
        .get(page.docs[0].id().unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(work.description.unwrap()["value"], "An authored fixture.");
    assert_eq!(work.subjects.unwrap(), ["Fixture subject"]);
    api.search().authors("a &/é?=").send().await.unwrap();
    api.author(AuthorId::parse("/authors/OL23919A").unwrap())
        .works()
        .set_limit(NonZeroU32::new(3).unwrap())
        .set_offset(50u64)
        .send()
        .await
        .unwrap();
    api.author(AuthorId::parse("OL23919A").unwrap())
        .works()
        .set_limit(NonZeroU32::MIN)
        .set_offset(0u64)
        .maybe_limit(None)
        .maybe_offset(None)
        .send()
        .await
        .unwrap();

    let requests = server.requests();
    assert_eq!(requests.len(), 5);
    assert_eq!(
        requests[0].target,
        "/prefix/search.json?q=a+%26%2F%C3%A9%3F%3D&page=1&limit=20"
    );
    assert_eq!(requests[0].query_all("q"), ["a &/é?="]);
    assert_eq!(requests[1].target, "/prefix/works/OL27448W.json");
    assert_eq!(
        requests[2].target,
        "/prefix/search/authors.json?q=a+%26%2F%C3%A9%3F%3D"
    );
    assert_eq!(
        requests[3].target,
        "/prefix/authors/OL23919A/works.json?limit=3&offset=50"
    );
    assert_eq!(requests[4].target, "/prefix/authors/OL23919A/works.json");
    for request in requests {
        assert_eq!(request.method, "GET");
        assert_eq!(request.header_all("accept"), ["application/json"]);
        assert_eq!(request.header("content-type"), None);
        assert_eq!(request.header("authorization"), None);
        assert_eq!(request.header("cookie"), None);
        assert_eq!(request.body_len, 0);
    }
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[test]
fn identifiers_normalize_without_accepting_urls_slugs_or_injection() {
    assert_eq!(
        WorkId::parse("/works/OL27448W").unwrap().as_str(),
        "OL27448W"
    );
    assert_eq!(
        "OL27448W".parse::<WorkId>().unwrap().to_string(),
        "OL27448W"
    );
    assert_eq!(
        AuthorId::parse("/authors/OL23919A").unwrap().as_str(),
        "OL23919A"
    );
    for invalid in [
        "",
        "..",
        "OLW",
        "OL12A",
        "OL1/2W",
        "OL12W.json",
        "/works/OL12W/slug",
        "https://untrusted.invalid/works/OL12W",
        "OL１２W",
        "OL12W?token=synthetic-request-secret",
    ] {
        let error = WorkId::parse(invalid).unwrap_err();
        assert!(matches!(error, Error::InvalidId(RecordKind::Work)));
        assert_eq!(error.status(), None);
        assert_redacted(&error);
        assert!(!error.to_string().contains(REQUEST_SECRET));
    }
    for invalid in [
        "",
        "OLA",
        "OL12W",
        "/works/OL12A",
        "/authors/OL12A/slug",
        "OL12A.json",
    ] {
        assert!(matches!(
            AuthorId::parse(invalid),
            Err(Error::InvalidId(RecordKind::Author))
        ));
    }
}

#[tokio::test]
async fn missing_selected_ids_are_input_errors_without_followup_requests() {
    let server = TestServer::start(|request: &Recorded| match request.path() {
        "/search.json" => Reply::json(200, r#"{"docs":[{"title":"unknown"}]}"#),
        "/search/authors.json" => Reply::json(200, r#"{"docs":[{"name":"unknown"}]}"#),
        _ => Reply::new(500),
    })
    .await;
    let api = support::client(&server);
    assert!(matches!(
        api.search().works("x").send().await.unwrap().docs[0].id(),
        Err(Error::InvalidId(RecordKind::Work))
    ));
    assert!(matches!(
        api.search().authors("x").send().await.unwrap().docs[0].id(),
        Err(Error::InvalidId(RecordKind::Author))
    ));
    assert_eq!(server.requests().len(), 2);
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn generated_request_and_prepared_debug_redact_search_text() {
    let server =
        TestServer::start(|_: &Recorded| Reply::json(200, include_str!("../fixtures/error.json")))
            .await;
    let api = support::client(&server);
    assert_redacted(&api);
    assert_redacted(api.search());
    let request = api.search().works(REQUEST_SECRET);
    assert_redacted(&request);
    let prepared = request.prepare().unwrap();
    assert_redacted(&prepared);
    assert!(
        prepared.request().url().as_str().contains(REQUEST_SECRET),
        "redaction does not change the wire"
    );
    let author = api.search().authors(REQUEST_SECRET);
    assert_redacted(&author);
    assert_redacted(author.prepare().unwrap());
    assert!(server.requests().is_empty(), "prepare is I/O-free");
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn prepared_send_has_concord_status_semantics_and_bypasses_sdk_policy() {
    let server =
        TestServer::start(|_: &Recorded| Reply::json(404, include_str!("../fixtures/error.json")))
            .await;
    let error = support::client(&server)
        .search()
        .works(REQUEST_SECRET)
        .prepare()
        .unwrap()
        .send()
        .await
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Status);
    assert_eq!(error.status(), Some(StatusCode::NOT_FOUND));
    assert!(error.response().is_some());
    assert_redacted(&error);
    assert!(!error.to_string().contains(REQUEST_SECRET));
    assert_eq!(server.requests().len(), 1);
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn build_failure_remains_redacted_and_sends_nothing() {
    let server = TestServer::start(|_: &Recorded| Reply::new(500)).await;
    let error = support::client(&server)
        .with_base_url("not-a-url-synthetic-request-secret")
        .unwrap_err();
    let wrapped = Error::from(error);
    let Error::Concord(inner) = &wrapped else {
        unreachable!()
    };
    assert_eq!(inner.kind(), ErrorKind::Build);
    assert_eq!(wrapped.status(), None);
    assert_redacted(&wrapped);
    assert!(!wrapped.to_string().contains(REQUEST_SECRET));
    assert!(server.requests().is_empty());
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn all_non_success_statuses_keep_status_and_opaque_json_without_retry() {
    for status in [301, 400, 401, 403, 404, 408, 422, 429, 500, 503] {
        let error = search_error(
            Reply::json(status, include_str!("../fixtures/error.json"))
                .header("set-cookie", RESPONSE_SECRET)
                .header("retry-after", "120")
                .header("location", "https://untrusted.invalid/never-follow"),
        )
        .await;
        assert_eq!(error.status().unwrap().as_u16(), status);
        assert_redacted(&error);
        assert!(!error.to_string().contains(RESPONSE_SECRET));
        assert!(error.source().is_none());
        let Error::Http(http) = error else {
            panic!("status must stay authoritative")
        };
        assert_eq!(http.meta().headers()["retry-after"], "120");
        assert_eq!(
            http.meta().operation().unwrap().name(),
            "OpenLibrary.search.works"
        );
        assert_eq!(
            http.document_unredacted().unwrap()["error"],
            RESPONSE_SECRET
        );
        assert_redacted(&http);
        assert!(!http.to_string().contains(RESPONSE_SECRET));
        assert!(http.source().is_none());
    }
}

#[tokio::test]
async fn malformed_truncated_wrong_shape_and_missing_envelope_are_decode_errors() {
    for body in [
        include_str!("../fixtures/malformed.json"),
        include_str!("../fixtures/truncated.json"),
        include_str!("../fixtures/wrong-shape.json"),
        include_str!("../fixtures/missing-envelope.json"),
        "",
        "null",
    ] {
        let error =
            search_error(Reply::json(200, body).header("set-cookie", RESPONSE_SECRET)).await;
        let inner = concord_error(&error, ErrorKind::Decode);
        assert_eq!(error.status(), Some(StatusCode::OK));
        assert!(inner.head().is_some());
        assert!(inner.codec_error().is_some());
    }
}

#[tokio::test]
async fn media_refusal_happens_before_a_stalled_body_is_read() {
    let replies = [
        Reply::text(
            200,
            "text/html",
            include_str!("../fixtures/wrong-media.html"),
        ),
        Reply::new(200),
        Reply::json(200, "").header("content-type", "application/json"),
        Reply::text(200, "application/json; charset=iso-8859-1", ""),
        Reply::text(200, "malformed", ""),
    ];
    for reply in replies {
        // Clock frozen while the call runs: the bound fails only a call that waits for something
        // that never comes (then reported as stuck).
        let error = tokio::time::timeout(
            Duration::from_secs(1),
            paused::frozen(search_error(reply.with_body(Body::Stall))),
        )
        .await
        .expect("media refusal must not wait for the body");
        let inner = concord_error(&error, ErrorKind::Metadata);
        assert_eq!(error.status(), Some(StatusCode::OK));
        assert!(inner.response().is_some(), "refused body remains unread");
    }
}

#[tokio::test]
async fn oversize_success_is_limited_for_content_length_and_chunked_bodies() {
    for reply in [
        Reply::json(200, vec![b' '; RESPONSE_BODY_LIMIT + 1]),
        Reply::json(200, "").with_body(Body::Chunks(vec![
            (Duration::ZERO, Bytes::from_static(b"{\"docs\":[]}")),
            (Duration::ZERO, Bytes::from(vec![b' '; RESPONSE_BODY_LIMIT])),
        ])),
    ] {
        let error = search_error(reply).await;
        concord_error(&error, ErrorKind::Limit);
        assert_eq!(error.status(), Some(StatusCode::OK));
    }
}

#[tokio::test]
async fn exact_size_limits_and_structured_json_media_are_accepted() {
    let mut success = b"{\"docs\":[]}".to_vec();
    success.resize(RESPONSE_BODY_LIMIT, b' ');
    let mut failure = b"{}".to_vec();
    failure.resize(ERROR_BODY_LIMIT, b' ');
    let script = Script::new([
        Reply::text(200, "application/vnd.fixture+json; charset=utf-8", success),
        Reply::json(404, failure),
    ]);
    let server = TestServer::start(move |_: &Recorded| script.next()).await;
    let api = support::client(&server);
    assert!(
        api.search()
            .works("x")
            .send()
            .await
            .unwrap()
            .docs
            .is_empty()
    );
    let Error::Http(http) = api.search().works("x").send().await.unwrap_err() else {
        panic!("HTTP error")
    };
    assert_eq!(http.document_unredacted().unwrap(), &serde_json::json!({}));
    assert_eq!(server.requests().len(), 2);
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn transport_loss_before_head_or_after_complete_json_never_succeeds() {
    // Complete JSON is insufficient when the HTTP chunked transfer is interrupted.
    let error = search_error(
        Reply::json(200, "").with_body(Body::FailAfter(vec![Bytes::from_static(b"{\"docs\":[]}")])),
    )
    .await;
    concord_error(&error, ErrorKind::Transport);
    assert_eq!(error.status(), Some(StatusCode::OK));

    let server = TestServer::start(|_: &Recorded| Reply::hang_up()).await;
    let error = support::client(&server)
        .search()
        .works(REQUEST_SECRET)
        .send()
        .await
        .unwrap_err();
    concord_error(&error, ErrorKind::Transport);
    assert_eq!(error.status(), None);
    assert_eq!(server.requests().len(), 1);
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn http_status_survives_every_error_document_failure() {
    let cases = [
        (
            Reply::json(404, include_str!("../fixtures/malformed.json")),
            ErrorKind::Decode,
        ),
        (
            Reply::json(404, include_str!("../fixtures/truncated.json")),
            ErrorKind::Decode,
        ),
        (Reply::json(404, ""), ErrorKind::Decode),
        (
            Reply::text(
                404,
                "text/html",
                include_str!("../fixtures/wrong-media.html"),
            ),
            ErrorKind::Metadata,
        ),
        (Reply::new(404), ErrorKind::Metadata),
        (
            Reply::json(404, vec![b' '; ERROR_BODY_LIMIT + 1]),
            ErrorKind::Limit,
        ),
        (
            Reply::json(404, "").with_body(Body::FailAfter(vec![Bytes::from_static(b"{}")])),
            ErrorKind::Transport,
        ),
    ];
    for (reply, expected) in cases {
        let error = search_error(reply.header("set-cookie", RESPONSE_SECRET)).await;
        assert_eq!(error.status(), Some(StatusCode::NOT_FOUND));
        assert_redacted(&error);
        assert!(!error.to_string().contains(RESPONSE_SECRET));
        let Error::Http(http) = error else {
            panic!("must preserve HTTP failure")
        };
        assert_eq!(http.document_unredacted().unwrap_err().kind(), expected);
        assert_redacted(&http);
    }
}
