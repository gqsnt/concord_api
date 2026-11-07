//! Connected offline paper-search and ID-list metadata workflows.
mod common;

use common::*;
use concord_sdk_arxiv::concord_pagination::{PagesError, Resume};
use concord_sdk_arxiv::{Arxiv, ArxivConfig, IdList, PageSize, Policy, SearchTerm, http_client};
use concord_testserver::{Recorded, Reply, TestServer};
use std::{num::NonZeroUsize, time::Duration};

#[tokio::test]
async fn search_by_term_page_and_inspect_with_extensions_and_missing_optional_fields() {
    let server = TestServer::start(|request: &Recorded| -> Reply {
        assert_eq!(request.query_all("search_query"), ["all:\"electron\""]);
        match request.query_all("start")[0].as_str() {
            "0" => Reply::text(200, "application/atom+xml", FIRST),
            "1" => Reply::text(200, "application/atom+xml", SECOND),
            _ => panic!("unexpected offset"),
        }
    })
    .await;
    let clock = VirtualClock::default();
    let api = Arxiv::new(
        http_client().unwrap(),
        ArxivConfig {
            policy: Policy::with_clock(clock.clone()),
        },
    )
    .with_base_url(format!("{}/api", server.url()))
    .unwrap();
    let mut pages = api
        .papers()
        .search(SearchTerm::new("electron").unwrap())
        .set_size(PageSize::new(1).unwrap())
        .pages(Resume::Start, NonZeroUsize::new(2).unwrap());
    let first = pages.next_page().await.unwrap().unwrap();
    let paper = &first.items[0];
    assert_eq!(paper.title, "Synthetic electron & photon study");
    assert_eq!(paper.id, "http://arxiv.org/abs/2601.00001v2");
    assert_eq!(
        paper.authors[0].affiliations,
        ["Fixture Institute", "Sample Lab"]
    );
    assert_eq!(paper.authors[1].name, "René Example");
    assert_eq!(paper.primary_category.as_ref().unwrap().term, "quant-ph");
    assert_eq!(paper.categories[1].term, "I.2.6");
    assert_eq!(paper.comment.as_deref(), Some("Invented: 3 pages"));
    assert_eq!(paper.journal_ref.as_deref(), Some("Fixture Journal 1"));
    assert_eq!(paper.doi.as_deref(), Some("10.0000/synthetic"));
    assert_eq!(paper.links[1].title.as_deref(), Some("pdf"));
    let second = pages.next_page().await.unwrap().unwrap();
    let paper = &second.items[0];
    assert_eq!(paper.title, "Synthetic legacy paper");
    assert!(paper.summary.contains("<with literal angle brackets>"));
    assert!(paper.doi.is_none() && paper.comment.is_none() && paper.journal_ref.is_none());
    assert!(paper.primary_category.is_none());
    assert!(paper.authors[0].affiliations.is_empty());
    assert!(pages.next_page().await.unwrap().is_none());
    assert_eq!(server.requests().len(), 2);
    assert_eq!(clock.waits(), [Duration::from_secs(3)]);
    server.shutdown().await;
}

#[tokio::test]
async fn fetch_known_id_list_and_read_each_papers_metadata() {
    let server = TestServer::start(|request: &Recorded| -> Reply {
        assert_eq!(
            request.query_all("id_list"),
            ["2601.00001v2,hep-ex/0307015v1"]
        );
        assert!(request.query_all("search_query").is_empty());
        Reply::text(200, "application/atom+xml; charset=utf-8", IDS)
    })
    .await;
    let feed = client(&server)
        .papers()
        .fetch(IdList::new(&["2601.00001v2", "hep-ex/0307015v1"]).unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(feed.total_results, 2);
    assert_eq!(feed.entries.len(), 2);
    assert_eq!(feed.entries[0].published, "2026-01-01T12:00:00Z");
    assert_eq!(feed.entries[0].updated, "2026-01-02T12:00:00Z");
    assert_eq!(feed.entries[1].authors[0].name, "Legacy Example");
    assert!(feed.entries[1].doi.is_none());
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
}

#[tokio::test]
async fn bounded_search_keeps_prefix_and_can_resume_at_next_offset() {
    let server = TestServer::start(|request: &Recorded| -> Reply {
        Reply::text(
            200,
            "application/atom+xml",
            if request.query_all("start") == ["0"] {
                FIRST
            } else {
                SECOND
            },
        )
    })
    .await;
    let template = client(&server)
        .papers()
        .search(SearchTerm::new("electron").unwrap())
        .set_size(PageSize::new(1).unwrap());
    let incomplete = template
        .clone()
        .pages(Resume::Start, NonZeroUsize::new(1).unwrap())
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected.len(), 1);
    let resume = match incomplete.error {
        PagesError::Bounded { resume, .. } => resume,
        e => panic!("{e:?}"),
    };
    assert_eq!(resume, Resume::At(1));
    let rest = template
        .pages(resume, NonZeroUsize::new(1).unwrap())
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(rest.len(), 1);
    assert_eq!(rest[0].title, "Synthetic legacy paper");
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}

#[tokio::test]
async fn failed_second_page_keeps_prefix_and_failed_position() {
    let server = TestServer::start(|request: &Recorded| -> Reply {
        if request.query_all("start") == ["0"] {
            Reply::text(200, "application/atom+xml", FIRST)
        } else {
            Reply::text(400, "application/atom+xml", ERROR)
        }
    })
    .await;
    let incomplete = client(&server)
        .papers()
        .search(SearchTerm::new("electron").unwrap())
        .set_size(PageSize::new(1).unwrap())
        .pages(Resume::Start, NonZeroUsize::new(3).unwrap())
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected.len(), 1);
    assert_eq!(incomplete.error.resume(), Some(&Resume::At(1)));
    assert!(matches!(
        incomplete.error.provider_error(),
        Some(concord_sdk_arxiv::Error::Http(_))
    ));
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}

#[tokio::test]
async fn empty_search_exhausts_without_an_extra_request() {
    let server = TestServer::start(|_: &Recorded| -> Reply {
        Reply::text(200, "application/atom+xml", EMPTY)
    })
    .await;
    let results = client(&server)
        .papers()
        .search(SearchTerm::new("unmatched").unwrap())
        .pages(Resume::Start, NonZeroUsize::new(2).unwrap())
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert!(results.is_empty());
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
}
