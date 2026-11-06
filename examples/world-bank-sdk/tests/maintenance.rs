//! Maintenance ticket "null indicator and multiple pages": a sparse indicator whose values are
//! mostly absent (JSON null) arrives as a three-page metadata response. Absent values stay
//! `None`, the numeric zero stays `Some(0.0)`, every page the metadata indicates is requested
//! exactly once, and a cap or failure before the last page is never a clean end.
use concord::reqwest;
use concord_example_testserver::{Reply, Script, TestServer};
use concord_sdk_world_bank::{
    Client, Error, IndicatorPages, Observation,
    concord_pagination::{Outcome, PagesError, Resume},
};
use std::{
    num::{NonZeroU32, NonZeroUsize},
    time::Duration,
};

const PAGE_1: &[u8] = include_bytes!("../fixtures/maintenance/sparse-1.json");
const PAGE_2: &[u8] = include_bytes!("../fixtures/maintenance/sparse-2.json");
const PAGE_3: &[u8] = include_bytes!("../fixtures/maintenance/sparse-3.json");
const PROVIDER_ERROR: &[u8] = include_bytes!("../fixtures/error.json");

/// Every observation of the three pages, in provider order: two per page, one on the last.
const ALL: [(&str, Option<f64>); 5] = [
    ("2005", None),
    ("2004", None),
    ("2003", Some(0.0)),
    ("2002", None),
    ("2001", None),
];

async fn fake(replies: impl IntoIterator<Item = Reply>) -> (TestServer, Client, Script) {
    let script = Script::new(replies);
    let responder = script.clone();
    let server = TestServer::start(move |_| responder.next()).await;
    let http = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();
    let client = Client::new(http)
        .with_base_url(format!("{}/v2", server.url()))
        .unwrap();
    (server, client, script)
}

fn page(bytes: &[u8]) -> Reply {
    Reply::json(200, bytes).header("x-maintenance-fixture", "sparse")
}

fn sparse(client: &Client, resume: Resume<u32>) -> IndicatorPages {
    client
        .range(
            "br",
            "XX.SYNTH.SPARSE",
            2001,
            2005,
            NonZeroU32::new(2).unwrap(),
            resume,
        )
        .unwrap()
}

fn values(items: &[Observation]) -> Vec<(&str, Option<f64>)> {
    items.iter().map(|o| (o.date.as_str(), o.value)).collect()
}

/// The same query is sent for each listed page, in order, with nothing else on the wire.
async fn finish(server: TestServer, script: Script, pages: &[u32]) {
    let requests = server.requests();
    let targets: Vec<_> = requests.iter().map(|r| r.target.clone()).collect();
    let expected: Vec<_> = pages
        .iter()
        .map(|p| {
            format!(
                "/v2/country/br/indicator/XX.SYNTH.SPARSE?date=2001%3A2005&page={p}&per_page=2&format=json"
            )
        })
        .collect();
    assert_eq!(
        targets, expected,
        "one request per indicated page, no retry"
    );
    for request in &requests {
        assert_eq!(request.method, "GET");
        assert!(request.header_all("authorization").is_empty());
        assert_eq!(request.body_len, 0);
    }
    assert_eq!(script.remaining(), 0, "every scripted page was requested");
    server.shutdown().await;
}

#[tokio::test]
async fn sparse_pages_follow_metadata_and_keep_null_distinct_from_zero() {
    let (server, client, script) = fake([page(PAGE_1), page(PAGE_2), page(PAGE_3)]).await;
    let mut pages = sparse(&client, Resume::Start);

    let first = pages.next_page().await.unwrap().unwrap();
    assert_eq!(values(&first.items), ALL[..2], "an all-null page is data");
    let meta = &first.meta.page;
    assert_eq!(
        (meta.page, meta.pages, meta.per_page, meta.total),
        (1, 3, 2, 5)
    );
    assert_eq!(first.next, Some(2));
    assert_eq!(pages.checkpoint(), Some(&Resume::At(2)));

    let second = pages.next_page().await.unwrap().unwrap();
    assert_eq!(values(&second.items), ALL[2..4]);
    assert_eq!(second.items[0].value, Some(0.0));
    assert_eq!(second.items[1].value, None);
    let meta = &second.meta.page;
    assert_eq!(
        (meta.page, meta.pages, meta.per_page, meta.total),
        (2, 3, 2, 5)
    );
    assert_eq!(second.next, Some(3));

    let last = pages.next_page().await.unwrap().unwrap();
    assert_eq!(values(&last.items), ALL[4..], "a trailing null is kept");
    let meta = &last.meta.page;
    assert_eq!(
        (meta.page, meta.pages, meta.per_page, meta.total),
        (3, 3, 2, 5)
    );
    assert_eq!(last.next, None);
    assert_eq!(last.meta.response.status(), 200);
    assert_eq!(
        last.meta.response.headers()["x-maintenance-fixture"],
        "sparse"
    );

    assert!(pages.next_page().await.unwrap().is_none());
    assert!(pages.next_page().await.unwrap().is_none());
    assert_eq!(pages.finish().outcome, Outcome::Exhausted);
    finish(server, script, &[1, 2, 3]).await;
}

#[tokio::test]
async fn sparse_items_are_complete_in_provider_order() {
    let (server, client, script) = fake([page(PAGE_1), page(PAGE_2), page(PAGE_3)]).await;
    let items = sparse(&client, Resume::Start)
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(values(&items), ALL);
    assert_eq!(items.iter().filter(|o| o.value.is_none()).count(), 4);
    assert_eq!(items.iter().filter(|o| o.value == Some(0.0)).count(), 1);
    finish(server, script, &[1, 2, 3]).await;
}

#[tokio::test]
async fn sparse_cap_before_last_page_is_bounded_and_resume_keeps_trailing_null() {
    let (server, client, script) = fake([page(PAGE_1), page(PAGE_2), page(PAGE_3)]).await;
    let incomplete = sparse(&client, Resume::Start)
        .max_pages(NonZeroUsize::new(2).unwrap())
        .collect_complete()
        .await
        .unwrap_err();
    let collected: Vec<_> = incomplete
        .collected
        .iter()
        .flat_map(|p| values(&p.items))
        .collect();
    assert_eq!(collected, ALL[..4]);
    assert!(matches!(
        incomplete.error,
        PagesError::Bounded {
            resume: Resume::At(3),
            ..
        }
    ));
    assert_eq!(server.requests().len(), 2, "the cap stops before page 3");

    let rest = sparse(&client, Resume::At(3))
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(values(&rest), ALL[4..]);
    finish(server, script, &[1, 2, 3]).await;
}

#[tokio::test]
async fn sparse_provider_error_on_last_page_keeps_prefix_and_is_typed() {
    let (server, client, script) =
        fake([page(PAGE_1), page(PAGE_2), Reply::json(200, PROVIDER_ERROR)]).await;
    let incomplete = sparse(&client, Resume::Start)
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(values(&incomplete.collected), ALL[..4]);
    assert_eq!(incomplete.error.outcome(), Outcome::Failed);
    match incomplete.error {
        PagesError::Provider {
            error: Error::Provider { meta, document },
            failed,
            ..
        } => {
            assert_eq!(failed, Resume::At(3));
            assert_eq!(meta.status(), 200);
            assert_eq!(document.message[0].id, "120");
        }
        other => panic!("expected a typed provider error on page 3, got {other:?}"),
    }
    finish(server, script, &[1, 2, 3]).await;
}
