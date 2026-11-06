//! Independent application consumer; predictions and provenance are in FIRST_USE.md.
//! The SDK source, existing tests, and author's fixtures were not consulted.
use concord::{ErrorKind, ResponseMeta, bytes::Bytes, reqwest};
use concord_example_testserver::{Body, Reply, Script, TestServer};
use concord_sdk_world_bank::{
    Client, Error, IndicatorPages, Observation,
    concord_pagination::{Outcome, PagesError, Resume},
};
use serde_json::{Value, json};
use std::{
    num::{NonZeroU32, NonZeroUsize},
    time::Duration,
};

const YEAR: &[u8] = include_bytes!("../fixtures/first-use/year.json");
const FIRST: &[u8] = include_bytes!("../fixtures/first-use/range-1.json");
const LAST: &[u8] = include_bytes!("../fixtures/first-use/range-2.json");
const PROBLEM: &[u8] = include_bytes!("../fixtures/first-use/provider-error.json");
const TRUNCATED: &[u8] = include_bytes!("../fixtures/first-use/truncated.json");

async fn fake(replies: impl IntoIterator<Item = Reply>) -> (TestServer, Client, Script) {
    let script = Script::new(replies);
    let responder = script.clone();
    let server = TestServer::start(move |_| responder.next()).await;
    let http = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(3))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let client = Client::new(http)
        .with_base_url(format!("{}/v2", server.url()))
        .unwrap();
    (server, client, script)
}

fn response(bytes: &[u8]) -> Reply {
    Reply::json(200, bytes).header("x-consumer-fixture", "world-bank-first-use")
}

fn edit(bytes: &[u8], change: impl FnOnce(&mut Value)) -> Vec<u8> {
    let mut value: Value = serde_json::from_slice(bytes).unwrap();
    change(&mut value);
    serde_json::to_vec(&value).unwrap()
}

fn range(client: &Client, resume: Resume<u32>) -> IndicatorPages {
    client
        .range(
            "br",
            "SP.POP.TOTL",
            2004,
            2006,
            NonZeroU32::new(2).unwrap(),
            resume,
        )
        .unwrap()
}

fn assert_head(meta: &ResponseMeta, status: u16) {
    assert_eq!(meta.status().as_u16(), status);
    assert_eq!(meta.version(), reqwest::Version::HTTP_11);
    let operation = meta
        .operation()
        .expect("declared operation survives decoding");
    assert!(operation.is_endpoint());
    assert_eq!(operation.method(), Some("GET"));
    assert!(!operation.name().is_empty());
}

fn observations(items: &[Observation]) -> Vec<(&str, Option<f64>)> {
    items.iter().map(|o| (o.date.as_str(), o.value)).collect()
}

fn assert_wire(server: &TestServer, date: &str, size: u32, pages: &[u32]) {
    let requests = server.requests();
    assert_eq!(
        requests.len(),
        pages.len(),
        "unexpected extra request or retry"
    );
    for (request, page) in requests.iter().zip(pages) {
        assert_eq!(request.method, "GET");
        assert_eq!(request.host, server.addr().to_string());
        assert_eq!(request.path(), "/v2/country/br/indicator/SP.POP.TOTL");
        let mut expected = vec![
            ("date".to_owned(), date.to_owned()),
            ("format".to_owned(), "json".to_owned()),
            ("page".to_owned(), page.to_string()),
            ("per_page".to_owned(), size.to_string()),
        ];
        let mut actual = request.query_pairs();
        expected.sort();
        actual.sort();
        assert_eq!(actual, expected, "all query fields, including duplicates");
        assert_eq!(request.header_all("accept"), ["application/json"]);
        assert!(request.header_all("authorization").is_empty());
        assert!(request.header_all("cookie").is_empty());
        assert!(request.body.is_empty());
        assert_eq!(request.body_len, 0);
    }
}

async fn finish(server: TestServer, script: Script, date: &str, size: u32, pages: &[u32]) {
    assert_wire(&server, date, size, pages);
    assert_eq!(
        script.remaining(),
        0,
        "not all expected responses were requested"
    );
    server.shutdown().await;
}

#[tokio::test]
async fn year_returns_observation_and_both_metadata_layers() {
    let (server, client, script) = fake([response(YEAR)]).await;
    let page = client.year("br", "SP.POP.TOTL", 2006).await.unwrap();
    assert_eq!(observations(&page.observations), [("2006", Some(123456.0))]);
    let observation = &page.observations[0];
    assert_eq!(observation.country.id, "BR");
    assert_eq!(observation.country.value, "Brazil");
    assert_eq!(observation.country_iso3_code.as_deref(), Some("BRA"));
    assert_eq!(observation.indicator.id, "SP.POP.TOTL");
    assert_eq!(observation.indicator.value, "Population, total");
    assert_eq!(observation.unit.as_deref(), Some(""));
    assert_eq!(observation.obs_status.as_deref(), Some(""));
    assert_eq!(observation.decimal, Some(0));
    assert_eq!(
        (
            page.metadata.page,
            page.metadata.pages,
            page.metadata.per_page,
            page.metadata.total
        ),
        (1, 1, 50, 1)
    );
    assert_eq!(page.metadata.lastupdated.as_deref(), Some("2026-10-01"));
    assert_head(&page.response, 200);
    assert_eq!(
        page.response.headers()["x-consumer-fixture"],
        "world-bank-first-use"
    );
    finish(server, script, "2006", 50, &[1]).await;
}

#[tokio::test]
async fn year_distinguishes_null_value_from_numeric_zero() {
    for (wire, expected) in [(Value::Null, None), (json!(0), Some(0.0))] {
        let body = edit(YEAR, |v| v[1][0]["value"] = wire);
        let (server, client, script) = fake([response(&body)]).await;
        let page = client.year("br", "SP.POP.TOTL", 2006).await.unwrap();
        assert_eq!(observations(&page.observations), [("2006", expected)]);
        finish(server, script, "2006", 50, &[1]).await;
    }
}

#[tokio::test]
async fn year_null_data_is_empty_not_an_observation_with_null_value() {
    let body = edit(YEAR, |v| {
        v[0]["total"] = json!(0);
        v[1] = Value::Null;
    });
    let (server, client, script) = fake([response(&body)]).await;
    let page = client.year("br", "SP.POP.TOTL", 2006).await.unwrap();
    assert!(page.observations.is_empty());
    assert_eq!(page.metadata.total, 0);
    assert_head(&page.response, 200);
    finish(server, script, "2006", 50, &[1]).await;
}

#[tokio::test]
async fn year_returns_only_first_page_when_provider_announces_more() {
    let body = edit(YEAR, |v| {
        v[0]["pages"] = json!(2);
        v[0]["total"] = json!(51);
    });
    let (server, client, script) = fake([response(&body)]).await;
    let page = client.year("br", "SP.POP.TOTL", 2006).await.unwrap();
    assert_eq!(page.metadata.pages, 2);
    assert_eq!(page.observations.len(), 1);
    finish(server, script, "2006", 50, &[1]).await;
}

#[tokio::test]
async fn year_provider_document_is_typed_at_success_and_error_http_status() {
    for status in [200, 400] {
        let (server, client, script) =
            fake([Reply::json(status, PROBLEM).header("x-request-id", "synthetic-request")]).await;
        let error = client.year("br", "SP.POP.TOTL", 2006).await.unwrap_err();
        assert_head(error.meta().unwrap(), status);
        match error {
            Error::Provider { meta, document } => {
                assert_eq!(meta.headers()["x-request-id"], "synthetic-request");
                assert_eq!(document.message.len(), 1);
                assert_eq!(document.message[0].id, "120");
                assert_eq!(document.message[0].key, "Invalid value");
                assert_eq!(
                    document.message[0].value,
                    "Synthetic invalid indicator for the offline consumer test."
                );
            }
            other => panic!("expected provider messages, got {other:?}"),
        }
        finish(server, script, "2006", 50, &[1]).await;
    }
}

#[tokio::test]
async fn year_non_json_http_failure_keeps_status_and_retry_header() {
    let (server, client, script) =
        fake([
            Reply::text(503, "text/html", "<p>synthetic maintenance</p>")
                .header("retry-after", "60"),
        ])
        .await;
    let error = client.year("br", "SP.POP.TOTL", 2006).await.unwrap_err();
    match error {
        Error::Http { meta, detail } => {
            assert_head(&meta, 503);
            assert_eq!(meta.headers()["retry-after"], "60");
            assert!(!detail.is_empty());
        }
        other => panic!("expected HTTP failure, got {other:?}"),
    }
    finish(server, script, "2006", 50, &[1]).await;
}

#[tokio::test]
async fn year_rejects_missing_value_and_malformed_success_json() {
    let missing = edit(YEAR, |v| {
        v[1][0].as_object_mut().unwrap().remove("value");
    });
    for body in [missing, b"{not-json}".to_vec()] {
        let (server, client, script) = fake([response(&body)]).await;
        match client.year("br", "SP.POP.TOTL", 2006).await.unwrap_err() {
            Error::Concord { source, meta } => {
                assert_eq!(source.kind(), ErrorKind::Decode);
                assert_head(meta.as_deref().unwrap(), 200);
            }
            other => panic!("expected decode failure, got {other:?}"),
        }
        finish(server, script, "2006", 50, &[1]).await;
    }
}

#[tokio::test]
async fn year_rejects_json_labelled_as_html() {
    let (server, client, script) = fake([Reply::text(200, "text/html", YEAR)]).await;
    match client.year("br", "SP.POP.TOTL", 2006).await.unwrap_err() {
        Error::Concord { source, meta } => {
            assert_eq!(source.kind(), ErrorKind::Metadata);
            assert_head(meta.as_deref().unwrap(), 200);
        }
        other => panic!("expected media type failure, got {other:?}"),
    }
    finish(server, script, "2006", 50, &[1]).await;
}

#[tokio::test]
async fn year_interrupted_http_fails_even_after_complete_json_bytes() {
    let reply = response(YEAR).with_body(Body::FailAfter(vec![Bytes::from_static(YEAR)]));
    let (server, client, script) = fake([reply]).await;
    match client.year("br", "SP.POP.TOTL", 2006).await.unwrap_err() {
        Error::Concord { source, meta } => {
            assert_eq!(source.kind(), ErrorKind::Transport);
            assert_head(meta.as_deref().unwrap(), 200);
        }
        other => panic!("incomplete HTTP must fail, got {other:?}"),
    }
    finish(server, script, "2006", 50, &[1]).await;
}

#[tokio::test]
async fn year_transport_failure_before_headers_has_no_invented_metadata() {
    let (server, client, script) = fake([Reply::hang_up()]).await;
    match client.year("br", "SP.POP.TOTL", 2006).await.unwrap_err() {
        Error::Concord { source, meta } => {
            assert_eq!(source.kind(), ErrorKind::Transport);
            assert!(meta.is_none());
        }
        other => panic!("expected transport error, got {other:?}"),
    }
    finish(server, script, "2006", 50, &[1]).await;
}

#[tokio::test]
async fn year_preserves_override_path_and_encodes_each_id_as_one_segment() {
    let (server, client, script) = fake([response(YEAR)]).await;
    let client = client
        .with_base_url(format!("{}/offline/v2/", server.url()))
        .unwrap();
    client.year("br/other", "SP.POP?x=1", 2006).await.unwrap();
    let request = server.single();
    assert_eq!(request.method, "GET");
    assert_eq!(
        request.path(),
        "/offline/v2/country/br%2Fother/indicator/SP.POP%3Fx%3D1"
    );
    assert_eq!(request.query_all("date"), ["2006"]);
    assert!(request.query_all("x").is_empty());
    assert_eq!(script.remaining(), 0);
    server.shutdown().await;
}

#[tokio::test]
async fn range_pages_preserve_metadata_and_exhaust_only_after_last_page() {
    let (server, client, script) = fake([response(FIRST), response(LAST)]).await;
    let mut pages = range(&client, Resume::Start);
    assert!(server.requests().is_empty());
    let first = pages.next_page().await.unwrap().unwrap();
    assert_eq!(
        observations(&first.items),
        [("2006", Some(123456.0)), ("2005", None)]
    );
    assert_eq!(first.next, Some(2));
    assert_eq!(first.meta.page.page, 1);
    assert_eq!(first.meta.page.per_page, 2);
    assert_eq!(first.meta.page.total, 3);
    assert_head(&first.meta.response, 200);
    assert_eq!(pages.checkpoint(), Some(&Resume::At(2)));
    let last = pages.next_page().await.unwrap().unwrap();
    assert_eq!(observations(&last.items), [("2004", Some(0.0))]);
    assert_eq!(last.next, None);
    assert_eq!(last.meta.page.page, 2);
    assert_eq!(last.meta.page.per_page, 2);
    assert_head(&last.meta.response, 200);
    assert_eq!(
        last.meta.response.operation(),
        first.meta.response.operation()
    );
    assert!(pages.checkpoint().is_none());
    assert!(pages.next_page().await.unwrap().is_none());
    assert!(pages.next_page().await.unwrap().is_none());
    assert_eq!(pages.finish().outcome, Outcome::Exhausted);
    finish(server, script, "2004:2006", 2, &[1, 2]).await;
}

#[tokio::test]
async fn range_items_collect_all_observations_in_provider_order() {
    let (server, client, script) = fake([response(FIRST), response(LAST)]).await;
    let items = range(&client, Resume::Start)
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(
        observations(&items),
        [
            ("2006", Some(123456.0)),
            ("2005", None),
            ("2004", Some(0.0))
        ]
    );
    finish(server, script, "2004:2006", 2, &[1, 2]).await;
}

#[tokio::test]
async fn range_empty_intermediate_page_is_not_exhaustion() {
    let empty = edit(FIRST, |v| v[1] = Value::Null);
    let (server, client, script) = fake([response(&empty), response(LAST)]).await;
    let items = range(&client, Resume::Start)
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(observations(&items), [("2004", Some(0.0))]);
    finish(server, script, "2004:2006", 2, &[1, 2]).await;
}

#[tokio::test]
async fn range_failure_keeps_prefix_and_can_resume_the_same_query() {
    let (server, client, script) = fake([
        response(FIRST),
        Reply::text(503, "text/plain", "synthetic outage"),
        response(LAST),
    ])
    .await;
    let incomplete = range(&client, Resume::Start)
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(
        observations(&incomplete.collected),
        [("2006", Some(123456.0)), ("2005", None)]
    );
    assert_eq!(incomplete.error.resume(), Some(&Resume::At(2)));
    assert_eq!(incomplete.error.outcome(), Outcome::Failed);
    let resume = incomplete.error.resume().unwrap().clone();
    match incomplete.error {
        PagesError::Provider {
            error: Error::Http { meta, .. },
            failed,
            ..
        } => {
            assert_eq!(failed, Resume::At(2));
            assert_head(&meta, 503);
        }
        other => panic!("expected failed second page, got {other:?}"),
    }
    let mut combined = incomplete.collected;
    combined.extend(
        range(&client, resume)
            .items()
            .collect_complete()
            .await
            .unwrap(),
    );
    assert_eq!(
        observations(&combined),
        [
            ("2006", Some(123456.0)),
            ("2005", None),
            ("2004", Some(0.0))
        ]
    );
    finish(server, script, "2004:2006", 2, &[1, 2, 2]).await;
}

#[tokio::test]
async fn range_cap_keeps_successful_pages_and_resumes_at_first_unread_page() {
    let (server, client, script) = fake([response(FIRST), response(LAST)]).await;
    let incomplete = range(&client, Resume::Start)
        .max_pages(NonZeroUsize::new(1).unwrap())
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected.len(), 1);
    assert_eq!(incomplete.collected[0].meta.page.page, 1);
    assert_head(&incomplete.collected[0].meta.response, 200);
    assert_eq!(incomplete.error.resume(), Some(&Resume::At(2)));
    assert!(matches!(&incomplete.error, PagesError::Bounded { .. }));
    assert_eq!(server.requests().len(), 1);
    let resumed = range(&client, incomplete.error.resume().unwrap().clone())
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(resumed.len(), 1);
    assert_eq!(resumed[0].meta.page.page, 2);
    assert_eq!(observations(&resumed[0].items), [("2004", Some(0.0))]);
    finish(server, script, "2004:2006", 2, &[1, 2]).await;
}

#[tokio::test]
async fn range_cap_remains_an_error_on_subsequent_calls() {
    let (server, client, script) = fake([response(FIRST)]).await;
    let mut pages = range(&client, Resume::Start).max_pages(NonZeroUsize::new(1).unwrap());
    pages.next_page().await.unwrap().unwrap();
    let bounded = pages.next_page().await.unwrap_err();
    assert!(matches!(bounded, PagesError::Bounded { .. }));
    assert_eq!(bounded.resume(), Some(&Resume::At(2)));
    for _ in 0..2 {
        let error = pages.next_page().await.unwrap_err();
        assert!(matches!(
            error,
            PagesError::Ended {
                outcome: Outcome::Bounded,
                ..
            }
        ));
        assert_eq!(error.resume(), Some(&Resume::At(2)));
    }
    finish(server, script, "2004:2006", 2, &[1]).await;
}

#[tokio::test]
async fn range_cap_on_exactly_the_last_page_is_complete() {
    let (server, client, script) = fake([response(FIRST), response(LAST)]).await;
    let pages = range(&client, Resume::Start)
        .max_pages(NonZeroUsize::new(2).unwrap())
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(pages.len(), 2);
    assert_eq!(pages[1].next, None);
    finish(server, script, "2004:2006", 2, &[1, 2]).await;
}

#[tokio::test]
async fn range_provider_error_is_typed_and_cannot_turn_into_clean_eof() {
    let (server, client, script) = fake([response(FIRST), response(PROBLEM)]).await;
    let mut pages = range(&client, Resume::Start);
    pages.next_page().await.unwrap().unwrap();
    match pages.next_page().await.unwrap_err() {
        PagesError::Provider {
            error: Error::Provider { meta, document },
            failed,
            ..
        } => {
            assert_eq!(failed, Resume::At(2));
            assert_head(&meta, 200);
            assert_eq!(document.message[0].id, "120");
        }
        other => panic!("expected provider error on second page, got {other:?}"),
    }
    for _ in 0..2 {
        let error = pages.next_page().await.unwrap_err();
        assert!(matches!(
            error,
            PagesError::Ended {
                outcome: Outcome::Failed,
                ..
            }
        ));
        assert_eq!(error.resume(), Some(&Resume::At(2)));
    }
    finish(server, script, "2004:2006", 2, &[1, 2]).await;
}

#[tokio::test]
async fn range_interrupted_body_retains_only_previously_completed_pages() {
    for bytes in [TRUNCATED, LAST] {
        let broken =
            response(bytes).with_body(Body::FailAfter(vec![Bytes::copy_from_slice(bytes)]));
        let (server, client, script) = fake([response(FIRST), broken]).await;
        let incomplete = range(&client, Resume::Start)
            .items()
            .collect_complete()
            .await
            .unwrap_err();
        assert_eq!(
            observations(&incomplete.collected),
            [("2006", Some(123456.0)), ("2005", None)]
        );
        assert_eq!(incomplete.error.resume(), Some(&Resume::At(2)));
        match incomplete.error {
            PagesError::Provider {
                error: Error::Concord { source, meta },
                ..
            } => {
                assert_eq!(source.kind(), ErrorKind::Transport);
                assert_head(meta.as_deref().unwrap(), 200);
            }
            other => panic!("expected failed body, got {other:?}"),
        }
        finish(server, script, "2004:2006", 2, &[1, 2]).await;
    }
}

#[tokio::test]
async fn range_items_error_after_prior_items_is_persistent() {
    let (server, client, script) = fake([response(FIRST), response(PROBLEM)]).await;
    let mut items = range(&client, Resume::Start).items();
    assert_eq!(items.next().await.unwrap().unwrap().date, "2006");
    assert_eq!(server.requests().len(), 1);
    assert_eq!(items.next().await.unwrap().unwrap().date, "2005");
    assert_eq!(
        server.requests().len(),
        1,
        "no read-ahead while items remain"
    );
    assert!(matches!(
        items.next().await,
        Err(PagesError::Provider { .. })
    ));
    let ended = items.next().await.unwrap_err();
    assert!(matches!(
        ended,
        PagesError::Ended {
            outcome: Outcome::Failed,
            ..
        }
    ));
    assert_eq!(ended.resume(), Some(&Resume::At(2)));
    finish(server, script, "2004:2006", 2, &[1, 2]).await;
}

#[tokio::test]
async fn range_rejects_page_mismatch_and_invalid_paging_metadata() {
    for (field, value) in [("page", 2), ("pages", 0), ("per_page", 0)] {
        let invalid = edit(FIRST, |v| v[0][field] = json!(value));
        let (server, client, script) = fake([response(&invalid)]).await;
        let incomplete = range(&client, Resume::Start)
            .collect_complete()
            .await
            .unwrap_err();
        assert!(incomplete.collected.is_empty());
        match incomplete.error {
            PagesError::Provider {
                error: Error::InvalidPage { meta, reason },
                ..
            } => {
                assert_head(&meta, 200);
                assert!(!reason.is_empty());
            }
            other => panic!("invalid {field} should fail, got {other:?}"),
        }
        finish(server, script, "2004:2006", 2, &[1]).await;
    }
}

#[tokio::test]
async fn range_reversed_years_fail_before_any_request() {
    let (server, client, script) = fake([]).await;
    let result = client.range(
        "br",
        "SP.POP.TOTL",
        2006,
        2004,
        NonZeroU32::new(2).unwrap(),
        Resume::Start,
    );
    assert!(matches!(result, Err(Error::InvalidRange)));
    finish(server, script, "2004:2006", 2, &[]).await;
}

#[tokio::test]
async fn range_same_year_is_valid_and_uses_range_query() {
    let (server, client, script) = fake([response(YEAR)]).await;
    let pages = client
        .range(
            "br",
            "SP.POP.TOTL",
            2006,
            2006,
            NonZeroU32::new(50).unwrap(),
            Resume::Start,
        )
        .unwrap()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(pages.len(), 1);
    assert_eq!(observations(&pages[0].items), [("2006", Some(123456.0))]);
    finish(server, script, "2006:2006", 50, &[1]).await;
}

#[tokio::test]
async fn range_unpolled_and_abandoned_traversals_do_not_fetch_more() {
    let (server, client, script) = fake([response(FIRST)]).await;
    drop(range(&client, Resume::Start));
    assert!(server.requests().is_empty());
    let mut pages = range(&client, Resume::Start);
    pages.next_page().await.unwrap().unwrap();
    drop(pages);
    finish(server, script, "2004:2006", 2, &[1]).await;
}
