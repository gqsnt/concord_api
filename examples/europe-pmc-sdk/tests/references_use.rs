//! Consumer use of the reference-list traversal, written against the README's public API by
//! the maintainer (not an independent consumer). A bibliography application selects a
//! publication from a JSON search, reads its whole reference list, follows matched references,
//! checkpoints an XML read under a page cap and handles a server that ignores `format=json`.
//! Bodies are built here from the documented envelope shapes, not taken from SDK fixtures.

use concord_sdk_europe_pmc::concord::ErrorKind;
use concord_sdk_europe_pmc::concord_pagination::{PagesError, Resume};
use concord_sdk_europe_pmc::{
    Error, EuropePmc, PageNumber, PageSize, Query, RecordKey, Reference, SearchFormat, http_client,
};
use concord_testserver::{Recorded, Reply, TestServer};
use std::num::NonZeroUsize;
use std::time::{Duration, SystemTime};

const BASE: &str = "/europepmc/webservices/rest";
const JSON_TYPE: &str = "application/json;charset=UTF-8";
const XML_TYPE: &str = "application/xml;charset=UTF-8";
const TOTAL: usize = 7;
const SIZE: usize = 3;

/// Reference `n` (1-based) of the selected publication; every third-but-one is unmatched.
fn identity(n: usize) -> Option<(&'static str, String)> {
    match n % 3 {
        2 => None,
        0 => Some(("PMC", format!("PMC80000{n}"))),
        _ => Some(("MED", format!("4000000{n}"))),
    }
}

fn page_range(r: &Recorded) -> std::ops::Range<usize> {
    let page: usize = r.query_all("page")[0].parse().unwrap();
    let size: usize = r.query_all("pageSize")[0].parse().unwrap();
    let start = ((page - 1) * size).min(TOTAL);
    start..(start + size).min(TOTAL)
}

fn references_json(r: &Recorded) -> String {
    let items: Vec<String> = page_range(r)
        .map(|i| {
            let n = i + 1;
            let id = identity(n)
                .map(|(src, id)| format!("\"id\":\"{id}\",\"source\":\"{src}\","))
                .unwrap_or_default();
            format!(
                "{{{id}\"title\":\"Reference {n}\",\"pubYear\":{}}}",
                2000 + n
            )
        })
        .collect();
    format!(
        "{{\"version\":\"6.5\",\"hitCount\":{TOTAL},\"referenceList\":{{\"reference\":[{}]}}}}",
        items.join(",")
    )
}

fn references_xml(r: &Recorded) -> String {
    let items: String = page_range(r)
        .map(|i| {
            let n = i + 1;
            let id = identity(n)
                .map(|(src, id)| format!("<id>{id}</id><source>{src}</source>"))
                .unwrap_or_default();
            format!(
                "<reference>{id}<title>Reference {n}</title><pubYear>{}</pubYear></reference>",
                2000 + n
            )
        })
        .collect();
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><responseWrapper><version>6.5</version>\
         <hitCount>{TOTAL}</hitCount><referenceList>{items}</referenceList></responseWrapper>"
    )
}

const SEARCH: &str = "{\"hitCount\":1,\"resultList\":{\"result\":[{\"id\":\"20485428\",\
    \"source\":\"MED\",\"title\":\"Chemical genetics of Plasmodium falciparum.\",\"pubYear\":\"2010\"}]}}";

fn core(id: &str) -> String {
    format!(
        "{{\"hitCount\":1,\"resultList\":{{\"result\":[{{\"id\":\"{id}\",\"source\":\"MED\",\
         \"title\":\"Reference record\",\"abstractText\":\"Synthetic abstract.\"}}]}}}}"
    )
}

/// The documented service: search, references in either format, and core lookups.
async fn service() -> TestServer {
    TestServer::start(|r: &Recorded| -> Reply {
        let format = r.query_all("format");
        let path = r.path().strip_prefix(BASE).expect("base path");
        match (path, format[0].as_str()) {
            ("/search", "json") if r.query_all("resultType") == ["core"] => {
                let query = &r.query_all("query")[0];
                let id = query
                    .strip_prefix("EXT_ID:")
                    .and_then(|q| q.strip_suffix(" AND SRC:MED"))
                    .expect("exact identity query");
                Reply::text(200, JSON_TYPE, core(id))
            }
            ("/search", "json") => Reply::text(200, JSON_TYPE, SEARCH),
            ("/MED/20485428/references", "json") => Reply::text(200, JSON_TYPE, references_json(r)),
            ("/MED/20485428/references", "xml") => Reply::text(200, XML_TYPE, references_xml(r)),
            _ => Reply::text(404, "text/plain", "not found"),
        }
    })
    .await
}

fn client(server: &TestServer) -> EuropePmc {
    EuropePmc::new(http_client().unwrap())
        .with_base_url(format!("{}{BASE}", server.url()))
        .unwrap()
}

fn cap(n: usize) -> NonZeroUsize {
    NonZeroUsize::new(n).unwrap()
}

/// One bibliography line per reference, as an application would render it.
fn line(r: &Reference) -> String {
    let who = r
        .key()
        .map(|k| format!("{}:{}", k.source.as_str(), k.id.as_str()))
        .unwrap_or_else(|| "unmatched".into());
    format!(
        "{who} {} ({})",
        r.title.as_deref().unwrap_or("?"),
        r.pub_year.as_deref().unwrap_or("?")
    )
}

/// A publication the application already knows, without searching.
fn known() -> RecordKey {
    RecordKey::new("MED", "20485428").unwrap()
}

async fn selected(api: &EuropePmc) -> RecordKey {
    let page = api
        .search(Query::new("malaria").unwrap(), PageSize::new(1).unwrap())
        .json()
        .send()
        .await
        .unwrap();
    page.result_list.result[0].key()
}

#[tokio::test]
async fn bibliography_of_a_selected_publication_in_json_then_follow_a_matched_reference() {
    let server = service().await;
    let api = client(&server);
    let key = selected(&api).await;

    let references = api
        .publication(key.source, key.id)
        .references(PageSize::new(SIZE as u16).unwrap());
    let mut pages = references
        .pages(SearchFormat::Json, Resume::Start, cap(5))
        .unwrap();
    let mut bibliography = Vec::new();
    let mut matched = Vec::new();
    while let Some(page) = pages.next_page().await.unwrap() {
        assert_eq!(
            page.meta, TOTAL as u64,
            "every page reports the same hit count"
        );
        for reference in page.items {
            bibliography.push(line(&reference));
            matched.extend(reference.key());
        }
    }
    assert_eq!(bibliography.len(), TOTAL);
    assert_eq!(bibliography[0], "MED:40000001 Reference 1 (2001)");
    assert_eq!(bibliography[1], "unmatched Reference 2 (2002)");
    assert_eq!(bibliography[2], "PMC:PMC800003 Reference 3 (2003)");
    assert_eq!(matched.len(), 5);

    // A matched reference is an ordinary record identity.
    let record = api.fetch_citation(&matched[0]).await.unwrap();
    assert_eq!(record.key(), matched[0]);
    assert_eq!(record.abstract_text.as_deref(), Some("Synthetic abstract."));

    let pages_sent: Vec<_> = server
        .requests()
        .iter()
        .filter(|r| r.path().ends_with("/references"))
        .map(|r| (r.query_all("page"), r.query_all("pageSize")))
        .collect();
    assert_eq!(
        pages_sent,
        [
            (vec!["1".to_string()], vec!["3".to_string()]),
            (vec!["2".to_string()], vec!["3".to_string()]),
            (vec!["3".to_string()], vec!["3".to_string()]),
        ]
    );
    server.shutdown().await;
}

#[tokio::test]
async fn xml_bibliography_under_a_page_cap_is_checkpointed_and_resumed_to_the_same_list() {
    let server = service().await;
    let api = client(&server);
    let key = selected(&api).await;
    let references = api
        .publication(key.source.clone(), key.id.clone())
        .references(PageSize::new(SIZE as u16).unwrap());
    let json: Vec<String> = references
        .pages(SearchFormat::Json, Resume::Start, cap(5))
        .unwrap()
        .items()
        .collect_complete()
        .await
        .unwrap()
        .iter()
        .map(line)
        .collect();

    // A worker allowed two requests per run stores the checkpoint and continues later.
    let first_run = references
        .pages(SearchFormat::Xml, Resume::Start, cap(2))
        .unwrap()
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert!(matches!(first_run.error, PagesError::Bounded { .. }));
    let checkpoint = first_run.error.resume().unwrap().clone();
    assert_eq!(checkpoint, Resume::At(PageNumber::new(3).unwrap()));
    let mut xml: Vec<String> = first_run.collected.iter().map(line).collect();
    assert_eq!(xml.len(), 2 * SIZE);

    let second_run = api
        .publication(key.source, key.id)
        .references(PageSize::new(SIZE as u16).unwrap())
        .pages(SearchFormat::Xml, checkpoint, cap(2))
        .unwrap()
        .items()
        .collect_complete()
        .await
        .unwrap();
    xml.extend(second_run.iter().map(line));
    assert_eq!(xml, json);
    server.shutdown().await;
}

#[tokio::test]
async fn a_server_ignoring_format_json_or_failing_midway_is_reported_not_misread() {
    // A deployment that always answers with the XML default.
    let server = TestServer::start(|r: &Recorded| -> Reply {
        Reply::text(200, XML_TYPE, references_xml(r))
    })
    .await;
    let api = client(&server);
    let references = api
        .publication(known().source, known().id)
        .references(PageSize::new(SIZE as u16).unwrap());
    let refused = references
        .pages(SearchFormat::Json, Resume::Start, cap(5))
        .unwrap()
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert!(refused.collected.is_empty());
    let sdk = refused.error.provider_error().unwrap();
    assert_eq!(sdk.call_error().unwrap().kind(), ErrorKind::Metadata);
    // The caller switches to the XML mode for the same plan.
    let xml = references
        .pages(SearchFormat::Xml, Resume::Start, cap(5))
        .unwrap()
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(xml.len(), TOTAL);
    server.shutdown().await;

    // Page 2 is throttled: page 1 is kept, the caller waits as told and resumes at page 2.
    let server = TestServer::start(|r: &Recorded| -> Reply {
        if r.query_all("page") == ["2"] {
            Reply::text(429, "text/plain", "slow down").header("retry-after", "3")
        } else {
            Reply::text(200, JSON_TYPE, references_json(r))
        }
    })
    .await;
    let api = client(&server);
    let references = api
        .publication(known().source, known().id)
        .references(PageSize::new(SIZE as u16).unwrap());
    let partial = references
        .pages(SearchFormat::Json, Resume::Start, cap(5))
        .unwrap()
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(partial.collected.len(), SIZE);
    assert_eq!(
        partial.error.resume(),
        Some(&Resume::At(PageNumber::new(2).unwrap()))
    );
    let Some(Error::Http(http)) = partial.error.provider_error() else {
        panic!("expected an HTTP failure");
    };
    assert_eq!(http.meta.status().as_u16(), 429);
    assert_eq!(
        http.retry_after(SystemTime::now()).unwrap(),
        Some(Duration::from_secs(3))
    );
    assert_eq!(server.requests().len(), 2, "the SDK does not retry");
    server.shutdown().await;
}
