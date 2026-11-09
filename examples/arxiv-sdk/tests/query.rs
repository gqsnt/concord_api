//! Typed `search_query` construction and sorting (manual §§3.1.1.3 and 5.1, read 2026-11-07).
#[allow(dead_code)] // the shared fixtures include the ID-list feed, unused here
mod common;

use common::*;
use concord_sdk_arxiv::concord_pagination::{PagesError, Resume};
use concord_sdk_arxiv::{
    Arxiv, ArxivConfig, Error, Field, PageSize, Paging, Policy, Query, SearchTerm, SortBy,
    SortOrder, http_client,
};
use concord_testserver::{Recorded, Reply, TestServer};
use std::{num::NonZeroUsize, time::Duration};

fn term(field: Field, phrase: &str) -> Query {
    Query::term(field, phrase).unwrap()
}

/// A synthetic §3.3 page: `size` invented entries from `start`, out of `total`.
fn synthetic_page(total: u64, start: u64, size: u64) -> String {
    let entries: String = (start..(start + size).min(total))
        .map(|i| {
            format!(
                "<entry><id>http://arxiv.org/abs/2401.{i:05}v1</id><title>Synthetic {i}</title>\
                 <summary>Invented.</summary><published>2024-01-01T00:00:00Z</published>\
                 <updated>2024-01-01T00:00:00Z</updated></entry>"
            )
        })
        .collect();
    format!(
        "<!-- SYNTHETIC --><feed xmlns=\"http://www.w3.org/2005/Atom\" \
         xmlns:o=\"http://a9.com/-/spec/opensearch/1.1/\"><o:totalResults>{total}</o:totalResults>\
         <o:startIndex>{start}</o:startIndex><o:itemsPerPage>{size}</o:itemsPerPage>\
         {entries}</feed>"
    )
}

/// The ordinary caller path: default transport and the default three-second policy.
fn caller(server: &TestServer) -> Arxiv {
    Arxiv::with_http(http_client().unwrap())
        .with_base_url(format!("{}/api", server.url()))
        .unwrap()
}

/// Generic SDK-side code over the shared result window.
fn ten_from<E: Paging>(request: E, start: u32) -> E {
    request
        .set_start(start)
        .set_size(PageSize::new(10).unwrap())
}

#[test]
fn query_syntax_uses_documented_prefixes_operators_quotes_and_explicit_grouping() {
    // §5.1 prefix table, in its order.
    let prefixes = [
        (Field::Title, "ti"),
        (Field::Author, "au"),
        (Field::Abstract, "abs"),
        (Field::Comment, "co"),
        (Field::JournalRef, "jr"),
        (Field::Category, "cat"),
        (Field::ReportNumber, "rn"),
        (Field::Id, "id"),
        (Field::All, "all"),
    ];
    for (field, prefix) in prefixes {
        assert_eq!(term(field, "x y").to_string(), format!("{prefix}:\"x y\""));
    }
    // §5.1's ANDNOT-with-grouping example, with every term quoted as a phrase.
    assert_eq!(
        term(Field::Author, "del_maestro")
            .and_not(term(Field::Title, "checkerboard").or(term(Field::Title, "Pyrochlore")))
            .to_string(),
        "au:\"del_maestro\" ANDNOT (ti:\"checkerboard\" OR ti:\"Pyrochlore\")"
    );
    // Grouping follows construction, on either side.
    let a_or_b = term(Field::Abstract, "a").or(term(Field::Abstract, "b"));
    assert_eq!(
        a_or_b
            .clone()
            .and(term(Field::Category, "hep-th"))
            .to_string(),
        "(abs:\"a\" OR abs:\"b\") AND cat:\"hep-th\""
    );
    assert_eq!(
        a_or_b
            .clone()
            .and_not(term(Field::Comment, "c").and(term(Field::JournalRef, "d")))
            .to_string(),
        "(abs:\"a\" OR abs:\"b\") ANDNOT (co:\"c\" AND jr:\"d\")"
    );
    // A chain nests to the left; nothing is flattened or reassociated.
    assert_eq!(
        a_or_b.or(term(Field::All, "c")).to_string(),
        "(abs:\"a\" OR abs:\"b\") OR all:\"c\""
    );
    // Operator words, colons, parentheses and brackets stay phrase text inside the quotes.
    assert_eq!(
        term(Field::Title, "x) OR (y ANDNOT cat:z [1 TO 2]").to_string(),
        "ti:\"x) OR (y ANDNOT cat:z [1 TO 2]\""
    );
    // Text that could end or escape the phrase, blank and oversized text are refused.
    for bad in [
        "",
        "   ",
        "say \"hi\"",
        "back\\slash",
        "tab\tseparated",
        "line\nbreak",
    ] {
        assert!(
            matches!(Query::term(Field::Title, bad), Err(Error::Input(_))),
            "{bad:?}"
        );
    }
    assert!(matches!(
        Query::term(Field::All, "é".repeat(257)),
        Err(Error::Input(_))
    ));
    assert!(Query::term(Field::All, "x".repeat(512)).is_ok());
    // The phrase rule is shared with term search: same accept/refuse decision.
    for text in ["x", " ", "a\"b", "a\\b", "a\u{7f}b", &"y".repeat(513)] {
        assert_eq!(
            Query::term(Field::All, text).is_ok(),
            SearchTerm::new(text).is_ok()
        );
    }
    assert_eq!(
        term(Field::All, "quantum criticality").to_string(),
        SearchTerm::new("quantum criticality").unwrap().to_string()
    );
    // §3.1.1.3 spellings.
    assert_eq!(
        [
            SortBy::Relevance,
            SortBy::LastUpdatedDate,
            SortBy::SubmittedDate
        ]
        .map(|s| s.to_string()),
        ["relevance", "lastUpdatedDate", "submittedDate"]
    );
    assert_eq!(
        [SortOrder::Ascending, SortOrder::Descending].map(|s| s.to_string()),
        ["ascending", "descending"]
    );
    assert_eq!(
        format!("{:?}", term(Field::Author, "private-sentinel")),
        "Query { .. }"
    );
}

#[tokio::test]
async fn query_wire_matches_the_manual_and_omits_unset_sort_keys() {
    let server = TestServer::start(|request: &Recorded| -> Reply {
        assert_eq!(request.path(), "/api/query");
        assert!(request.query_all("id_list").is_empty());
        // An empty page that echoes the requested window, as the policy requires.
        let number = |key| request.query_all(key)[0].parse::<u64>().unwrap();
        let body = synthetic_page(0, number("start"), number("max_results"));
        Reply::text(200, "application/atom+xml", body)
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
    let papers = api.papers();
    let grouped = term(Field::Author, "del_maestro")
        .and_not(term(Field::Title, "checkerboard").or(term(Field::Title, "Pyrochlore")));
    papers.query(grouped).send().await.unwrap();
    papers
        .query(term(Field::Title, "electron thermal conductivity"))
        .set_sort_by(SortBy::LastUpdatedDate)
        .set_sort_order(SortOrder::Ascending)
        .send()
        .await
        .unwrap();
    papers
        .query(term(Field::Author, "del_maestro").and(term(Field::Title, "quantum criticality")))
        .set_sort_order(SortOrder::Descending)
        .set_size(PageSize::new(100).unwrap())
        .set_start(200)
        .send()
        .await
        .unwrap();
    ten_from(papers.query(term(Field::Category, "cs.AI")), 20)
        .set_sort_by(SortBy::SubmittedDate)
        .send()
        .await
        .unwrap();
    // Term search is unchanged and still accepts the same generic window code.
    ten_from(papers.search(SearchTerm::new("electron").unwrap()), 20)
        .send()
        .await
        .unwrap();
    let targets: Vec<_> = server.requests().into_iter().map(|r| r.target).collect();
    assert_eq!(
        targets,
        [
            // %22 quotes, %28/%29 parentheses and + spaces, as in manual §5.1.
            "/api/query?search_query=au%3A%22del_maestro%22+ANDNOT+%28ti%3A%22checkerboard%22+OR+ti%3A%22Pyrochlore%22%29&start=0&max_results=10",
            "/api/query?search_query=ti%3A%22electron+thermal+conductivity%22&start=0&max_results=10&sortBy=lastUpdatedDate&sortOrder=ascending",
            "/api/query?search_query=au%3A%22del_maestro%22+AND+ti%3A%22quantum+criticality%22&start=200&max_results=100&sortOrder=descending",
            "/api/query?search_query=cat%3A%22cs.AI%22&start=20&max_results=10&sortBy=submittedDate",
            "/api/query?search_query=all%3A%22electron%22&start=20&max_results=10",
        ]
    );
    // One shared policy: four waits between five dispatches.
    assert_eq!(clock.waits(), [Duration::from_secs(3); 4]);
    server.shutdown().await;
}

#[tokio::test]
async fn sorted_query_pages_inspects_entries_and_resumes_like_term_search() {
    let expected_query = "(cat:\"quant-ph\" OR cat:\"hep-ex\") ANDNOT ti:\"retracted\"".to_owned();
    let server = TestServer::start(move |request: &Recorded| -> Reply {
        assert_eq!(request.query_all("search_query"), [expected_query.as_str()]);
        assert_eq!(request.query_all("sortBy"), ["submittedDate"]);
        assert_eq!(request.query_all("sortOrder"), ["descending"]);
        assert_eq!(request.query_all("max_results"), ["1"]);
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
    let request = api
        .papers()
        .query(
            term(Field::Category, "quant-ph")
                .or(term(Field::Category, "hep-ex"))
                .and_not(term(Field::Title, "retracted")),
        )
        .set_sort_by(SortBy::SubmittedDate)
        .set_sort_order(SortOrder::Descending)
        .set_size(PageSize::new(1).unwrap());
    assert!(!format!("{request:?}").contains("retracted"));
    assert!(!format!("{:?}", request.clone().prepare().unwrap()).contains("retracted"));

    // Whole traversal: extension elements and absent optional fields do not invalidate pages.
    let mut pages = request
        .clone()
        .pages(Resume::Start, NonZeroUsize::new(3).unwrap());
    let first = pages.next_page().await.unwrap().unwrap();
    assert_eq!(first.next, Some(1));
    assert_eq!(first.items[0].title, "Synthetic electron & photon study");
    assert_eq!(first.items[0].doi.as_deref(), Some("10.0000/synthetic"));
    assert_eq!(first.items[0].authors[0].affiliations.len(), 2);
    let second = pages.next_page().await.unwrap().unwrap();
    assert_eq!(second.items[0].id, "http://arxiv.org/abs/hep-ex/0307015v1");
    assert!(second.items[0].doi.is_none() && second.items[0].primary_category.is_none());
    assert!(pages.next_page().await.unwrap().is_none());
    assert_eq!(clock.waits(), [Duration::from_secs(3)]);

    // A budget of one page reports the next offset; resuming continues the same query and sort.
    let incomplete = request
        .clone()
        .pages(Resume::Start, NonZeroUsize::new(1).unwrap())
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected.len(), 1);
    let PagesError::Bounded { resume, .. } = incomplete.error else {
        panic!("{:?}", incomplete.error)
    };
    assert_eq!(resume, Resume::At(1));
    let rest = request
        .clone()
        .pages(resume, NonZeroUsize::new(1).unwrap())
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(rest[0].title, "Synthetic legacy paper");

    // `Resume::Start` keeps the request's own `set_start`, as for term search.
    let from_one = request
        .set_start(1)
        .pages(Resume::Start, NonZeroUsize::new(1).unwrap())
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(from_one.len(), 1);
    let starts: Vec<_> = server
        .requests()
        .iter()
        .map(|r| r.query_all("start").join(","))
        .collect();
    assert_eq!(starts, ["0", "1", "0", "1", "1"]);
    server.shutdown().await;
}

#[tokio::test]
async fn query_window_and_provider_failures_match_term_search() {
    let server = TestServer::start(|request: &Recorded| -> Reply {
        let start: u64 = request.query_all("start")[0].parse().unwrap();
        Reply::text(
            200,
            "application/atom+xml",
            synthetic_page(40_000, start, 10),
        )
    })
    .await;
    let api = caller(&server);
    let query = || term(Field::Abstract, "electron");
    // A window ending beyond 30,000 is refused before admission or I/O.
    for start in [29_991, 30_000, u32::MAX] {
        let error = api
            .papers()
            .query(query())
            .set_sort_by(SortBy::Relevance)
            .set_start(start)
            .set_size(PageSize::new(10).unwrap())
            .send()
            .await
            .unwrap_err();
        assert!(matches!(error, Error::Input(_)), "{error:?}");
    }
    assert!(server.requests().is_empty());
    // Traversal reads the page ending at 30,000 and refuses the next one locally.
    let mut pages = api
        .papers()
        .query(query())
        .set_size(PageSize::new(10).unwrap())
        .pages(Resume::At(29_990), NonZeroUsize::new(3).unwrap());
    let last = pages.next_page().await.unwrap().unwrap();
    assert_eq!((last.items.len(), last.next), (10, Some(30_000)));
    let error = pages.next_page().await.unwrap_err();
    assert!(
        matches!(error.provider_error(), Some(Error::Input(_))),
        "{error:?}"
    );
    assert_eq!(error.resume(), Some(&Resume::At(30_000)));
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;

    // A §3.4 error entry under HTTP 200 and a non-2xx status keep their term-search meaning.
    for (status, document) in [(200, ERROR), (400, ERROR)] {
        let server = TestServer::start(move |_: &Recorded| -> Reply {
            Reply::text(status, "application/atom+xml", document)
        })
        .await;
        let error = caller(&server)
            .papers()
            .query(query().and(term(Field::Author, "private-sentinel")))
            .send()
            .await
            .unwrap_err();
        assert!(!format!("{error:?} {error}").contains("private-sentinel"));
        match (status, error) {
            (200, Error::Api(failure)) => assert!(!failure.problem.summary.is_empty()),
            (400, Error::Http(failure)) => assert_eq!(failure.meta.status().as_u16(), 400),
            (_, error) => panic!("{error:?}"),
        }
        assert_eq!(server.requests().len(), 1, "no retry");
        server.shutdown().await;
    }
}
