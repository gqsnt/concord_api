//! Fresh-consumer application for SDK49 (Europe PMC), written from the README and rustdoc.
//!
//! Workflow 1: search publications in JSON, select one, read its first citing publication and
//! fetch that citation's core metadata. Workflow 2: run the same bounded search in XML and
//! continue with the returned cursor mark. Every response comes from a loopback test server;
//! the bodies are written from the official documentation (REST page and the 2020 EMBL-EBI
//! webinar slides), not copied from the SDK's own fixtures.

use concord_sdk_europe_pmc::concord::ErrorKind;
use concord_sdk_europe_pmc::concord_pagination::{PagesError, Resume};
use concord_sdk_europe_pmc::{
    Cursor, Error, EuropePmc, PageSize, PublicationId, Query, RecordKey, SearchFormat, Source,
    http_client,
};
use concord_testserver::{Recorded, Reply, TestServer};
use std::num::NonZeroUsize;
use std::time::{Duration, SystemTime};

const BASE_PATH: &str = "/europepmc/webservices/rest";
const CURSOR_1: &str = "AoIIQKsQtyg0MjQ5NDc4NA==";
const CURSOR_2: &str = "AoIIQKuyOCgzODQzNzM5Ng==";
const XML_TYPE: &str = "application/xml;charset=UTF-8";
const JSON_TYPE: &str = "application/json;charset=UTF-8";

// ---------------------------------------------------------------- fixtures (from the docs)

/// One lite search record as the provider documents it (webinar pp. 34, 44).
struct Rec {
    id: &'static str,
    source: &'static str,
    title: &'static str,
    authors: &'static str,
    journal: &'static str,
    year: &'static str,
    doi: &'static str,
}

const MALARIA: [Rec; 5] = [
    Rec {
        id: "20485428",
        source: "MED",
        title: "Chemical genetics of Plasmodium falciparum.",
        authors: "Guiguemde WA, Shelat AA, Bouck D, et al.",
        journal: "Nature",
        year: "2010",
        doi: "10.1038/nature09099",
    },
    Rec {
        id: "PMC7561308",
        source: "PMC",
        title: "Malaria in travellers.",
        authors: "Doe J.",
        journal: "BMJ",
        year: "2020",
        doi: "10.1136/bmj.example",
    },
    Rec {
        id: "PPR218846",
        source: "PPR",
        title: "Artemisinin resistance markers.",
        authors: "Roe R.",
        journal: "bioRxiv",
        year: "2021",
        doi: "10.1101/2021.01.01.000001",
    },
    Rec {
        id: "33176044",
        source: "MED",
        title: "Vector control and malaria.",
        authors: "Poe P.",
        journal: "Lancet",
        year: "2020",
        doi: "10.1016/example",
    },
    Rec {
        id: "34567890",
        source: "MED",
        title: "Malaria vaccines: a review.",
        authors: "Moe M.",
        journal: "Vaccine",
        year: "2021",
        doi: "10.1016/vaccine.example",
    },
];

fn json_search(hit: u64, sent: &str, next: Option<&str>, recs: &[Rec]) -> String {
    let next = next
        .map(|c| format!(r#""nextCursorMark":"{c}","#))
        .unwrap_or_default();
    let results: Vec<String> = recs
        .iter()
        .map(|r| {
            format!(
                r#"{{"id":"{}","source":"{}","pmid":"{}","doi":"{}","title":"{}","authorString":"{}","journalTitle":"{}","pubYear":"{}","isOpenAccess":"N","citedByCount":293}}"#,
                r.id, r.source, r.id, r.doi, r.title, r.authors, r.journal, r.year
            )
        })
        .collect();
    format!(
        r#"{{"version":"6.9","hitCount":{hit},{next}"request":{{"queryString":"malaria","resultType":"lite","cursorMark":"{sent}","pageSize":2,"sort":"","synonym":false}},"resultList":{{"result":[{}]}}}}"#,
        results.join(",")
    )
}

fn xml_search(hit: u64, sent: &str, next: Option<&str>, recs: &[Rec]) -> String {
    let next = next
        .map(|c| format!("<nextCursorMark>{c}</nextCursorMark>"))
        .unwrap_or_default();
    let results: String = recs
        .iter()
        .map(|r| {
            format!(
                "<result><id>{}</id><source>{}</source><pmid>{}</pmid><fullTextIdList><fullTextId>PMC2874979</fullTextId></fullTextIdList><doi>{}</doi><title>{}</title><authorString>{}</authorString><journalTitle>{}</journalTitle><pubYear>{}</pubYear><isOpenAccess>N</isOpenAccess></result>",
                r.id, r.source, r.id, r.doi, r.title, r.authors, r.journal, r.year
            )
        })
        .collect();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><responseWrapper xmlns:slx="http://www.scholix.org" xmlns:epmc="https://www.europepmc.org/data"><script/><version>6.9</version><hitCount>{hit}</hitCount>{next}<request><queryString>malaria</queryString><resultType>lite</resultType><cursorMark>{sent}</cursorMark><pageSize>2</pageSize><sort/><synonym>false</synonym></request><resultList>{results}</resultList></responseWrapper>"#
    )
}

/// Citations of MED/20485428, page 1 of size 1 (webinar pp. 36-37, as JSON).
const CITATIONS: &str = r#"{"version":"6.5","hitCount":288,"request":{"id":"20485428","source":"MED","offSet":0,"pageSize":1},"citationList":{"citation":[{"id":"32403408","source":"MED","citationType":"research-article; journal article","title":"Predicting Drug Resistance Using Deep Mutational Scanning.","authorString":"Pines G, Fankhauser RG, Eckert CA.","journalAbbreviation":"Molecules","pubYear":"2020","volume":"25","issue":"9","citedByCount":0}]}}"#;

/// Core record for MED/32403408 (core: abstract and journal info; webinar p. 40).
fn core_record(id: &str, source: &str) -> String {
    format!(
        r#"{{"id":"{id}","source":"{source}","pmid":"{id}","doi":"10.3390/molecules25092265","title":"Predicting Drug Resistance Using Deep Mutational Scanning.","authorString":"Pines G, Fankhauser RG, Eckert CA.","authorList":{{"author":[{{"fullName":"Pines G"}}]}},"journalInfo":{{"volume":"25","issue":"9","journal":{{"title":"Molecules (Basel, Switzerland)","isoabbreviation":"Molecules"}}}},"pubYear":"2020","abstractText":"Drug resistance is a major healthcare challenge.","meshHeadingList":{{"meshHeading":[]}}}}"#
    )
}

fn core_search(records: &[String]) -> String {
    core_search_hits(records.len(), records)
}

fn core_search_hits(hits: usize, records: &[String]) -> String {
    format!(
        r#"{{"version":"6.9","hitCount":{hits},"request":{{"queryString":"(core lookup)","resultType":"core","cursorMark":"*","pageSize":1,"sort":"","synonym":false}},"resultList":{{"result":[{}]}}}}"#,
        records.join(",")
    )
}

fn reply(content_type: &str, body: impl AsRef<[u8]>) -> Reply {
    Reply::new(200)
        .header("content-type", content_type)
        .body(body)
}

fn one(r: &Recorded, name: &str) -> Option<String> {
    let mut all = r.query_all(name);
    assert!(all.len() <= 1, "{name} sent {} times", all.len());
    all.pop()
}

async fn client(server: &TestServer) -> EuropePmc {
    EuropePmc::new(http_client().expect("transport"))
        .with_base_url(format!("{}{BASE_PATH}", server.url()))
        .expect("loopback base URL")
}

fn malaria(api: &EuropePmc) -> concord_sdk_europe_pmc::EuropePmcSearch {
    api.search(
        Query::new("malaria AND OPEN_ACCESS:y").expect("query"),
        PageSize::new(2).expect("page size"),
    )
}

fn kind(error: &Error) -> Option<ErrorKind> {
    error.call_error().map(|e| e.kind())
}

/// The headers a public, credential-free request must (not) carry.
fn assert_plain_get(r: &Recorded, accept: &str) {
    assert_eq!(r.method, "GET");
    assert_eq!(r.header("accept"), Some(accept));
    for absent in ["authorization", "cookie", "referer", "content-type"] {
        assert!(r.header_all(absent).is_empty(), "{absent} was sent");
    }
    assert_eq!(r.body_len, 0);
}

// ------------------------------------------------------------------- workflow 1 (JSON)

/// Responder for the whole JSON workflow: lite search → citations → core lookup.
fn json_workflow(r: &Recorded) -> Reply {
    let path = r.path().to_owned();
    if path == format!("{BASE_PATH}/search") {
        match one(r, "resultType").as_deref() {
            Some("lite") => reply(
                JSON_TYPE,
                json_search(197_037, "*", Some(CURSOR_1), &MALARIA[..2]),
            ),
            Some("core") => reply(JSON_TYPE, core_search(&[core_record("32403408", "MED")])),
            other => panic!("unexpected resultType {other:?}"),
        }
    } else if path == format!("{BASE_PATH}/MED/20485428/citations") {
        reply(JSON_TYPE, CITATIONS)
    } else {
        Reply::text(404, "text/html", "<html>Not Found</html>")
    }
}

#[tokio::test]
async fn workflow_1_json_search_select_publication_and_fetch_one_citation() {
    let server = TestServer::start(json_workflow).await;
    let api = client(&server).await;
    let search = malaria(&api);

    // 1. One JSON page through the ordinary generated send.
    let page = search.json().send().await.expect("search page");
    assert_eq!(page.hit_count, 197_037);
    assert_eq!(
        page.next_cursor_mark.as_ref().map(Cursor::as_str),
        Some(CURSOR_1)
    );
    assert_eq!(page.result_list.result.len(), 2);
    let selected = &page.result_list.result[0];
    assert_eq!(
        selected.title.as_deref(),
        Some("Chemical genetics of Plasmodium falciparum.")
    );
    assert_eq!(selected.journal_title.as_deref(), Some("Nature"));
    assert_eq!(selected.doi.as_deref(), Some("10.1038/nature09099"));
    assert_eq!(selected.pub_year.as_deref(), Some("2010"));
    assert_eq!(
        selected.abstract_text, None,
        "lite results carry no abstract"
    );

    // 2. The selected publication's first citing publication.
    let key = selected.key();
    assert_eq!(key, RecordKey::new("MED", "20485428").unwrap());
    let citing = api
        .publication(key.source, key.id)
        .citations()
        .set_page_size(PageSize::new(1).unwrap())
        .send()
        .await
        .expect("citations page");
    assert_eq!(citing.hit_count, 288);
    let [citation] = citing.citation_list.citation.as_slice() else {
        panic!("one citation expected");
    };
    assert_eq!(citation.key(), RecordKey::new("MED", "32403408").unwrap());
    assert_eq!(
        citation.author_string.as_deref(),
        Some("Pines G, Fankhauser RG, Eckert CA.")
    );
    // Citations carry `journalAbbreviation`; the model has only `journal_title`.
    let citation_journal = citation.journal_title.clone();

    // 3. Exact core metadata for that citation.
    let core = api
        .fetch_citation(&citation.key())
        .await
        .expect("core lookup");
    assert_eq!(core.key(), citation.key());
    assert_eq!(
        core.abstract_text.as_deref(),
        Some("Drug resistance is a major healthcare challenge.")
    );
    assert_eq!(core.doi.as_deref(), Some("10.3390/molecules25092265"));
    let core_journal = core.journal_title.clone();

    // Wire: three requests, in order, each a plain public GET.
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    let [search_req, citations_req, core_req] = requests.as_slice() else {
        unreachable!()
    };
    assert_eq!(search_req.path(), format!("{BASE_PATH}/search"));
    assert_eq!(
        search_req.query_pairs(),
        [
            ("query", "malaria AND OPEN_ACCESS:y"),
            ("pageSize", "2"),
            ("cursorMark", "*"),
            ("format", "json"),
            ("resultType", "lite"),
        ]
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
    );
    assert_plain_get(search_req, "application/json");

    assert_eq!(
        citations_req.path(),
        format!("{BASE_PATH}/MED/20485428/citations")
    );
    assert_eq!(one(citations_req, "pageSize").as_deref(), Some("1"));
    assert_eq!(one(citations_req, "format").as_deref(), Some("json"));
    assert_eq!(one(citations_req, "page").as_deref(), Some("1"));
    assert_plain_get(citations_req, "application/json");

    assert_eq!(core_req.path(), format!("{BASE_PATH}/search"));
    assert_eq!(one(core_req, "format").as_deref(), Some("json"));
    assert_eq!(one(core_req, "resultType").as_deref(), Some("core"));
    assert_eq!(one(core_req, "cursorMark"), None);
    let core_query = one(core_req, "query").expect("core query");
    assert!(core_query.contains("32403408") && core_query.contains("MED"));
    assert_plain_get(core_req, "application/json");

    eprintln!("raw search target: {}", search_req.target);
    eprintln!("raw core target:   {}", core_req.target);
    eprintln!("core query (decoded): {core_query}");
    eprintln!("core pageSize: {:?}", one(core_req, "pageSize"));
    eprintln!("citation journal_title: {citation_journal:?}; core journal_title: {core_journal:?}");
    let cleanup = server.shutdown().await;
    assert_eq!(cleanup.open_after, 0);
}

#[tokio::test]
async fn workflow_1_through_the_json_traversal_has_page_metadata() {
    let server = TestServer::start(json_workflow).await;
    let api = client(&server).await;
    let mut pages = malaria(&api)
        .pages(
            SearchFormat::Json,
            Resume::Start,
            NonZeroUsize::new(1).unwrap(),
        )
        .unwrap();
    let page = pages.next_page().await.unwrap().expect("first page");
    assert_eq!(page.meta, 197_037);
    assert_eq!(page.next.as_ref().map(Cursor::as_str), Some(CURSOR_1));
    assert_eq!(page.items.len(), 2);
    // The cap of one page was reached while the provider has more: bounded, not done.
    let err = pages.next_page().await.unwrap_err();
    assert!(matches!(err, PagesError::Bounded { .. }), "{err:?}");
    assert_eq!(
        err.resume(),
        Some(&Resume::At(Cursor::new(CURSOR_1).unwrap()))
    );
    assert_eq!(server.requests().len(), 1);
}

#[tokio::test]
async fn workflow_1_xml_default_is_never_fed_to_the_json_decoder() {
    // A proxy (or a dropped `format` parameter) yields the provider's XML default.
    let xml = xml_search(197_037, "*", Some(CURSOR_1), &MALARIA[..2]);
    let server = TestServer::start(move |_r: &Recorded| reply(XML_TYPE, xml.clone())).await;
    let api = client(&server).await;

    let err = malaria(&api).json().send().await.unwrap_err();
    assert_eq!(kind(&err), Some(ErrorKind::Metadata), "{err:?}");
    assert_eq!(err.status().map(|s| s.as_u16()), Some(200));
    let r = server.single();
    assert_eq!(
        one(&r, "format").as_deref(),
        Some("json"),
        "JSON was requested explicitly"
    );

    // The same refusal inside a traversal ends it as a provider failure on the first page.
    let mut pages = malaria(&api)
        .pages(
            SearchFormat::Json,
            Resume::Start,
            NonZeroUsize::new(5).unwrap(),
        )
        .unwrap();
    let err = pages.next_page().await.unwrap_err();
    assert_eq!(err.resume(), Some(&Resume::Start));
    assert_eq!(
        err.provider_error().and_then(kind),
        Some(ErrorKind::Metadata)
    );
    assert!(matches!(
        pages.next_page().await,
        Err(PagesError::Ended { .. })
    ));
    assert_eq!(server.requests().len(), 2);
}

#[tokio::test]
async fn workflow_1_mislabelled_and_malformed_json_fail_explicitly() {
    let xml = xml_search(2, "*", None, &MALARIA[..2]);
    let cases: Vec<(&str, Reply, ErrorKind)> = vec![
        (
            "xml labelled json",
            reply("application/json", xml),
            ErrorKind::Decode,
        ),
        (
            "truncated json",
            reply(
                JSON_TYPE,
                r#"{"version":"6.9","hitCount":2,"resultList":{"result":[{"id":"#,
            ),
            ErrorKind::Decode,
        ),
        (
            "html error page",
            reply("text/html", "<html>busy</html>"),
            ErrorKind::Metadata,
        ),
        (
            "oversized",
            reply(JSON_TYPE, vec![b' '; 300 * 1024]),
            ErrorKind::Limit,
        ),
        (
            // A 200 document without results (no documented error schema exists).
            "no resultList",
            reply(
                JSON_TYPE,
                r#"{"version":"6.9","hitCount":0,"request":{"queryString":"malaria"}}"#,
            ),
            ErrorKind::Decode,
        ),
        (
            "empty json cursor",
            reply(JSON_TYPE, json_search(5, "*", Some(""), &MALARIA[..2])),
            ErrorKind::Decode,
        ),
    ];
    for (name, scripted, expected) in cases {
        let scripted = std::sync::Mutex::new(Some(scripted));
        let server = TestServer::start(move |_r: &Recorded| {
            scripted.lock().unwrap().take().expect("one request")
        })
        .await;
        let api = client(&server).await;
        let err = malaria(&api).json().send().await.unwrap_err();
        assert_eq!(kind(&err), Some(expected), "{name}: {err:?}");
        assert_eq!(err.status().map(|s| s.as_u16()), Some(200), "{name}");
        assert_eq!(server.requests().len(), 1, "{name}: no retry");
    }
}

#[tokio::test]
async fn workflow_1_http_failures_keep_status_body_and_retry_after() {
    let server = TestServer::start(|r: &Recorded| {
        if r.path().ends_with("/citations") {
            Reply::text(429, "text/plain", "slow down").header("retry-after", "120")
        } else {
            reply(
                JSON_TYPE,
                json_search(197_037, "*", Some(CURSOR_1), &MALARIA[..2]),
            )
        }
    })
    .await;
    let api = client(&server).await;

    // Search succeeds; the citations step fails. The caller keeps the partial result.
    let page = malaria(&api).json().send().await.unwrap();
    let selected = page.result_list.result[0].key();
    let err = api
        .publication(selected.source.clone(), selected.id.clone())
        .citations()
        .send()
        .await
        .unwrap_err();
    assert_eq!(err.status().map(|s| s.as_u16()), Some(429));
    let Error::Http(http) = &err else {
        panic!("expected Error::Http, got {err:?}")
    };
    assert_eq!(http.meta.status().as_u16(), 429);
    assert_eq!(http.body.as_deref().ok(), Some(&b"slow down"[..]));
    assert_eq!(
        http.retry_after(SystemTime::now()).unwrap(),
        Some(Duration::from_secs(120))
    );
    assert_eq!(
        http.meta.operation().map(|o| o.name()),
        Some("EuropePmc.publication.citations")
    );
    // Default page size of the citations request is the SDK default (25).
    assert_eq!(
        one(&server.requests()[1], "pageSize").as_deref(),
        Some("25")
    );
    assert_eq!(server.requests().len(), 2, "no automatic retry");
    // The partial result: the selected publication survives the failed step.
    assert_eq!(selected, RecordKey::new("MED", "20485428").unwrap());

    // Diagnostics do not leak the search expression, cursor or bodies.
    let search = malaria(&api);
    for text in [
        format!("{err}"),
        format!("{err:?}"),
        format!("{search:?}"),
        format!("{:?}", search.json()),
        format!("{api:?}"),
        format!("{:?}", page.result_list.result[0]),
    ] {
        for secret in [
            "OPEN_ACCESS",
            "malaria",
            CURSOR_1,
            "slow down",
            "Plasmodium",
        ] {
            assert!(!text.contains(secret), "{secret} leaked in {text}");
        }
    }
}

#[tokio::test]
async fn workflow_1_zero_hits_is_an_empty_final_page_and_redirects_are_http_errors() {
    let server = TestServer::start(|r: &Recorded| {
        if r.path().ends_with("/citations") {
            Reply::new(302).header("location", "https://www.ebi.ac.uk/europepmc/elsewhere")
        } else {
            reply(JSON_TYPE, json_search(0, "*", None, &[]))
        }
    })
    .await;
    let api = client(&server).await;
    let mut pages = malaria(&api)
        .pages(
            SearchFormat::Json,
            Resume::Start,
            NonZeroUsize::new(2).unwrap(),
        )
        .unwrap();
    let page = pages
        .next_page()
        .await
        .unwrap()
        .expect("one empty final page");
    assert!(page.items.is_empty() && page.next.is_none());
    assert_eq!(page.meta, 0);
    assert!(pages.next_page().await.unwrap().is_none());
    assert_eq!(server.requests().len(), 1);

    let err = api
        .publication(
            Source::new("MED").unwrap(),
            PublicationId::new("20485428").unwrap(),
        )
        .citations()
        .send()
        .await
        .unwrap_err();
    assert!(matches!(err, Error::Http(_)), "{err:?}");
    assert_eq!(err.status().map(|s| s.as_u16()), Some(302));
    assert_eq!(server.requests().len(), 2, "the redirect was not followed");
}

#[tokio::test]
async fn workflow_1_fetch_citation_is_exact() {
    // Zero records: NotFound. A different identity or two records: UnexpectedRecord.
    let bodies = [
        (core_search(&[]), "not found"),
        (core_search(&[core_record("99999999", "MED")]), "other id"),
        (
            core_search(&[core_record("32403408", "PMC")]),
            "other source",
        ),
        (
            core_search(&[
                core_record("32403408", "MED"),
                core_record("32403408", "MED"),
            ]),
            "two records",
        ),
        // The realistic ambiguity: pageSize=1 returns one record but reports two hits.
        (
            core_search_hits(2, &[core_record("32403408", "MED")]),
            "two hits, one record",
        ),
    ];
    for (body, name) in bodies {
        let server = TestServer::start(move |_r: &Recorded| reply(JSON_TYPE, body.clone())).await;
        let api = client(&server).await;
        let err = api
            .fetch_citation(&RecordKey::new("MED", "32403408").unwrap())
            .await
            .unwrap_err();
        match name {
            "not found" => assert!(matches!(err, Error::NotFound), "{name}: {err:?}"),
            // Observed: more records than the lookup's pageSize=1 is an envelope violation,
            // reported before the identity check (the rustdoc promised UnexpectedRecord).
            "two records" => assert!(matches!(err, Error::InvalidPage(_)), "{name}: {err:?}"),
            _ => assert!(matches!(err, Error::UnexpectedRecord), "{name}: {err:?}"),
        }
        eprintln!("fetch_citation {name}: {err:?}");
        assert_eq!(server.requests().len(), 1);
    }
}

#[tokio::test]
async fn workflow_1_excess_records_and_invalid_inputs_are_refused() {
    let server = TestServer::start(|_r: &Recorded| {
        reply(
            JSON_TYPE,
            json_search(5, "*", Some(CURSOR_1), &MALARIA[..3]),
        )
    })
    .await;
    let api = client(&server).await;
    let err = malaria(&api).json().send().await.unwrap_err();
    assert!(matches!(err, Error::InvalidPage(_)), "{err:?}");

    // Local refusals, before any I/O.
    assert!(matches!(Query::new("   "), Err(Error::InvalidInput(_))));
    assert!(matches!(PageSize::new(0), Err(Error::InvalidInput(_))));
    assert!(matches!(PageSize::new(101), Err(Error::InvalidInput(_))));
    assert!(PageSize::new(100).is_ok());
    assert!(matches!(Source::new("med"), Err(Error::InvalidInput(_))));
    assert!(matches!(
        PublicationId::new("a/b"),
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(Cursor::new(""), Err(Error::InvalidInput(_))));
    assert!(matches!(
        malaria(&api).pages(
            SearchFormat::Json,
            Resume::Start,
            NonZeroUsize::new(21).unwrap()
        ),
        Err(Error::InvalidInput(_))
    ));
    assert_eq!(server.requests().len(), 1, "refusals sent nothing");
}

// -------------------------------------------------------------------- workflow 2 (XML)

/// Stateless XML responder routed by the cursor mark the SDK sends.
fn xml_chain(r: &Recorded) -> Reply {
    assert_eq!(one(r, "format").as_deref(), Some("xml"));
    match one(r, "cursorMark").as_deref() {
        Some("*") => reply(XML_TYPE, xml_search(5, "*", Some(CURSOR_1), &MALARIA[..2])),
        Some(CURSOR_1) => reply(
            XML_TYPE,
            xml_search(5, CURSOR_1, Some(CURSOR_2), &MALARIA[2..4]),
        ),
        Some(CURSOR_2) => reply(XML_TYPE, xml_search(5, CURSOR_2, None, &MALARIA[4..])),
        other => panic!("unexpected cursor {other:?}"),
    }
}

#[tokio::test]
async fn workflow_2_same_search_in_xml_continues_with_the_cursor_to_completion() {
    let server = TestServer::start(xml_chain).await;
    let api = client(&server).await;
    let search = malaria(&api);
    let mut xml = search
        .pages(
            SearchFormat::Xml,
            Resume::Start,
            NonZeroUsize::new(5).unwrap(),
        )
        .unwrap();

    let mut ids = Vec::new();
    let mut hit_counts = Vec::new();
    while let Some(page) = xml.next_page().await.expect("page") {
        hit_counts.push(page.meta);
        ids.extend(page.items.into_iter().map(|p| p.id.as_str().to_owned()));
    }
    assert_eq!(ids, MALARIA.map(|r| r.id.to_owned()));
    assert_eq!(hit_counts, [5, 5, 5]);
    // Completion is sticky and costs no request.
    assert!(xml.next_page().await.unwrap().is_none());
    assert!(xml.checkpoint().is_none());

    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    let cursors: Vec<_> = requests
        .iter()
        .map(|r| one(r, "cursorMark").unwrap())
        .collect();
    assert_eq!(cursors, ["*", CURSOR_1, CURSOR_2]);
    for r in &requests {
        assert_eq!(r.path(), format!("{BASE_PATH}/search"));
        // Same bounded search as workflow 1: only format differs.
        assert_eq!(
            one(r, "query").as_deref(),
            Some("malaria AND OPEN_ACCESS:y")
        );
        assert_eq!(one(r, "pageSize").as_deref(), Some("2"));
        assert_eq!(one(r, "resultType").as_deref(), Some("lite"));
        assert_eq!(one(r, "format").as_deref(), Some("xml"));
        assert_plain_get(r, requests[0].header("accept").unwrap());
    }
    eprintln!("xml accept: {:?}", requests[0].header("accept"));
    eprintln!("raw continuation target: {}", requests[1].target);
    // The opaque cursor's `=` padding is percent-encoded, never sent raw.
    assert!(
        requests[1]
            .target
            .contains("cursorMark=AoIIQKsQtyg0MjQ5NDc4NA%3D%3D")
    );
}

#[tokio::test]
async fn workflow_2_single_xml_page_then_explicit_cursor() {
    let server = TestServer::start(xml_chain).await;
    let api = client(&server).await;
    let search = malaria(&api);
    let first = search.xml().send().await.unwrap();
    assert_eq!(first.hit_count, 5);
    assert_eq!(
        first.result_list.result[0].journal_title.as_deref(),
        Some("Nature")
    );
    let next = first.next_cursor_mark.expect("cursor");
    let second = search.xml().set_cursor(next).send().await.unwrap();
    assert_eq!(second.result_list.result[0].id.as_str(), "PPR218846");
    assert_eq!(
        second.next_cursor_mark.as_ref().map(Cursor::as_str),
        Some(CURSOR_2)
    );
    assert_eq!(server.requests().len(), 2);
}

#[tokio::test]
async fn workflow_2_stops_when_a_cursor_repeats() {
    // Page 2 hands back the cursor that was just requested: an infinite loop if followed.
    let server = TestServer::start(|r: &Recorded| match one(r, "cursorMark").as_deref() {
        Some("*") => reply(XML_TYPE, xml_search(5, "*", Some(CURSOR_1), &MALARIA[..2])),
        Some(CURSOR_1) => reply(
            XML_TYPE,
            xml_search(5, CURSOR_1, Some(CURSOR_1), &MALARIA[2..4]),
        ),
        other => panic!("request after a repeated cursor: {other:?}"),
    })
    .await;
    let api = client(&server).await;
    let mut xml = malaria(&api)
        .pages(
            SearchFormat::Xml,
            Resume::Start,
            NonZeroUsize::new(10).unwrap(),
        )
        .unwrap();
    let first = xml.next_page().await.unwrap().unwrap();
    assert_eq!(first.items.len(), 2);
    let err = xml.next_page().await.unwrap_err();
    assert!(
        matches!(err.provider_error(), Some(Error::RepeatedCursor)),
        "{err:?}"
    );
    // The page whose response repeated the cursor is the failed one (not emitted).
    assert_eq!(
        err.resume(),
        Some(&Resume::At(Cursor::new(CURSOR_1).unwrap()))
    );
    assert!(matches!(
        xml.next_page().await,
        Err(PagesError::Ended { .. })
    ));
    assert_eq!(server.requests().len(), 2, "no request after the repeat");

    // The item-level collection keeps the successful prefix only.
    let incomplete = malaria(&api)
        .pages(
            SearchFormat::Xml,
            Resume::Start,
            NonZeroUsize::new(10).unwrap(),
        )
        .unwrap()
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    let prefix: Vec<_> = incomplete.collected.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(prefix, ["20485428", "PMC7561308"]);
    assert!(matches!(
        incomplete.error.provider_error(),
        Some(Error::RepeatedCursor)
    ));
    assert_eq!(server.requests().len(), 4);
}

#[tokio::test]
async fn workflow_2_stops_on_a_longer_cursor_cycle() {
    let server = TestServer::start(|r: &Recorded| match one(r, "cursorMark").as_deref() {
        Some("*") => reply(XML_TYPE, xml_search(9, "*", Some(CURSOR_1), &MALARIA[..2])),
        Some(CURSOR_1) => reply(
            XML_TYPE,
            xml_search(9, CURSOR_1, Some(CURSOR_2), &MALARIA[2..4]),
        ),
        // Back to page 2: a two-page cycle.
        Some(CURSOR_2) => reply(
            XML_TYPE,
            xml_search(9, CURSOR_2, Some(CURSOR_1), &MALARIA[4..]),
        ),
        other => panic!("request after a cycle: {other:?}"),
    })
    .await;
    let api = client(&server).await;
    let mut xml = malaria(&api)
        .pages(
            SearchFormat::Xml,
            Resume::Start,
            NonZeroUsize::new(10).unwrap(),
        )
        .unwrap();
    assert_eq!(xml.next_page().await.unwrap().unwrap().items.len(), 2);
    assert_eq!(xml.next_page().await.unwrap().unwrap().items.len(), 2);
    let err = xml.next_page().await.unwrap_err();
    assert!(
        matches!(err.provider_error(), Some(Error::RepeatedCursor)),
        "{err:?}"
    );
    assert_eq!(server.requests().len(), 3);
}

#[tokio::test]
async fn workflow_2_bounded_traversal_resumes_from_the_returned_cursor() {
    let server = TestServer::start(xml_chain).await;
    let api = client(&server).await;
    let search = malaria(&api);
    let mut xml = search
        .pages(
            SearchFormat::Xml,
            Resume::Start,
            NonZeroUsize::new(1).unwrap(),
        )
        .unwrap();
    assert_eq!(xml.next_page().await.unwrap().unwrap().items.len(), 2);
    let err = xml.next_page().await.unwrap_err();
    let PagesError::Bounded { resume, .. } = err else {
        panic!("a cap with a next cursor is not completion: {err:?}")
    };
    assert_eq!(resume, Resume::At(Cursor::new(CURSOR_1).unwrap()));
    assert_eq!(server.requests().len(), 1);

    // A later run continues from that checkpoint with the same query, size and format.
    let rest = search
        .pages(SearchFormat::Xml, resume, NonZeroUsize::new(5).unwrap())
        .unwrap()
        .items()
        .collect_complete()
        .await
        .expect("rest of the listing");
    let ids: Vec<_> = rest.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, ["PPR218846", "33176044", "34567890"]);
    let requests = server.requests();
    assert_eq!(one(&requests[1], "cursorMark").as_deref(), Some(CURSOR_1));
    assert_eq!(requests.len(), 3);
}

#[tokio::test]
async fn workflow_2_empty_page_with_fresh_cursor_continues() {
    let server = TestServer::start(|r: &Recorded| match one(r, "cursorMark").as_deref() {
        Some("*") => reply(XML_TYPE, xml_search(2, "*", Some(CURSOR_1), &[])),
        Some(CURSOR_1) => reply(XML_TYPE, xml_search(2, CURSOR_1, None, &MALARIA[..2])),
        other => panic!("unexpected {other:?}"),
    })
    .await;
    let api = client(&server).await;
    let items = malaria(&api)
        .pages(
            SearchFormat::Xml,
            Resume::Start,
            NonZeroUsize::new(3).unwrap(),
        )
        .unwrap()
        .items()
        .collect_complete()
        .await
        .expect("complete");
    assert_eq!(items.len(), 2);
    assert_eq!(server.requests().len(), 2);
}

#[tokio::test]
async fn workflow_2_bad_xml_documents_fail_explicitly() {
    let dtd = r#"<?xml version="1.0"?><!DOCTYPE r [<!ENTITY x "boom">]><responseWrapper><hitCount>1</hitCount><resultList><result><id>1</id><source>MED</source><title>&x;</title></result></resultList></responseWrapper>"#;
    let json = json_search(2, "*", None, &MALARIA[..2]);
    let cases: Vec<(&str, Reply)> = vec![
        (
            "truncated",
            reply(
                XML_TYPE,
                "<responseWrapper><hitCount>5</hitCount><resultList><result><id>",
            ),
        ),
        ("dtd entity", reply(XML_TYPE, dtd)),
        ("json labelled xml", reply(XML_TYPE, json.clone())),
        ("json labelled json", reply(JSON_TYPE, json)),
        (
            "empty cursor",
            reply(XML_TYPE, xml_search(5, "*", Some(""), &MALARIA[..2])),
        ),
    ];
    for (name, scripted) in cases {
        let scripted = std::sync::Mutex::new(Some(scripted));
        let server = TestServer::start(move |_r: &Recorded| {
            scripted.lock().unwrap().take().expect("one request")
        })
        .await;
        let api = client(&server).await;
        let mut xml = malaria(&api)
            .pages(
                SearchFormat::Xml,
                Resume::Start,
                NonZeroUsize::new(3).unwrap(),
            )
            .unwrap();
        let err = xml.next_page().await.unwrap_err();
        let sdk = err.provider_error().expect("provider failure");
        eprintln!("xml {name}: kind={:?} sdk={sdk:?}", kind(sdk));
        match name {
            "json labelled json" => assert_eq!(kind(sdk), Some(ErrorKind::Metadata), "{name}"),
            "empty cursor" => assert!(
                matches!(sdk, Error::InvalidPage(_)) || kind(sdk) == Some(ErrorKind::Decode),
                "{name}: {sdk:?}"
            ),
            _ => assert_eq!(kind(sdk), Some(ErrorKind::Decode), "{name}"),
        }
        assert_eq!(server.requests().len(), 1, "{name}");
    }
}

#[tokio::test]
async fn workflow_2_http_error_page_is_typed_and_ends_the_chain() {
    let server = TestServer::start(|r: &Recorded| match one(r, "cursorMark").as_deref() {
        Some("*") => reply(XML_TYPE, xml_search(5, "*", Some(CURSOR_1), &MALARIA[..2])),
        _ => Reply::text(503, "text/html", "<html>Service Unavailable</html>"),
    })
    .await;
    let api = client(&server).await;
    let mut xml = malaria(&api)
        .pages(
            SearchFormat::Xml,
            Resume::Start,
            NonZeroUsize::new(5).unwrap(),
        )
        .unwrap();
    let first = xml.next_page().await.unwrap().unwrap();
    assert_eq!(first.items.len(), 2);
    let err = xml.next_page().await.unwrap_err();
    let sdk = err.provider_error().expect("typed provider error");
    assert_eq!(sdk.status().map(|s| s.as_u16()), Some(503));
    assert!(matches!(sdk, Error::Http(_)));
    // The failed page is the one to retry later.
    assert_eq!(
        err.resume(),
        Some(&Resume::At(Cursor::new(CURSOR_1).unwrap()))
    );
    assert!(matches!(
        xml.next_page().await,
        Err(PagesError::Ended { .. })
    ));
    assert_eq!(server.requests().len(), 2);
}

#[tokio::test]
async fn workflow_2_oversized_xml_is_a_limit_failure() {
    let mut big = xml_search(2, "*", None, &MALARIA[..2]);
    big.insert_str(
        big.len() - "</responseWrapper>".len(),
        &" ".repeat(300 * 1024),
    );
    let server = TestServer::start(move |_r: &Recorded| reply(XML_TYPE, big.clone())).await;
    let api = client(&server).await;
    let err = malaria(&api).xml().send().await.unwrap_err();
    assert_eq!(kind(&err), Some(ErrorKind::Limit), "{err:?}");
    assert_eq!(server.requests().len(), 1);
}
