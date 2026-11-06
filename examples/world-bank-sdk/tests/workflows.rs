//! End-to-end public client workflows against fixtures.
use concord::reqwest;
use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_world_bank::{
    Client,
    concord_pagination::{PagesError, Resume},
};
use std::num::{NonZeroU32, NonZeroUsize};
fn client(server: &TestServer) -> Client {
    Client::new(reqwest::Client::builder().no_proxy().build().unwrap())
        .with_base_url(format!("{}/v2", server.url()))
        .unwrap()
}
fn json(status: u16, body: &'static str) -> Reply {
    Reply::new(status)
        .header("content-type", "application/json")
        .body(body)
}
#[tokio::test]
async fn one_year() {
    let server =
        TestServer::start(|_: &Recorded| json(200, include_str!("../fixtures/year.json"))).await;
    let result = client(&server)
        .year("br", "SP.POP.TOTL", 2006)
        .await
        .unwrap();
    assert_eq!(result.metadata.total, 1);
    assert_eq!(result.observations[0].value, Some(0.0));
    assert_eq!(
        server.single().target,
        "/v2/country/br/indicator/SP.POP.TOTL?date=2006&page=1&per_page=50&format=json"
    );
    server.shutdown().await;
}
#[tokio::test]
async fn time_range_reaches_last_page_and_preserves_null() {
    let server = TestServer::start(|r: &Recorded| {
        if r.target.contains("page=2") {
            json(200, include_str!("../fixtures/page-2.json"))
        } else {
            json(200, include_str!("../fixtures/page-1.json"))
        }
    })
    .await;
    let api = client(&server);
    let pages = api
        .range(
            "br",
            "SP.POP.TOTL",
            2006,
            2007,
            NonZeroU32::new(1).unwrap(),
            Resume::Start,
        )
        .unwrap()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(pages.len(), 2);
    assert_eq!(pages[0].items[0].value, None);
    assert_eq!(pages[1].items[0].value, Some(0.0));
    assert_eq!(pages[1].meta.page.page, 2);
    assert_eq!(pages[0].meta.response.status(), 200);
    let targets: Vec<_> = server.requests().into_iter().map(|r| r.target).collect();
    assert_eq!(
        targets,
        [
            "/v2/country/br/indicator/SP.POP.TOTL?date=2006%3A2007&page=1&per_page=1&format=json",
            "/v2/country/br/indicator/SP.POP.TOTL?date=2006%3A2007&page=2&per_page=1&format=json"
        ]
    );
    server.shutdown().await;
}
#[tokio::test]
async fn cap_and_failure_return_prefix_and_resume() {
    let server = TestServer::start(|r: &Recorded| {
        if r.target.contains("page=2") {
            json(503, include_str!("../fixtures/error.json"))
        } else {
            json(200, include_str!("../fixtures/page-1.json"))
        }
    })
    .await;
    let api = client(&server);
    let range = || {
        api.range(
            "br",
            "SP.POP.TOTL",
            2006,
            2007,
            NonZeroU32::new(1).unwrap(),
            Resume::Start,
        )
        .unwrap()
    };
    let bounded = range()
        .max_pages(NonZeroUsize::new(1).unwrap())
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(bounded.collected.len(), 1);
    assert!(matches!(
        bounded.error,
        PagesError::Bounded {
            resume: Resume::At(2),
            ..
        }
    ));
    let failed = range().items().collect_complete().await.unwrap_err();
    assert_eq!(failed.collected.len(), 1);
    assert!(matches!(
        failed.error,
        PagesError::Provider {
            failed: Resume::At(2),
            ..
        }
    ));
    server.shutdown().await;
}

#[tokio::test]
async fn exact_cap_and_resume_complete() {
    let server = TestServer::start(|r: &Recorded| {
        if r.target.contains("page=2") {
            json(200, include_str!("../fixtures/page-2.json"))
        } else {
            json(200, include_str!("../fixtures/page-1.json"))
        }
    })
    .await;
    let api = client(&server);
    let pages = api
        .range(
            "br",
            "SP.POP.TOTL",
            2006,
            2007,
            1.try_into().unwrap(),
            Resume::Start,
        )
        .unwrap()
        .max_pages(NonZeroUsize::new(2).unwrap())
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(pages.len(), 2);
    let resumed = api
        .range(
            "br",
            "SP.POP.TOTL",
            2006,
            2007,
            1.try_into().unwrap(),
            Resume::At(2),
        )
        .unwrap()
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(resumed.len(), 1);
    assert_eq!(resumed[0].value, Some(0.0));
    server.shutdown().await;
}
