//! Connected fixture-backed workflows and explicit offset traversal outcomes.

mod support;

use std::num::NonZeroUsize;

use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_fred::{
    Error, ObservationLimit, SearchLimit, concord,
    concord_pagination::{PagesError, Resume},
    pages,
};
use support::{api, kind, safe};

// Expectations below describe the independently authored synthetic provider corpus.
fn fixture_service(r: &Recorded) -> Reply {
    let offset = r.query_all("offset").first().cloned().unwrap_or_default();
    let json = r.query_all("file_type") == ["json"];
    match (r.path(), offset.as_str(), json) {
        ("/fred/series/search", "0", true) => {
            Reply::json(200, include_bytes!("../fixtures/search-0.json"))
        }
        ("/fred/series/search", "1", true) => {
            Reply::json(200, include_bytes!("../fixtures/search-1.json"))
        }
        ("/fred/series/search", "0", false) => {
            Reply::text(200, "text/xml", include_bytes!("../fixtures/search-0.xml"))
        }
        ("/fred/series/search", "1", false) => {
            Reply::text(200, "text/xml", include_bytes!("../fixtures/search-1.xml"))
        }
        ("/fred/series", _, true) => Reply::json(200, include_bytes!("../fixtures/metadata.json")),
        ("/fred/series", _, false) => {
            Reply::text(200, "text/xml", include_bytes!("../fixtures/metadata.xml"))
        }
        ("/fred/series/observations", _, _) if r.query_all("file_type") == ["csv"] => Reply::text(
            200,
            "application/zip",
            include_bytes!("../fixtures/observations.zip"),
        ),
        ("/fred/series/observations", "0", true) => {
            Reply::json(200, include_bytes!("../fixtures/observations-0.json"))
        }
        ("/fred/series/observations", "2", true) => {
            Reply::json(200, include_bytes!("../fixtures/observations-2.json"))
        }
        ("/fred/series/observations", "0", false) => Reply::text(
            200,
            "text/xml",
            include_bytes!("../fixtures/observations-0.xml"),
        ),
        ("/fred/series/observations", "2", false) => Reply::text(
            200,
            "text/xml",
            include_bytes!("../fixtures/observations-2.xml"),
        ),
        _ => Reply::new(404),
    }
}

#[tokio::test]
async fn search_choose_and_fetch_metadata_in_both_representations() {
    let server = TestServer::start(fixture_service).await;
    let client = api(&server);
    let mut search = pages(
        client
            .search("synthetic activity")
            .set_limit(SearchLimit::new(1).unwrap()),
        Resume::Start,
        NonZeroUsize::new(2).unwrap(),
    );
    let first = search.next_page().await.unwrap().unwrap();
    assert_eq!(first.items[0].id, "SYN_A");
    assert_eq!(first.items[0].title, "Synthetic output & activity");
    let selected = client.series(first.items[0].id.clone());
    let metadata = selected.metadata().send().await.unwrap();
    assert_eq!(metadata.seriess[0], first.items[0]);
    assert_eq!(metadata.seriess[0].units_short, "SU");
    let last = search.next_page().await.unwrap().unwrap();
    assert_eq!(last.items[0].id, "SYN_B");
    assert_eq!(last.items[0].notes, None);
    assert!(search.next_page().await.unwrap().is_none());
    let page_json = client
        .search_json("synthetic activity")
        .set_limit(SearchLimit::new(1).unwrap())
        .send()
        .await
        .unwrap();
    let metadata_json = client
        .series(&page_json.seriess[0].id)
        .metadata_json()
        .send()
        .await
        .unwrap();
    assert_eq!(metadata_json, metadata);
    assert_eq!(server.requests().len(), 5);
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn selected_series_read_xml_json_then_export_exact_csv() {
    let server = TestServer::start(fixture_service).await;
    let client = api(&server);
    let found = client
        .search("synthetic activity")
        .set_limit(SearchLimit::new(1).unwrap())
        .send()
        .await
        .unwrap();
    let selected = client.series(found.seriess[0].id.clone());
    let metadata = selected.metadata().send().await.unwrap();
    assert_eq!(metadata.seriess[0].frequency_short, "A");
    let limit = ObservationLimit::new(2).unwrap();
    let mut xml_pages = pages(
        selected.observations().set_limit(limit),
        Resume::Start,
        NonZeroUsize::new(3).unwrap(),
    );
    let mut xml = Vec::new();
    while let Some(page) = xml_pages.next_page().await.unwrap() {
        xml.extend(page.items);
    }
    let mut json_pages = pages(
        selected.observations_json().set_limit(limit),
        Resume::Start,
        NonZeroUsize::new(3).unwrap(),
    );
    let mut json = Vec::new();
    while let Some(page) = json_pages.next_page().await.unwrap() {
        json.extend(page.items);
    }
    assert_eq!(xml, json);
    assert_eq!(
        xml.iter().map(|o| o.value.as_str()).collect::<Vec<_>>(),
        ["2.5000", ".", "NA"]
    );
    let export = selected.export_csv().send().await.unwrap();
    assert_eq!(export.csv(), include_bytes!("../fixtures/observations.csv"));
    assert_eq!(
        export.readme(),
        Some(include_bytes!("../fixtures/README.txt").as_slice())
    );
    let destination =
        std::env::temp_dir().join(format!("concord-fred-export-{}.csv", std::process::id()));
    std::fs::write(&destination, export.into_csv()).unwrap();
    assert_eq!(
        std::fs::read(&destination).unwrap(),
        b"DATE,SYN_A\n2000-01-01,2.5000\n2001-01-01,.\n2002-01-01,NA\n2003-01-01,\n"
    );
    std::fs::remove_file(destination).unwrap();
    assert_eq!(server.requests().len(), 7);
    server.shutdown().await;
}

#[tokio::test]
async fn export_workflow_rejects_plain_csv_and_both_oversize_classes() {
    for scenario in 0..3 {
        let server = TestServer::start(move |r: &Recorded| {
            if r.path() == "/fred/series" {
                return Reply::text(200, "text/xml", include_bytes!("../fixtures/metadata.xml"));
            }
            match scenario {
                0 => Reply::text(
                    200,
                    "application/zip",
                    include_bytes!("../fixtures/observations.csv"),
                ),
                1 => Reply::text(
                    200,
                    "application/zip",
                    include_bytes!("../fixtures/oversized-extracted.zip"),
                ),
                _ => Reply::text(200, "application/zip", support::oversized_archive()),
            }
        })
        .await;
        let selected = api(&server).series("SYN_A");
        assert_eq!(
            selected.metadata().send().await.unwrap().seriess[0].id,
            "SYN_A"
        );
        let error = selected.export_csv().send().await.unwrap_err();
        safe(&error);
        assert_eq!(
            kind(&error),
            if scenario == 2 {
                concord::ErrorKind::Limit
            } else {
                concord::ErrorKind::Decode
            }
        );
        assert_eq!(server.requests().len(), 2);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn page_budget_is_explicit_and_resume_preserves_selected_query() {
    let server = TestServer::start(fixture_service).await;
    let template = api(&server)
        .search_json("synthetic activity")
        .set_limit(SearchLimit::new(1).unwrap());
    let mut bounded = pages(template.clone(), Resume::Start, NonZeroUsize::MIN);
    assert_eq!(
        bounded.next_page().await.unwrap().unwrap().items[0].id,
        "SYN_A"
    );
    let error = bounded.next_page().await.unwrap_err();
    let PagesError::Bounded { resume, .. } = error else {
        panic!("expected explicit cap")
    };
    assert_eq!(resume, Resume::At(1));
    assert_eq!(server.requests().len(), 1);
    let mut resumed = pages(template, resume, NonZeroUsize::MIN);
    assert_eq!(
        resumed.next_page().await.unwrap().unwrap().items[0].id,
        "SYN_B"
    );
    assert!(resumed.next_page().await.unwrap().is_none());
    let requests = server.requests();
    assert_eq!(requests[1].query_all("offset"), ["1"]);
    assert_eq!(requests[1].query_all("search_text"), ["synthetic activity"]);
    server.shutdown().await;
}

#[tokio::test]
async fn pagination_rejects_nonprogress_mismatch_and_overflow() {
    for scenario in 0..6 {
        let server = TestServer::start(move |_: &Recorded| {
            let (count, offset, limit) = match scenario {
                0 => (3, 0, 2), // Empty while more records remain.
                1 => (0, 1, 2), // Response offset differs from request.
                2 => (0, 0, 0), // Zero limit.
                3 => (0, 0, 1), // Items extend past count.
                4 => (2, 0, 1), // More items than limit.
                _ => (u64::MAX, u64::MAX, 2), // Adding an item overflows.
            };
            let items = if scenario < 3 { "[]" } else if scenario == 4 {
                "[{\"date\":\"2000-01-01\",\"value\":\".\",\"realtime_start\":\"2026-01-01\",\"realtime_end\":\"2026-01-01\"},{\"date\":\"2001-01-01\",\"value\":\".\",\"realtime_start\":\"2026-01-01\",\"realtime_end\":\"2026-01-01\"}]"
            } else { "[{\"date\":\"2000-01-01\",\"value\":\".\",\"realtime_start\":\"2026-01-01\",\"realtime_end\":\"2026-01-01\"}]" };
            Reply::json(200, format!("{{\"count\":{count},\"offset\":{offset},\"limit\":{limit},\"observations\":{items}}}"))
        }).await;
        let from = if scenario == 5 {
            Resume::At(u64::MAX)
        } else {
            Resume::Start
        };
        let mut walk = pages(
            api(&server).series("SYN_A").observations_json(),
            from,
            NonZeroUsize::MIN,
        );
        let failure = walk.next_page().await.unwrap_err();
        assert!(matches!(failure.provider_error(), Some(Error::Pagination)));
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn failed_second_page_keeps_prior_page_and_failed_offset() {
    let server = TestServer::start(|r: &Recorded| {
        if r.query_all("offset") == ["0"] {
            fixture_service(r)
        } else {
            Reply::text(429, "text/xml", include_bytes!("../fixtures/error.xml"))
        }
    })
    .await;
    let mut walk = pages(
        api(&server)
            .series("SYN_A")
            .observations()
            .set_limit(ObservationLimit::new(2).unwrap()),
        Resume::Start,
        NonZeroUsize::new(2).unwrap(),
    );
    let retained = walk.next_page().await.unwrap().unwrap();
    let error = walk.next_page().await.unwrap_err();
    let PagesError::Provider { error, failed, .. } = error else {
        panic!("provider error lost")
    };
    assert_eq!(retained.items.len(), 2);
    assert_eq!(failed, Resume::At(2));
    safe(&error);
    assert!(matches!(error, Error::Http(ref h) if h.meta.status().as_u16() == 429));
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}
