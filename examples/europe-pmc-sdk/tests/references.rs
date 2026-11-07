//! Reference-list traversal: `GET /{source}/{id}/references` with `page`, `pageSize` and
//! `format` (ticket contract; the REST page's Request Methods section did not render on
//! 2026-11-07, see MAINTENANCE.md). Envelope names mirror the documented citations envelope
//! (webinar pp. 36–37). Every body is authored synthetic; the fault bodies are inline.
mod support;
use concord_sdk_europe_pmc::{
    BODY_LIMIT, Error, PageNumber, PageSize, RecordKey, SearchFormat,
    concord::{ErrorKind, bytes::Bytes},
    concord_pagination::{Outcome, PagesError, Resume},
};
use concord_testserver::{Body, Recorded, Reply, TestServer};
use std::{num::NonZeroUsize, time::Duration};
use support::*;

const REFS_JSON: [&str; 3] = [
    include_str!("../fixtures/references-1.json"),
    include_str!("../fixtures/references-2.json"),
    include_str!("../fixtures/references-3.json"),
];
const REFS_XML: [&str; 3] = [
    include_str!("../fixtures/references-1.xml"),
    include_str!("../fixtures/references-2.xml"),
    include_str!("../fixtures/references-3.xml"),
];
const PATH: &str = "/rest/MED/20485428/references";
const EXPECTED: [Option<&str>; 5] = [
    Some("31000001"),
    None,
    Some("PMC9000003"),
    Some("31000004"),
    Some("31000005"),
];

fn cap(n: usize) -> NonZeroUsize {
    NonZeroUsize::new(n).unwrap()
}

fn refs(
    api: &concord_sdk_europe_pmc::EuropePmc,
) -> concord_sdk_europe_pmc::EuropePmcPublicationReferences {
    let key = RecordKey::new("MED", "20485428").unwrap();
    api.publication(key.source, key.id)
        .references(PageSize::new(2).unwrap())
}

fn page_of(r: &Recorded) -> usize {
    r.query_all("page")[0].parse().unwrap()
}

/// Serves the three authored pages of the requested format; asserts the fixed plan.
async fn three_pages() -> TestServer {
    TestServer::start(|r: &Recorded| -> Reply {
        assert_eq!(r.path(), PATH);
        assert_eq!(r.query_all("pageSize"), ["2"]);
        let page = page_of(r);
        // Past the last page, a stand-in answers with the hit count and no references.
        match r.query_all("format")[0].as_str() {
            "json" => Reply::json(
                200,
                REFS_JSON
                    .get(page - 1)
                    .copied()
                    .unwrap_or("{\"hitCount\":5}"),
            ),
            "xml" => Reply::text(
                200,
                "application/xml",
                REFS_XML
                    .get(page - 1)
                    .copied()
                    .unwrap_or("<responseWrapper><hitCount>5</hitCount></responseWrapper>"),
            ),
            _ => panic!("unexpected format"),
        }
    })
    .await
}

fn ids(items: &[concord_sdk_europe_pmc::Reference]) -> Vec<Option<&str>> {
    items
        .iter()
        .map(|r| r.id.as_ref().map(|id| id.as_str()))
        .collect()
}

#[tokio::test]
async fn wire_shape_is_path_page_size_page_and_explicit_format_in_both_modes() {
    let server = three_pages().await;
    let api = api(&server);
    refs(&api).json().send().await.unwrap();
    refs(&api)
        .xml()
        .set_page(PageNumber::new(3).unwrap())
        .send()
        .await
        .unwrap();
    let wire = server.requests();
    assert_eq!(
        wire[0].query_pairs(),
        [
            ("pageSize".into(), "2".into()),
            ("page".into(), "1".into()),
            ("format".into(), "json".into()),
        ]
    );
    assert_eq!(wire[0].header("accept"), Some("application/json"));
    assert_eq!(wire[1].query_all("page"), ["3"]);
    assert_eq!(wire[1].query_all("format"), ["xml"]);
    assert_eq!(wire[1].header("accept"), Some("application/xml"));
    for r in &wire {
        assert_eq!(r.method, "GET");
        assert!(r.query_all("cursorMark").is_empty());
        assert!(r.header("authorization").is_none());
    }
    server.shutdown().await;
}

#[tokio::test]
async fn an_xml_default_is_refused_on_json_references_before_its_body_is_read() {
    // A stand-in ignores format=json and answers with the documented XML default; the stalled
    // body proves the refusal happens at the media check. The XML mode refuses JSON likewise.
    for (format, media) in [
        (SearchFormat::Json, "application/xml"),
        (SearchFormat::Json, "text/xml;charset=UTF-8"),
        (SearchFormat::Xml, "application/json"),
    ] {
        let server = TestServer::start(move |_: &Recorded| -> Reply {
            Reply::new(200)
                .header("content-type", media)
                .with_body(Body::Stall)
        })
        .await;
        let api = api(&server);
        let mut pages = refs(&api).pages(format, Resume::Start, cap(3)).unwrap();
        let failed = tokio::time::timeout(Duration::from_secs(1), pages.next_page())
            .await
            .unwrap()
            .unwrap_err();
        let error = failed.provider_error().unwrap();
        assert_eq!(
            error.call_error().unwrap().kind(),
            ErrorKind::Metadata,
            "{media}"
        );
        assert!(error.call_error().unwrap().codec_error().is_some());
        assert_eq!(failed.resume(), Some(&Resume::Start));
        assert!(matches!(
            pages.next_page().await,
            Err(PagesError::Ended { .. })
        ));
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
    // XML bytes falsely labelled JSON are a JSON decode failure, never a JSON success.
    let server = fixed(200, "application/json", REFS_XML[0]).await;
    let error = refs(&api(&server)).json().send().await.unwrap_err();
    assert_eq!(error.call_error().unwrap().kind(), ErrorKind::Decode);
    server.shutdown().await;
}

#[tokio::test]
async fn unmatched_references_and_numeric_years_decode_identically_in_both_formats() {
    let server = three_pages().await;
    let api = api(&server);
    let json = refs(&api).json().send().await.unwrap();
    let xml = refs(&api).xml().send().await.unwrap();
    for page in [&json, &xml] {
        assert_eq!(page.hit_count, 5);
        let list = &page.reference_list.reference;
        assert_eq!(ids(list), EXPECTED[..2]);
        assert_eq!(
            list[0].key(),
            Some(RecordKey::new("MED", "31000001").unwrap())
        );
        assert_eq!(list[1].key(), None);
        assert!(list[1].source.is_none());
        assert_eq!(list[0].pub_year.as_deref(), Some("2001")); // a JSON number in the fixture
        assert_eq!(list[1].pub_year.as_deref(), Some("2002"));
        assert_eq!(list[0].journal_abbreviation.as_deref(), Some("Fixture J"));
        assert_eq!(
            list[1].title.as_deref(),
            Some("Synthetic unmatched reference 2")
        );
        let debug = format!("{page:?}");
        assert!(!debug.contains("Synthetic"), "{debug}");
    }
    server.shutdown().await;
}

#[tokio::test]
async fn reference_inputs_caps_sizes_and_byte_limits_follow_the_search_rules() {
    assert!(matches!(PageNumber::new(0), Err(Error::InvalidInput(_))));
    assert_eq!(PageNumber::FIRST.get(), 1);
    let server = fixed(200, "application/json", REFS_JSON[0]).await;
    let api = api(&server);
    assert!(matches!(
        refs(&api).pages(SearchFormat::Json, Resume::Start, cap(21)),
        Err(Error::InvalidInput(_))
    ));
    // Three records for a requested page size of two: refused by the ordinary send policy.
    let overfull = "{\"hitCount\":5,\"referenceList\":{\"reference\":[{},{},{}]}}";
    server.shutdown().await;
    let server = fixed(200, "application/json", overfull).await;
    assert!(matches!(
        refs(&support::api(&server)).json().send().await,
        Err(Error::InvalidPage(_))
    ));
    server.shutdown().await;
    for xml in [false, true] {
        let server = TestServer::start(move |_: &Recorded| -> Reply {
            Reply::new(200)
                .header(
                    "content-type",
                    if xml {
                        "application/xml"
                    } else {
                        "application/json"
                    },
                )
                .with_body(Body::Chunks(vec![(
                    Duration::ZERO,
                    Bytes::from(vec![b' '; BODY_LIMIT + 1]),
                )]))
        })
        .await;
        let api = support::api(&server);
        let error = if xml {
            refs(&api).xml().send().await
        } else {
            refs(&api).json().send().await
        }
        .unwrap_err();
        assert_eq!(error.call_error().unwrap().kind(), ErrorKind::Limit);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn malformed_reference_envelopes_are_decode_errors_not_empty_pages() {
    for body in [
        "{}",
        "null",
        "{\"hitCount\":\"5\",\"referenceList\":{\"reference\":[]}}",
        "{\"hitCount\":1,\"referenceList\":{\"reference\":[{\"id\":\"a/b\"}]}}",
        "{\"hitCount\":1,\"referenceList\":{\"reference\":[{\"source\":\"med\"}]}}",
        "{\"hitCount\":1,\"referenceList\":{\"reference\":[{\"pubYear\":-1}]}}",
        REFS_JSON[0].trim_end().trim_end_matches('}'),
    ] {
        let server = fixed(200, "application/json", body).await;
        let error = refs(&api(&server)).json().send().await.unwrap_err();
        assert_eq!(
            error.call_error().unwrap().kind(),
            ErrorKind::Decode,
            "{body}"
        );
        server.shutdown().await;
    }
    for body in [
        "",
        "<wrong/>",
        "<responseWrapper><referenceList/></responseWrapper>",
        "<responseWrapper><hitCount>-1</hitCount></responseWrapper>",
        "<responseWrapper><hitCount>1</hitCount><referenceList/><referenceList/></responseWrapper>",
        "<responseWrapper><hitCount>1</hitCount><referenceList><reference><id>a/b</id></reference></referenceList></responseWrapper>",
        "<responseWrapper><hitCount>1</hitCount><referenceList><reference><source></source></reference></referenceList></responseWrapper>",
        "<responseWrapper><hitCount>1</hitCount><referenceList><reference><title><b>x</b></title></reference></referenceList></responseWrapper>",
    ] {
        let server = fixed(200, "application/xml", body).await;
        let error = refs(&api(&server)).xml().send().await.unwrap_err();
        assert_eq!(
            error.call_error().unwrap().kind(),
            ErrorKind::Decode,
            "{body}"
        );
        server.shutdown().await;
    }
}

/// Page 1 is the authored first page; page 2 is `second`. Returns the traversal's error.
async fn second_page_fails(format: SearchFormat, second: &'static str) -> (usize, Error, usize) {
    let server = TestServer::start(move |r: &Recorded| -> Reply {
        let body = if page_of(r) == 1 {
            if format == SearchFormat::Json {
                REFS_JSON[0]
            } else {
                REFS_XML[0]
            }
        } else {
            second
        };
        let media = if format == SearchFormat::Json {
            "application/json"
        } else {
            "application/xml"
        };
        Reply::text(200, media, body)
    })
    .await;
    let api = api(&server);
    let incomplete = refs(&api)
        .pages(format, Resume::Start, cap(5))
        .unwrap()
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(
        incomplete.error.resume(),
        Some(&Resume::At(PageNumber::new(2).unwrap()))
    );
    assert_eq!(incomplete.error.outcome(), Outcome::Failed);
    let requests = server.requests().len();
    server.shutdown().await;
    let (error, _, _) = incomplete.error.into_provider_error().unwrap();
    (incomplete.collected.len(), error, requests)
}

#[tokio::test]
async fn pages_that_contradict_the_hit_count_fail_before_emission() {
    for (format, second) in [
        // A short page before the hit count: page 3 would start at record 4, so one is lost.
        (
            SearchFormat::Json,
            "{\"hitCount\":5,\"referenceList\":{\"reference\":[{\"title\":\"x\"}]}}",
        ),
        // An empty page before the hit count, with the list absent.
        (SearchFormat::Json, "{\"hitCount\":5}"),
        // The hit count changed: offsets no longer address the same list.
        (
            SearchFormat::Json,
            "{\"hitCount\":6,\"referenceList\":{\"reference\":[{},{}]}}",
        ),
        // The same short page in XML.
        (
            SearchFormat::Xml,
            "<responseWrapper><hitCount>5</hitCount><referenceList><reference/></referenceList></responseWrapper>",
        ),
    ] {
        let (collected, error, requests) = second_page_fails(format, second).await;
        assert_eq!(collected, 2, "{second}");
        assert!(
            matches!(error, Error::InvalidPage(_)),
            "{second}: {error:?}"
        );
        assert_eq!(requests, 2);
        redacted(&error);
    }
}

#[tokio::test]
async fn a_final_page_must_hold_exactly_the_remainder_and_zero_hits_end_at_once() {
    // Hit count 5 at page size 2: page 3 must contain exactly one record.
    for (third, ok) in [
        (
            "{\"hitCount\":5,\"referenceList\":{\"reference\":[]}}",
            false,
        ),
        (
            "{\"hitCount\":5,\"referenceList\":{\"reference\":[{},{}]}}",
            false,
        ),
        (
            "{\"hitCount\":5,\"referenceList\":{\"reference\":[{}]}}",
            true,
        ),
    ] {
        let server = TestServer::start(move |r: &Recorded| -> Reply {
            Reply::json(
                200,
                if page_of(r) == 3 {
                    third
                } else {
                    REFS_JSON[page_of(r) - 1]
                },
            )
        })
        .await;
        let api = api(&server);
        let result = refs(&api)
            .pages(SearchFormat::Json, Resume::Start, cap(5))
            .unwrap()
            .items()
            .collect_complete()
            .await;
        match result {
            Ok(items) => {
                assert!(ok, "{third}");
                assert_eq!(items.len(), 5);
            }
            Err(incomplete) => {
                assert!(!ok, "{third}");
                assert_eq!(incomplete.collected.len(), 4);
                assert!(matches!(
                    incomplete.error.provider_error(),
                    Some(Error::InvalidPage(_))
                ));
            }
        }
        assert_eq!(server.requests().len(), 3);
        server.shutdown().await;
    }
    // A publication without references: zero hits, with or without an empty list, in both formats.
    for (media, body) in [
        ("application/json", "{\"hitCount\":0}"),
        (
            "application/json",
            "{\"hitCount\":0,\"referenceList\":{\"reference\":[]}}",
        ),
        (
            "application/xml",
            "<responseWrapper><hitCount>0</hitCount></responseWrapper>",
        ),
        (
            "application/xml",
            "<responseWrapper><hitCount>0</hitCount><referenceList/></responseWrapper>",
        ),
    ] {
        let server = fixed(200, media, body).await;
        let format = if media.ends_with("json") {
            SearchFormat::Json
        } else {
            SearchFormat::Xml
        };
        let mut pages = refs(&api(&server))
            .pages(format, Resume::Start, cap(1))
            .unwrap();
        let page = pages.next_page().await.unwrap().unwrap();
        assert!(page.items.is_empty());
        assert_eq!(page.meta, 0);
        assert!(pages.next_page().await.unwrap().is_none());
        assert_eq!(pages.finish().outcome, Outcome::Exhausted);
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn json_search_selects_a_publication_then_reads_its_references_to_the_hit_count() {
    let server = TestServer::start(|r: &Recorded| -> Reply {
        assert_eq!(r.query_all("format"), ["json"]);
        match r.path() {
            "/rest/search" => Reply::json(
                200,
                if r.query_all("cursorMark") == ["*"] {
                    FIRST_JSON
                } else {
                    LAST_JSON
                },
            ),
            PATH => {
                assert_eq!(r.query_all("pageSize"), ["2"]);
                Reply::json(200, REFS_JSON[page_of(r) - 1])
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
    let selected = records[0].key();
    let mut pages = api
        .publication(selected.source, selected.id)
        .references(PageSize::new(2).unwrap())
        .pages(SearchFormat::Json, Resume::Start, cap(3))
        .unwrap();
    let mut all = Vec::new();
    while let Some(page) = pages.next_page().await.unwrap() {
        assert_eq!(page.meta, 5);
        all.extend(page.items);
    }
    assert_eq!(ids(&all), EXPECTED);
    assert_eq!(pages.finish().outcome, Outcome::Exhausted);
    let wire = server.requests();
    assert_eq!(wire.len(), 5);
    assert_eq!(wire[2..].iter().map(page_of).collect::<Vec<_>>(), [1, 2, 3]);
    server.shutdown().await;
}

#[tokio::test]
async fn the_same_reference_traversal_in_xml_yields_the_same_list() {
    let server = three_pages().await;
    let api = api(&server);
    let json = refs(&api)
        .pages(SearchFormat::Json, Resume::Start, cap(3))
        .unwrap()
        .items()
        .collect_complete()
        .await
        .unwrap();
    let xml = refs(&api)
        .pages(SearchFormat::Xml, Resume::Start, cap(3))
        .unwrap()
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(ids(&xml), EXPECTED);
    assert_eq!(ids(&json), ids(&xml));
    let keys =
        |v: &[concord_sdk_europe_pmc::Reference]| v.iter().map(|r| r.key()).collect::<Vec<_>>();
    assert_eq!(keys(&json), keys(&xml));
    let wire = server.requests();
    assert_eq!(wire.len(), 6);
    assert!(
        wire[3..]
            .iter()
            .all(|r| r.query_all("format") == ["xml"]
                && r.header("accept") == Some("application/xml"))
    );
    server.shutdown().await;
}

#[tokio::test]
async fn a_failed_later_page_keeps_completed_references_and_resumes_at_that_page() {
    let server = TestServer::start(|r: &Recorded| -> Reply {
        let page = page_of(r);
        if page == 3 {
            return Reply::new(503).header("retry-after", "1");
        }
        Reply::text(200, "application/xml", REFS_XML[page - 1])
    })
    .await;
    let api = api(&server);
    let incomplete = refs(&api)
        .pages(SearchFormat::Xml, Resume::Start, cap(5))
        .unwrap()
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(ids(&incomplete.collected), EXPECTED[..4]);
    let resume = incomplete.error.resume().unwrap().clone();
    assert_eq!(resume, Resume::At(PageNumber::new(3).unwrap()));
    let Some(Error::Http(http)) = incomplete.error.provider_error() else {
        panic!("expected the HTTP failure");
    };
    assert_eq!(http.meta.status().as_u16(), 503);
    assert_eq!(
        http.meta.operation().unwrap().name(),
        "EuropePmc.publication.references.xml"
    );
    assert_eq!(server.requests().len(), 3); // never retried by the SDK
    server.shutdown().await;

    // Resuming the same plan at the failed page requests only that page.
    let server = three_pages().await;
    let rest = refs(&support::api(&server))
        .pages(SearchFormat::Xml, resume, cap(5))
        .unwrap()
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(ids(&rest), EXPECTED[4..]);
    assert_eq!(
        server.requests().iter().map(page_of).collect::<Vec<_>>(),
        [3]
    );
    server.shutdown().await;
}

#[tokio::test]
async fn a_page_cap_before_the_hit_count_is_bounded_with_the_next_page_number() {
    let server = three_pages().await;
    let api = api(&server);
    let incomplete = refs(&api)
        .pages(SearchFormat::Json, Resume::Start, cap(2))
        .unwrap()
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected.len(), 4);
    assert!(matches!(incomplete.error, PagesError::Bounded { .. }));
    let resume = incomplete.error.resume().unwrap().clone();
    assert_eq!(resume, Resume::At(PageNumber::new(3).unwrap()));
    assert_eq!(server.requests().len(), 2);
    let mut rest = refs(&api)
        .pages(SearchFormat::Json, resume, cap(1))
        .unwrap();
    assert_eq!(
        ids(&rest.next_page().await.unwrap().unwrap().items),
        EXPECTED[4..]
    );
    assert!(rest.next_page().await.unwrap().is_none()); // the last page needs no extra budget
    assert_eq!(server.requests().len(), 3);
    // Resuming past the end of the list is a contradiction, not an empty success.
    let mut past = refs(&api)
        .pages(
            SearchFormat::Json,
            Resume::At(PageNumber::new(4).unwrap()),
            cap(1),
        )
        .unwrap();
    assert!(matches!(
        past.next_page().await.unwrap_err().provider_error(),
        Some(Error::InvalidPage(_))
    ));
    server.shutdown().await;
}
