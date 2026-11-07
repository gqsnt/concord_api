//! Connected fixture workflows. Wire facts: official webinar pp. 18, 32, 35, 40–44;
//! authored record identities and cursor transitions: fixtures/README.md.
mod support;
use concord_sdk_europe_pmc::{
    Cursor, Error, PageSize, SearchFormat,
    concord_pagination::{Outcome, PagesError, Resume},
};
use concord_testserver::{Recorded, Reply, TestServer};
use std::num::NonZeroUsize;
use support::*;

fn cap(n: usize) -> NonZeroUsize {
    NonZeroUsize::new(n).unwrap()
}

#[tokio::test]
async fn json_search_select_citing_publication_and_fetch_its_core_citation() {
    let server = TestServer::start(|r: &Recorded| -> Reply {
        assert_eq!(r.method, "GET");
        assert_eq!(r.header("accept"), Some("application/json"));
        assert!(r.header("authorization").is_none());
        assert_eq!(r.query_all("format"), ["json"]);
        match r.path() {
            "/rest/search" if r.query_all("resultType") == ["lite"] => {
                assert_eq!(r.query_all("query"), ["malaria"]);
                assert_eq!(r.query_all("pageSize"), ["2"]);
                match r.query_all("cursorMark")[0].as_str() {
                    "*" => Reply::json(200, FIRST_JSON),
                    "p+1/==?&" => Reply::json(200, LAST_JSON),
                    _ => panic!("unexpected cursor"),
                }
            }
            "/rest/MED/20485428/citations" => {
                assert_eq!(r.query_all("page"), ["1"]);
                assert_eq!(r.query_all("pageSize"), ["1"]);
                assert!(r.query_all("cursorMark").is_empty());
                Reply::json(200, include_str!("../fixtures/citations.json"))
            }
            "/rest/search" => {
                assert_eq!(r.query_all("query"), ["EXT_ID:30000001 AND SRC:MED"]);
                assert_eq!(r.query_all("resultType"), ["core"]);
                assert_eq!(r.query_all("pageSize"), ["1"]);
                assert!(r.query_all("cursorMark").is_empty());
                Reply::json(200, include_str!("../fixtures/citation-core.json"))
            }
            _ => panic!("unexpected route"),
        }
    })
    .await;
    let api = api(&server);
    let records = search(&api)
        .pages(SearchFormat::Json, Resume::Start, cap(2))
        .unwrap()
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(
        records.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
        ["20485428", "PMC7561308", "20485429"]
    );
    let selected = records[0].key();
    let citations = api
        .publication(selected.source, selected.id)
        .citations()
        .set_page_size(PageSize::new(1).unwrap())
        .send()
        .await
        .unwrap();
    let core = api
        .fetch_citation(&citations.citation_list.citation[0].key())
        .await
        .unwrap();
    assert_eq!(core.id.as_str(), "30000001");
    assert_eq!(
        core.abstract_text.as_deref(),
        Some("Authored fixture abstract; no scientific claim.")
    );
    assert_eq!(server.requests().len(), 4);
    server.shutdown().await;
}

#[tokio::test]
async fn same_bounded_search_in_xml_follows_exact_returned_cursor() {
    let server = TestServer::start(|r: &Recorded| -> Reply {
        assert_eq!(r.path(), "/rest/search");
        assert_eq!(r.header("accept"), Some("application/xml"));
        assert_eq!(r.query_all("query"), ["malaria"]);
        assert_eq!(r.query_all("resultType"), ["lite"]);
        assert_eq!(r.query_all("format"), ["xml"]);
        assert_eq!(r.query_all("pageSize"), ["2"]);
        let body = match r.query_all("cursorMark")[0].as_str() {
            "*" => FIRST_XML,
            "p+1/==?&" => LAST_XML,
            _ => panic!("cursor was altered"),
        };
        Reply::text(200, "application/xml", body)
    })
    .await;
    let api = api(&server);
    let mut pages = search(&api)
        .pages(SearchFormat::Xml, Resume::Start, cap(2))
        .unwrap();
    let first = pages.next_page().await.unwrap().unwrap();
    assert_eq!(first.meta, 3);
    assert_eq!(first.items[0].id.as_str(), "20485428");
    assert_eq!(first.next.unwrap().as_str(), "p+1/==?&");
    let second = pages.next_page().await.unwrap().unwrap();
    assert_eq!(second.items[0].id.as_str(), "20485429");
    assert!(pages.next_page().await.unwrap().is_none());
    assert_eq!(pages.finish().outcome, Outcome::Exhausted);
    assert!(
        server.requests()[1]
            .target
            .contains("cursorMark=p%2B1%2F%3D%3D%3F%26")
    );
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}

#[tokio::test]
async fn xml_default_is_refused_on_json_workflow_then_xml_can_continue() {
    // A stand-in deliberately ignores format=json and sends the documented XML default.
    let server = TestServer::start(|r: &Recorded| -> Reply {
        let body = if r.query_all("cursorMark") == ["*"] {
            FIRST_XML
        } else {
            LAST_XML
        };
        Reply::text(200, "application/xml", body)
    })
    .await;
    let api = api(&server);
    let mut json = search(&api)
        .pages(SearchFormat::Json, Resume::Start, cap(2))
        .unwrap();
    let failed = json.next_page().await.unwrap_err();
    let error = failed.provider_error().unwrap();
    assert_eq!(
        error.call_error().unwrap().kind(),
        concord_sdk_europe_pmc::concord::ErrorKind::Metadata
    );
    assert!(error.call_error().unwrap().codec_error().is_some());
    assert!(matches!(
        json.next_page().await,
        Err(PagesError::Ended { .. })
    ));
    let xml = search(&api)
        .pages(SearchFormat::Xml, Resume::Start, cap(2))
        .unwrap()
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(xml.len(), 3);
    assert_eq!(server.requests().len(), 3);
    server.shutdown().await;
}

#[tokio::test]
async fn cursor_repetition_stops_without_emitting_the_bad_page_or_sending_again() {
    let server = TestServer::start(|r: &Recorded| -> Reply {
        Reply::text(
            200,
            "text/xml",
            if r.query_all("cursorMark") == ["*"] {
                FIRST_XML
            } else {
                include_str!("../fixtures/repeated.xml")
            },
        )
    })
    .await;
    let api = api(&server);
    let mut pages = search(&api)
        .pages(SearchFormat::Xml, Resume::Start, cap(10))
        .unwrap();
    assert_eq!(pages.next_page().await.unwrap().unwrap().items.len(), 2);
    let error = pages.next_page().await.unwrap_err();
    assert!(matches!(
        error.provider_error(),
        Some(Error::RepeatedCursor)
    ));
    assert_eq!(
        error.resume(),
        Some(&Resume::At(Cursor::new("p+1/==?&").unwrap()))
    );
    assert!(matches!(
        pages.next_page().await,
        Err(PagesError::Ended { .. })
    ));
    assert_eq!(pages.finish().outcome, Outcome::Failed);
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}

#[tokio::test]
async fn a_cap_preserves_prefix_and_cursor_and_resumes_with_the_same_plan() {
    let server = TestServer::start(|r: &Recorded| -> Reply {
        Reply::json(
            200,
            if r.query_all("cursorMark") == ["*"] {
                FIRST_JSON
            } else {
                LAST_JSON
            },
        )
    })
    .await;
    let api = api(&server);
    let scope = search(&api);
    let incomplete = scope
        .pages(SearchFormat::Json, Resume::Start, cap(1))
        .unwrap()
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected.len(), 2);
    assert!(matches!(incomplete.error, PagesError::Bounded { .. }));
    let resume = incomplete.error.resume().unwrap().clone();
    assert_eq!(server.requests().len(), 1);
    let rest = scope
        .pages(SearchFormat::Json, resume, cap(1))
        .unwrap()
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(rest[0].id.as_str(), "20485429");
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}

#[tokio::test]
async fn empty_page_with_fresh_cursor_continues_and_a_multi_page_cycle_stops() {
    let server = TestServer::start(|r: &Recorded| -> Reply {
        let body = match r.query_all("cursorMark")[0].as_str() {
            "*" => FIRST_XML,
            "p+1/==?&" => include_str!("../fixtures/empty-continuing.xml"),
            "empty+next==" => include_str!("../fixtures/repeated.xml"),
            _ => panic!("unexpected fourth request"),
        };
        Reply::text(200, "application/xml", body)
    })
    .await;
    let api = api(&server);
    let incomplete = search(&api)
        .pages(SearchFormat::Xml, Resume::Start, cap(10))
        .unwrap()
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected.len(), 2);
    assert!(matches!(
        incomplete.error.provider_error(),
        Some(Error::RepeatedCursor)
    ));
    assert_eq!(server.requests().len(), 3);
    server.shutdown().await;
}

#[tokio::test]
async fn a_raised_cap_still_stops_at_the_twenty_page_ceiling_without_sending() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let served = AtomicUsize::new(0);
    let server = TestServer::start(move |_: &Recorded| -> Reply {
        let n = served.fetch_add(1, Ordering::SeqCst);
        Reply::json(200, FIRST_JSON.replace("p+1/==?&", &format!("fresh-{n}")))
    })
    .await;
    let api = api(&server);
    let incomplete = search(&api)
        .pages(SearchFormat::Json, Resume::Start, cap(20))
        .unwrap()
        .max_pages(cap(21))
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert!(matches!(
        incomplete.error.provider_error(),
        Some(Error::InvalidInput(_))
    ));
    assert_eq!(incomplete.collected.len(), 40);
    assert_eq!(server.requests().len(), 20);
    server.shutdown().await;
}
