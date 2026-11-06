//! Independent application consumer. Predictions and fixture provenance: ../FIRST_USE.md.

use std::{collections::VecDeque, num::NonZeroUsize, sync::Mutex, time::Duration};

use concord::ErrorKind;
use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use concord_sdk_airtable::{
    Client, Error, ListOptions, ProviderError, Record, ResponseMeta,
    pagination::{Outcome, PagesError, Resume},
};
use futures_util::StreamExt;
use serde_json::json;

const TOKEN: &str = "SYNTHETIC_FIRST_USE_NOT_A_CREDENTIAL";
const BASE: &str = "appFirstUse";
const OFFSET: &str = "itrFirstUse/recNext+part%2F &?=é";
const LAST_OFFSET: &str = "itrFirstUse/recLast";
const FIRST: &[u8] = include_bytes!("../fixtures/first_use/page_first.json");
const MIDDLE: &[u8] = include_bytes!("../fixtures/first_use/page_middle.json");
const LAST: &[u8] = include_bytes!("../fixtures/first_use/page_last.json");
const EMPTY: &[u8] = include_bytes!("../fixtures/first_use/empty.json");
const UNAVAILABLE: &[u8] = include_bytes!("../fixtures/first_use/unavailable.json");

async fn script(replies: impl IntoIterator<Item = Reply>) -> TestServer {
    let replies = Mutex::new(replies.into_iter().collect::<VecDeque<_>>());
    TestServer::start(move |_| {
        replies
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected extra request to the local fake")
    })
    .await
}

fn client(server: &TestServer) -> Client {
    let http = concord::reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .redirect(concord::reqwest::redirect::Policy::none())
        .no_proxy()
        .build()
        .unwrap();
    Client::new(http, TOKEN)
        .with_base_url(server.url())
        .unwrap()
}

fn cap(n: usize) -> NonZeroUsize {
    NonZeroUsize::new(n).unwrap()
}

fn ids(records: &[Record]) -> Vec<&str> {
    records.iter().map(|record| record.id.as_str()).collect()
}

fn assert_wire(request: &Recorded, path: &str, expected_query: &[(&str, &str)]) {
    assert_eq!(request.method, "GET");
    assert_eq!(request.path(), path);
    assert_eq!(
        request.header("authorization"),
        Some(format!("Bearer {TOKEN}").as_str())
    );
    assert_eq!(request.header("accept"), Some("application/json"));
    assert_eq!(request.body_len, 0);
    assert!(request.body.is_empty());
    let mut actual = request.query_pairs();
    let mut expected: Vec<_> = expected_query
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect();
    actual.sort();
    expected.sort();
    assert_eq!(actual, expected);
}

fn assert_meta(meta: &ResponseMeta, status: u16, marker: &str) {
    assert_eq!(meta.status().as_u16(), status);
    assert_eq!(meta.headers()["x-first-use"], marker);
    let operation = meta.operation().expect("declared operation identity");
    assert!(operation.is_endpoint());
    assert_eq!(operation.method(), Some("GET"));
}

// Outcome A: offset traversal, partial progress and truthful completion.

#[tokio::test]
async fn lists_offset_pages_through_an_empty_page_and_exhausts_at_the_cap() {
    let server = script([
        Reply::json(200, FIRST).header("x-first-use", "first"),
        Reply::json(200, MIDDLE).header("x-first-use", "middle"),
        Reply::json(200, LAST).header("x-first-use", "last"),
    ])
    .await;
    let client = client(&server);
    let mut pages = client.list_pages(BASE, "Tasks", ListOptions::default(), Resume::Start, cap(3));
    assert!(server.requests().is_empty());
    assert_eq!(pages.progress().admitted, 0);

    let first = pages.next_page().await.unwrap().unwrap();
    assert_eq!(ids(&first.items), ["recFirstUse1", "recFirstUse2"]);
    assert_eq!(first.next.as_deref(), Some(OFFSET));
    assert_meta(&first.meta, 200, "first");
    assert_eq!(
        first.items[0].created_time.as_deref(),
        Some("2026-01-02T03:04:05.000Z")
    );
    assert_eq!(first.items[0].fields["Count"], json!(7));
    assert_eq!(first.items[0].fields["Ready"], json!(true));
    assert_eq!(first.items[0].fields["Tags"], json!(["red", "blue"]));
    assert!(!first.items[1].fields.contains_key("Ready"));
    assert_eq!(first.items[1].created_time, None);

    let middle = pages.next_page().await.unwrap().unwrap();
    assert!(middle.items.is_empty());
    assert_eq!(middle.next.as_deref(), Some(LAST_OFFSET));
    assert_meta(&middle.meta, 200, "middle");
    let last = pages.next_page().await.unwrap().unwrap();
    assert_eq!(ids(&last.items), ["recFirstUse3"]);
    assert!(last.next.is_none());
    assert_meta(&last.meta, 200, "last");
    assert!(pages.next_page().await.unwrap().is_none());
    assert!(pages.next_page().await.unwrap().is_none());
    let report = pages.finish();
    assert_eq!(report.outcome, Outcome::Exhausted);
    assert_eq!(report.progress.admitted, 3);
    assert!(report.resume.is_none());
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    for (request, query) in requests.iter().zip([
        vec![],
        vec![("offset", OFFSET)],
        vec![("offset", LAST_OFFSET)],
    ]) {
        assert_wire(request, "/v0/appFirstUse/Tasks", &query);
    }
    assert!(requests[1].target.contains("%2F"));
    assert!(requests[1].target.contains("%252F"));
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn empty_first_page_is_successful_exhaustion() {
    let server = script([Reply::json(200, EMPTY)]).await;
    let records = client(&server)
        .list_pages(BASE, "Tasks", ListOptions::default(), Resume::Start, cap(1))
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert!(records.is_empty());
    assert_wire(&server.single(), "/v0/appFirstUse/Tasks", &[]);
    server.shutdown().await;
}

#[tokio::test]
async fn page_cap_is_incomplete_and_can_resume_without_replaying_the_prefix() {
    let server = script([Reply::json(200, FIRST), Reply::json(200, LAST)]).await;
    let client = client(&server);
    let mut pages = client.list_pages(BASE, "Tasks", ListOptions::default(), Resume::Start, cap(1));
    let mut records = pages.next_page().await.unwrap().unwrap().items;
    let error = pages.next_page().await.unwrap_err();
    assert!(matches!(error, PagesError::Bounded { .. }));
    assert_eq!(error.resume(), Some(&Resume::At(OFFSET.into())));
    assert_eq!(error.progress().admitted, 1);
    assert!(matches!(
        pages.next_page().await,
        Err(PagesError::Ended {
            outcome: Outcome::Bounded,
            ..
        })
    ));
    assert_eq!(server.requests().len(), 1);
    let rest = client
        .list_pages(
            BASE,
            "Tasks",
            ListOptions::default(),
            error.resume().unwrap().clone(),
            cap(1),
        )
        .items()
        .collect_complete()
        .await
        .unwrap();
    records.extend(rest);
    assert_eq!(
        ids(&records),
        ["recFirstUse1", "recFirstUse2", "recFirstUse3"]
    );
    assert_eq!(pages.finish().outcome, Outcome::Bounded);
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_wire(&requests[0], "/v0/appFirstUse/Tasks", &[]);
    assert_wire(&requests[1], "/v0/appFirstUse/Tasks", &[("offset", OFFSET)]);
    server.shutdown().await;
}

#[tokio::test]
async fn failed_page_keeps_partial_records_provider_details_and_retry_position() {
    let server = script([
        Reply::json(200, FIRST),
        Reply::json(503, UNAVAILABLE)
            .header("retry-after", "7")
            .header("x-first-use", "unavailable"),
        Reply::json(200, LAST),
    ])
    .await;
    let client = client(&server);
    let incomplete = client
        .list_pages(
            BASE,
            "Tasks",
            ListOptions::default(),
            Resume::Start,
            cap(10),
        )
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(ids(&incomplete.collected), ["recFirstUse1", "recFirstUse2"]);
    assert_eq!(incomplete.error.outcome(), Outcome::Failed);
    let resume = incomplete.error.resume().unwrap().clone();
    assert_eq!(resume, Resume::At(OFFSET.into()));
    let PagesError::Provider {
        error,
        failed,
        progress,
        ..
    } = incomplete.error
    else {
        panic!("expected a typed provider failure");
    };
    assert_eq!(failed, resume);
    assert_eq!(progress.admitted, 2);
    assert_eq!(progress.failed, 1);
    assert_meta(error.meta().unwrap(), 503, "unavailable");
    assert!(
        matches!(error.provider(), Some(ProviderError::Object { kind, message: Some(message) })
        if kind == "RETRIABLE_ERROR" && message == "Synthetic first-use service unavailable")
    );
    let Error::Http {
        meta,
        body,
        body_error,
        ..
    } = error
    else {
        panic!("expected an HTTP error");
    };
    assert_eq!(meta.headers()["retry-after"], "7");
    assert_eq!(body.as_deref(), Some(UNAVAILABLE));
    assert!(body_error.is_none());
    assert_eq!(
        server.requests().len(),
        2,
        "SDK must not retry automatically"
    );

    // The application explicitly decides to resume; only the failed page is replayed.
    let rest = client
        .list_pages(BASE, "Tasks", ListOptions::default(), resume, cap(1))
        .items()
        .collect_complete()
        .await
        .unwrap();
    let mut records = incomplete.collected;
    records.extend(rest);
    assert_eq!(
        ids(&records),
        ["recFirstUse1", "recFirstUse2", "recFirstUse3"]
    );
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    assert_wire(&requests[0], "/v0/appFirstUse/Tasks", &[]);
    for request in &requests[1..] {
        assert_wire(request, "/v0/appFirstUse/Tasks", &[("offset", OFFSET)]);
    }
    server.shutdown().await;
}

#[tokio::test]
async fn interrupted_page_stream_yields_error_before_end_even_with_complete_json() {
    let server = script([
        Reply::json(200, FIRST),
        Reply::json(200, LAST)
            .header("x-first-use", "cut-off")
            .with_body(Body::FailAfter(vec![LAST.into()])),
    ])
    .await;
    let mut stream = client(&server)
        .list_pages(
            BASE,
            "Tasks",
            ListOptions::default(),
            Resume::Start,
            cap(10),
        )
        .items()
        .into_stream();
    assert_eq!(stream.next().await.unwrap().unwrap().id, "recFirstUse1");
    assert_eq!(stream.next().await.unwrap().unwrap().id, "recFirstUse2");
    let error = stream.next().await.unwrap().unwrap_err();
    let PagesError::Provider { error, failed, .. } = error else {
        panic!("an interrupted page must fail, not complete the listing");
    };
    assert_eq!(failed, Resume::At(OFFSET.into()));
    assert_meta(error.meta().unwrap(), 200, "cut-off");
    let Error::Concord { source, .. } = error else {
        panic!("expected a transport error");
    };
    assert_eq!(source.kind(), ErrorKind::Transport);
    assert!(source.operation().unwrap().is_endpoint());
    assert!(stream.next().await.is_none());
    assert!(stream.next().await.is_none());
    let mut items = stream.into_inner();
    assert!(matches!(
        items.next().await,
        Err(PagesError::Ended {
            outcome: Outcome::Failed,
            ..
        })
    ));
    assert_eq!(items.finish().outcome, Outcome::Failed);
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_wire(&requests[0], "/v0/appFirstUse/Tasks", &[]);
    assert_wire(&requests[1], "/v0/appFirstUse/Tasks", &[("offset", OFFSET)]);
    server.shutdown().await;
}

#[tokio::test]
async fn stopping_an_item_stream_early_is_not_exhaustion_and_does_not_prefetch() {
    let server = script([Reply::json(200, FIRST)]).await;
    let mut stream = client(&server)
        .list_pages(
            BASE,
            "Tasks",
            ListOptions::default(),
            Resume::Start,
            cap(10),
        )
        .items()
        .into_stream();
    assert_eq!(stream.next().await.unwrap().unwrap().id, "recFirstUse1");
    let report = stream.into_inner().finish();
    assert_eq!(report.outcome, Outcome::Stopped);
    assert!(report.resume.is_some());
    assert_wire(&server.single(), "/v0/appFirstUse/Tasks", &[]);
    server.shutdown().await;
}

#[tokio::test]
async fn malformed_success_and_empty_offset_do_not_masquerade_as_completion() {
    for (body, expected_kind) in [
        (
            include_bytes!("../fixtures/first_use/malformed.json").as_slice(),
            Some(ErrorKind::Decode),
        ),
        (
            include_bytes!("../fixtures/first_use/empty_offset.json").as_slice(),
            None,
        ),
    ] {
        let server = script([Reply::json(200, body).header("x-first-use", "invalid-page")]).await;
        let mut pages = client(&server).list_pages(
            BASE,
            "Tasks",
            ListOptions::default(),
            Resume::Start,
            cap(2),
        );
        let PagesError::Provider { error, failed, .. } = pages.next_page().await.unwrap_err()
        else {
            panic!("invalid response must fail");
        };
        assert_eq!(failed, Resume::Start);
        assert_meta(error.meta().unwrap(), 200, "invalid-page");
        match expected_kind {
            Some(kind) => {
                let Error::Concord { source, .. } = error else {
                    panic!("expected decode error")
                };
                assert_eq!(source.kind(), kind);
            }
            None => assert!(matches!(error, Error::EmptyOffset { .. })),
        }
        assert!(matches!(
            pages.next_page().await,
            Err(PagesError::Ended {
                outcome: Outcome::Failed,
                ..
            })
        ));
        assert_wire(&server.single(), "/v0/appFirstUse/Tasks", &[]);
        server.shutdown().await;
    }
}

// Outcome B: provider-evaluated filtering and selected dynamic fields.

#[tokio::test]
async fn filter_and_field_selection_survive_each_offset_page() {
    const FORMULA: &str = "AND({Status}='Open',{Name}='A+B & café / 50%?#')";
    const VIEW: &str = "Needs review / équipe";
    let server = script([
        Reply::json(
            200,
            include_bytes!("../fixtures/first_use/selected_first.json"),
        ),
        Reply::json(
            200,
            include_bytes!("../fixtures/first_use/selected_last.json"),
        ),
    ])
    .await;
    let options = ListOptions {
        filter_by_formula: Some(FORMULA.into()),
        fields: vec!["Name".into(), "Status".into()],
        view: Some(VIEW.into()),
        page_size: Some(1),
        max_records: Some(2),
    };
    let records = client(&server)
        .list_pages(BASE, "Tasks / équipe? #%", options, Resume::Start, cap(2))
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(ids(&records), ["recSelected1", "recSelected2"]);
    for record in records {
        assert_eq!(record.fields.len(), 2);
        assert_eq!(record.fields["Name"], json!("A+B & café / 50%?#"));
        assert_eq!(record.fields["Status"], json!("Open"));
    }
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    for (index, request) in requests.iter().enumerate() {
        let mut query = vec![
            ("filterByFormula", FORMULA),
            ("fields[]", "Name"),
            ("fields[]", "Status"),
            ("view", VIEW),
            ("pageSize", "1"),
            ("maxRecords", "2"),
        ];
        if index == 1 {
            query.push(("offset", OFFSET));
        }
        assert_wire(
            request,
            "/v0/appFirstUse/Tasks%20%2F%20%C3%A9quipe%3F%20%23%25",
            &query,
        );
        assert_eq!(request.query_all("fields[]"), ["Name", "Status"]);
        assert_eq!(request.target.matches("fields%5B%5D=").count(), 2);
    }
    server.shutdown().await;
}

#[tokio::test]
async fn single_page_field_selection_preserves_omitted_empty_fields() {
    let server = script([Reply::json(
        200,
        include_bytes!("../fixtures/first_use/selected_sparse.json"),
    )
    .header("x-first-use", "selected")])
    .await;
    let options = ListOptions {
        fields: vec!["Name".into(), "Notes".into()],
        ..Default::default()
    };
    let page = client(&server)
        .list_page(BASE, "Tasks", &options, None)
        .await
        .unwrap();
    assert_eq!(ids(&page.value.records), ["recSparse"]);
    assert_eq!(page.value.records[0].fields.len(), 1);
    assert_eq!(
        page.value.records[0].fields["Name"],
        json!("Task without notes")
    );
    assert!(!page.value.records[0].fields.contains_key("Notes"));
    assert!(page.value.offset.is_none());
    assert_meta(&page.meta, 200, "selected");
    assert_wire(
        &server.single(),
        "/v0/appFirstUse/Tasks",
        &[("fields[]", "Name"), ("fields[]", "Notes")],
    );
    server.shutdown().await;
}

#[tokio::test]
async fn filter_with_no_matches_returns_an_empty_complete_page() {
    let server = script([Reply::json(200, EMPTY).header("x-first-use", "no-matches")]).await;
    let options = ListOptions {
        filter_by_formula: Some("{Status}='No matching status'".into()),
        ..Default::default()
    };
    let page = client(&server)
        .list_page(BASE, "Tasks", &options, None)
        .await
        .unwrap();
    assert!(page.value.records.is_empty());
    assert!(page.value.offset.is_none());
    assert_meta(&page.meta, 200, "no-matches");
    assert_wire(
        &server.single(),
        "/v0/appFirstUse/Tasks",
        &[("filterByFormula", "{Status}='No matching status'")],
    );
    server.shutdown().await;
}

#[tokio::test]
async fn query_errors_keep_structured_string_or_non_json_provider_evidence() {
    let cases = [
        (
            422,
            "application/json",
            include_bytes!("../fixtures/first_use/invalid_query.json").as_slice(),
        ),
        (
            404,
            "application/json",
            include_bytes!("../fixtures/first_use/not_found.json").as_slice(),
        ),
        (
            502,
            "text/html",
            include_bytes!("../fixtures/first_use/gateway.html").as_slice(),
        ),
    ];
    for (status, content_type, bytes) in cases {
        let server =
            script([Reply::text(status, content_type, bytes).header("x-first-use", "query-error")])
                .await;
        let options = ListOptions {
            filter_by_formula: Some("{Missing field}='test'".into()),
            ..Default::default()
        };
        let error = client(&server)
            .list_page(BASE, "Tasks", &options, None)
            .await
            .unwrap_err();
        assert_meta(error.meta().unwrap(), status, "query-error");
        match status {
            422 => assert!(
                matches!(error.provider(), Some(ProviderError::Object { kind, message: Some(message) })
                if kind == "INVALID_REQUEST_UNKNOWN" && message == "Synthetic first-use query rejected")
            ),
            404 => assert!(
                matches!(error.provider(), Some(ProviderError::String(kind)) if kind == "NOT_FOUND")
            ),
            502 => assert!(error.provider().is_none()),
            _ => unreachable!(),
        }
        let Error::Http {
            body, body_error, ..
        } = error
        else {
            panic!("provider status should remain an HTTP error");
        };
        assert_eq!(body.as_deref(), Some(bytes));
        assert!(body_error.is_none());
        assert_wire(
            &server.single(),
            "/v0/appFirstUse/Tasks",
            &[("filterByFormula", "{Missing field}='test'")],
        );
        server.shutdown().await;
    }
}

#[tokio::test]
async fn invalid_page_sizes_fail_locally_without_sending_requests() {
    let server = script([]).await;
    let client = client(&server);
    for page_size in [0, 101, 255] {
        let options = ListOptions {
            page_size: Some(page_size),
            ..Default::default()
        };
        let error = client
            .list_page(BASE, "Tasks", &options, None)
            .await
            .unwrap_err();
        assert!(matches!(error, Error::InvalidInput(_)));
        assert!(error.meta().is_none());
    }
    assert!(server.requests().is_empty());
    server.shutdown().await;
}

#[tokio::test]
async fn long_filter_surfaces_get_rejection_without_a_post_fallback() {
    const TOO_LONG: &[u8] = include_bytes!("../fixtures/first_use/url_too_long.txt");
    let server =
        script([Reply::text(414, "text/plain", TOO_LONG).header("x-first-use", "long-url")]).await;
    let formula = format!("{{Name}}='{}'", "x".repeat(16_100));
    let options = ListOptions {
        filter_by_formula: Some(formula.clone()),
        ..Default::default()
    };
    let error = client(&server)
        .list_page(BASE, "Tasks", &options, None)
        .await
        .unwrap_err();
    assert_meta(error.meta().unwrap(), 414, "long-url");
    let Error::Http {
        provider,
        body,
        body_error,
        ..
    } = error
    else {
        panic!("expected the fake provider's URL rejection");
    };
    assert!(provider.is_none());
    assert_eq!(body.as_deref(), Some(TOO_LONG));
    assert!(body_error.is_none());
    let request = server.single();
    assert!(request.target.len() > 16_000);
    assert_wire(
        &request,
        "/v0/appFirstUse/Tasks",
        &[("filterByFormula", &formula)],
    );
    server.shutdown().await;
}

#[tokio::test]
async fn interrupted_error_document_keeps_http_status_and_body_failure() {
    let server = script([Reply::json(503, UNAVAILABLE)
        .header("retry-after", "7")
        .header("x-first-use", "interrupted-error")
        .with_body(Body::FailAfter(vec![UNAVAILABLE.into()]))])
    .await;
    let error = client(&server)
        .list_page(BASE, "Tasks", &ListOptions::default(), None)
        .await
        .unwrap_err();
    assert_meta(error.meta().unwrap(), 503, "interrupted-error");
    let Error::Http {
        meta,
        provider,
        body,
        body_error,
    } = error
    else {
        panic!("HTTP failure must survive the interrupted error document");
    };
    assert_eq!(meta.headers()["retry-after"], "7");
    assert!(provider.is_none());
    assert!(
        body.is_none(),
        "partial transfer is not a complete error document"
    );
    let body_error = body_error.expect("body failure is observable");
    assert_eq!(body_error.kind(), ErrorKind::Transport);
    assert!(body_error.operation().unwrap().is_endpoint());
    assert_wire(&server.single(), "/v0/appFirstUse/Tasks", &[]);
    server.shutdown().await;
}
