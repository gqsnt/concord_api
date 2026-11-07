//! Fresh-consumer application for the arXiv SDK (see FIRST_USE.md).
//!
//! Two caller tasks against loopback fixtures (live calls: NOT_RUN):
//! 1. search papers by term, inspect an entry, page through the results;
//! 2. fetch a known list of arXiv IDs and read their metadata.
//!
//! Fixtures are written from the arXiv API user manual (§3.3, §3.4; read 2026-11-07),
//! not copied from the SDK's own fixtures. Paper titles and abstracts are synthetic.

use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use concord_sdk_arxiv::concord::ErrorKind;
use concord_sdk_arxiv::concord_pagination::{PagesError, Resume};
use concord_sdk_arxiv::{Arxiv, Error, Feed, IdList, PageSize, Paper, SearchTerm, http_client};
use concord_testserver::{Body, Recorded, Reply, TestServer};

const ATOM: &str = "http://www.w3.org/2005/Atom";
const OPENSEARCH: &str = "http://a9.com/-/spec/opensearch/1.1/";
const ARXIV: &str = "http://arxiv.org/schemas/atom";

// ---------------------------------------------------------------- fixtures

/// A result feed in the manual's §3.3 shape, with caller-chosen prefixes for the
/// OpenSearch and arXiv namespaces (`os`/`ax` exercise prefix independence).
fn feed_with(
    os: &str,
    ax: &str,
    total: u64,
    start: u64,
    per_page: u64,
    entries: &[String],
) -> String {
    let mut xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="{ATOM}" xmlns:{os}="{OPENSEARCH}" xmlns:{ax}="{ARXIV}">
  <link href="http://arxiv.org/api/query?search_query%3D%26id_list%3D%26start%3D{start}%26max_results%3D{per_page}" rel="self" type="application/atom+xml"/>
  <title type="html">ArXiv Query: search_query=&amp;id_list=&amp;start={start}&amp;max_results={per_page}</title>
  <id>http://arxiv.org/api/9hQ1yQ3s0dLzUNkVx4cZrmqHnXg</id>
  <updated>2026-11-07T00:00:00-05:00</updated>
  <{os}:totalResults>{total}</{os}:totalResults>
  <{os}:startIndex>{start}</{os}:startIndex>
  <{os}:itemsPerPage>{per_page}</{os}:itemsPerPage>
"#
    );
    for entry in entries {
        xml.push_str(&entry.replace("arxiv:", &format!("{ax}:")));
    }
    xml.push_str("</feed>\n");
    xml
}

fn feed(total: u64, start: u64, per_page: u64, entries: &[String]) -> String {
    feed_with("opensearch", "arxiv", total, start, per_page, entries)
}

/// Every optional field present, two affiliations, a DOI link, a multi-line title.
fn full_entry() -> String {
    r#"  <entry>
    <id>http://arxiv.org/abs/2101.00001v2</id>
    <updated>2021-03-04T18:00:00Z</updated>
    <published>2021-01-01T00:00:01Z</published>
    <title>Quantum criticality in a
      two-dimensional electron gas</title>
    <summary>  We study quantum criticality near a metal-insulator transition.
    </summary>
    <author>
      <name>Ada Example</name>
      <arxiv:affiliation>University of Somewhere</arxiv:affiliation>
      <arxiv:affiliation>Institute for Examples</arxiv:affiliation>
    </author>
    <author><name>Bo Sample</name></author>
    <arxiv:doi>10.1103/PhysRevB.000.000001</arxiv:doi>
    <link title="doi" href="http://dx.doi.org/10.1103/PhysRevB.000.000001" rel="related"/>
    <arxiv:comment>12 pages, 4 figures</arxiv:comment>
    <arxiv:journal_ref>Phys. Rev. B 000, 000001 (2021)</arxiv:journal_ref>
    <link href="http://arxiv.org/abs/2101.00001v2" rel="alternate" type="text/html"/>
    <link title="pdf" href="http://arxiv.org/pdf/2101.00001v2" rel="related" type="application/pdf"/>
    <arxiv:primary_category term="cond-mat.str-el" scheme="http://arxiv.org/schemas/atom"/>
    <category term="cond-mat.str-el" scheme="http://arxiv.org/schemas/atom"/>
    <category term="quant-ph" scheme="http://arxiv.org/schemas/atom"/>
  </entry>
"#
    .to_owned()
}

/// No optional arXiv field, a bare link and category, and foreign extension elements —
/// including ones named `entry` and `title` — that must be ignored.
fn sparse_entry() -> String {
    r#"  <entry>
    <id>http://arxiv.org/abs/hep-ex/0307015</id>
    <updated>2003-07-07T13:46:39-04:00</updated>
    <published>2003-07-07T13:46:39-04:00</published>
    <title>Multi-Electron Production at High Transverse Momenta in ep Collisions at HERA</title>
    <summary>Multi-electron production is studied at high electron transverse momentum.</summary>
    <author><name>H1 Collaboration</name></author>
    <link href="http://arxiv.org/abs/hep-ex/0307015"/>
    <category term="hep-ex"/>
    <x:entry xmlns:x="urn:example:mirror"><x:title>not a paper</x:title></x:entry>
    <x:title xmlns:x="urn:example:mirror">shadow title</x:title>
    <media:thumbnail xmlns:media="http://search.yahoo.com/mrss/" url="http://example.invalid/t.png"/>
  </entry>
"#
    .to_owned()
}

fn plain_entry(id: &str, title: &str) -> String {
    format!(
        r#"  <entry>
    <id>http://arxiv.org/abs/{id}</id>
    <updated>2023-10-10T17:59:59Z</updated>
    <published>2023-10-10T17:59:59Z</published>
    <title>{title}</title>
    <summary>Synthetic abstract for {id}.</summary>
    <author><name>C. Writer</name></author>
    <link href="http://arxiv.org/abs/{id}" rel="alternate" type="text/html"/>
    <link title="pdf" href="http://arxiv.org/pdf/{id}" rel="related" type="application/pdf"/>
    <arxiv:primary_category term="cond-mat.str-el" scheme="http://arxiv.org/schemas/atom"/>
    <category term="cond-mat.str-el" scheme="http://arxiv.org/schemas/atom"/>
  </entry>
"#
    )
}

/// Five search results: one full, one sparse with extensions, three plain.
fn corpus(i: u64) -> String {
    match i {
        0 => full_entry(),
        1 => sparse_entry(),
        n => plain_entry(
            &format!("2310.0682{n}v1"),
            &format!("Quantum criticality, part {n}"),
        ),
    }
}

/// The manual's §3.4 error feed for a malformed identifier.
fn error_feed(slug: &str, message: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="{ATOM}" xmlns:opensearch="{OPENSEARCH}">
  <link href="http://arxiv.org/api/query?search_query=&amp;id_list=1234.12345" rel="self" type="application/atom+xml"/>
  <title type="html">ArXiv Query: search_query=&amp;id_list=1234.12345</title>
  <id>http://arxiv.org/api/kvuntZ8c9a4Eq5CF7KY03nMug+Q</id>
  <updated>2007-10-12T00:00:00-04:00</updated>
  <opensearch:totalResults>1</opensearch:totalResults>
  <opensearch:startIndex>0</opensearch:startIndex>
  <opensearch:itemsPerPage>1</opensearch:itemsPerPage>
  <entry>
    <id>http://arxiv.org/api/errors#{slug}</id>
    <title>Error</title>
    <summary>{message}</summary>
    <updated>2007-10-12T00:00:00-04:00</updated>
    <link href="http://arxiv.org/api/errors#{slug}" rel="alternate" type="text/html"/>
    <author><name>arXiv api core</name></author>
  </entry>
</feed>
"#
    )
}

fn atom(status: u16, body: impl AsRef<[u8]>) -> Reply {
    Reply::text(status, "application/atom+xml", body)
}

/// Serves `total` corpus entries, paged by the request's own `start`/`max_results`, and
/// records when each request arrived.
fn paged_search(
    total: u64,
    arrivals: Arc<Mutex<Vec<Instant>>>,
) -> impl Fn(&Recorded) -> Reply + Send + Sync + 'static {
    move |r: &Recorded| {
        arrivals.lock().unwrap().push(Instant::now());
        let start: u64 = r.query_all("start")[0].parse().unwrap();
        let size: u64 = r.query_all("max_results")[0].parse().unwrap();
        let entries: Vec<String> = (start..(start + size).min(total)).map(corpus).collect();
        atom(200, feed(total, start, size, &entries))
    }
}

fn client(server: &TestServer) -> Arxiv {
    Arxiv::with_http(http_client().expect("transport"))
        .with_base_url(format!("{}/api", server.url()))
        .expect("loopback base URL")
}

fn starts(server: &TestServer) -> Vec<String> {
    server
        .requests()
        .iter()
        .map(|r| r.query_all("start").join(","))
        .collect()
}

fn concord_kind(error: &Error) -> Option<ErrorKind> {
    match error {
        Error::Concord(e) => Some(e.kind()),
        _ => None,
    }
}

fn assert_ordinary_get(r: &Recorded) {
    assert_eq!(r.method, "GET");
    assert_eq!(r.path(), "/api/query");
    assert_eq!(r.header("accept"), Some("application/atom+xml"));
    assert!(r.header_all("authorization").is_empty());
    assert!(r.header_all("cookie").is_empty());
    assert!(r.body.is_empty());
}

fn abs_id(paper: &Paper) -> &str {
    paper
        .id
        .strip_prefix("http://arxiv.org/abs/")
        .expect("abstract URI")
}

// ---------------------------------------------------------------- workflow 1: search

#[tokio::test]
async fn search_inspect_entry_then_page_to_the_end() {
    let arrivals = Arc::new(Mutex::new(Vec::new()));
    let server = TestServer::start(paged_search(5, arrivals.clone())).await;
    let api = client(&server);
    let papers = api.papers();
    let term = SearchTerm::new("quantum criticality").unwrap();
    let size = PageSize::new(2).unwrap();

    // One page.
    let feed: Feed = papers
        .search(term.clone())
        .set_size(size)
        .send()
        .await
        .unwrap();
    let r = server.single();
    assert_ordinary_get(&r);
    assert_eq!(
        r.query(),
        Some("search_query=all%3A%22quantum+criticality%22&start=0&max_results=2")
    );
    assert_eq!(
        r.query_pairs(),
        [
            (
                "search_query".to_owned(),
                "all:\"quantum criticality\"".to_owned()
            ),
            ("start".to_owned(), "0".to_owned()),
            ("max_results".to_owned(), "2".to_owned()),
        ]
    );
    assert_eq!(
        (feed.total_results, feed.start, feed.items_per_page),
        (5, 0, 2)
    );
    assert!(feed.problem.is_none());
    assert_eq!(feed.entries.len(), 2);

    // Inspect the first entry: every optional field.
    let p = &feed.entries[0];
    assert_eq!(p.id, "http://arxiv.org/abs/2101.00001v2");
    assert_eq!(abs_id(p), "2101.00001v2");
    assert_eq!(
        p.title,
        "Quantum criticality in a\n      two-dimensional electron gas"
    );
    assert_eq!(
        p.summary,
        "We study quantum criticality near a metal-insulator transition."
    );
    assert_eq!(
        (p.published.as_str(), p.updated.as_str()),
        ("2021-01-01T00:00:01Z", "2021-03-04T18:00:00Z")
    );
    let names: Vec<&str> = p.authors.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, ["Ada Example", "Bo Sample"]);
    assert_eq!(
        p.authors[0].affiliations,
        ["University of Somewhere", "Institute for Examples"]
    );
    assert!(p.authors[1].affiliations.is_empty());
    let terms: Vec<&str> = p.categories.iter().map(|c| c.term.as_str()).collect();
    assert_eq!(terms, ["cond-mat.str-el", "quant-ph"]);
    assert_eq!(
        p.primary_category.as_ref().map(|c| c.term.as_str()),
        Some("cond-mat.str-el")
    );
    assert_eq!(p.comment.as_deref(), Some("12 pages, 4 figures"));
    assert_eq!(
        p.journal_ref.as_deref(),
        Some("Phys. Rev. B 000, 000001 (2021)")
    );
    assert_eq!(p.doi.as_deref(), Some("10.1103/PhysRevB.000.000001"));
    let pdf = p
        .links
        .iter()
        .find(|l| l.title.as_deref() == Some("pdf"))
        .expect("pdf link");
    assert_eq!(pdf.href, "http://arxiv.org/pdf/2101.00001v2");
    assert_eq!(pdf.media_type.as_deref(), Some("application/pdf"));
    let doi = p
        .links
        .iter()
        .find(|l| l.title.as_deref() == Some("doi"))
        .expect("doi link");
    assert_eq!(
        (doi.rel.as_deref(), doi.media_type.as_deref()),
        (Some("related"), None)
    );

    // The second entry has no optional fields and carries foreign extensions.
    let s = &feed.entries[1];
    assert_eq!(abs_id(s), "hep-ex/0307015");
    assert_eq!(
        s.title,
        "Multi-Electron Production at High Transverse Momenta in ep Collisions at HERA"
    );
    assert!(
        s.primary_category.is_none()
            && s.comment.is_none()
            && s.journal_ref.is_none()
            && s.doi.is_none()
    );
    assert!(s.authors[0].affiliations.is_empty());
    assert_eq!(s.categories.len(), 1);
    assert_eq!(s.categories[0].scheme, None);
    assert_eq!(s.links.len(), 1);
    assert_eq!(
        (s.links[0].rel.as_deref(), s.links[0].title.as_deref()),
        (None, None)
    );

    // Page through every result with a three-page budget; the end is explicit.
    let mut pages = papers
        .search(term)
        .set_size(size)
        .pages(Resume::Start, NonZeroUsize::new(3).unwrap());
    let mut seen = Vec::new();
    let mut nexts = Vec::new();
    while let Some(page) = pages.next_page().await.unwrap() {
        nexts.push(page.next);
        seen.extend(page.items.into_iter().map(|p| abs_id(&p).to_owned()));
    }
    assert_eq!(nexts, [Some(2), Some(4), None]);
    assert_eq!(
        seen,
        [
            "2101.00001v2",
            "hep-ex/0307015",
            "2310.06822v1",
            "2310.06823v1",
            "2310.06824v1"
        ]
    );
    assert!(
        pages.next_page().await.unwrap().is_none(),
        "the end stays the end"
    );
    assert_eq!(starts(&server), ["0", "0", "2", "4"]);
    for r in server.requests() {
        assert_ordinary_get(&r);
        assert_eq!(r.query_all("max_results"), ["2"]);
    }

    // One client shares one pacing policy: admissions are three seconds apart.
    let arrivals = arrivals.lock().unwrap().clone();
    for pair in arrivals.windows(2) {
        let gap = pair[1] - pair[0];
        assert!(gap >= Duration::from_millis(2_900), "gap {gap:?}");
    }
}

#[tokio::test]
async fn search_budget_is_resumable_from_the_reported_offset() {
    let arrivals = Arc::new(Mutex::new(Vec::new()));
    let server = TestServer::start(paged_search(5, arrivals)).await;
    let api = client(&server);
    let term = SearchTerm::new("electron").unwrap();
    let size = PageSize::new(2).unwrap();

    let mut pages = api
        .papers()
        .search(term.clone())
        .set_size(size)
        .pages(Resume::Start, NonZeroUsize::new(2).unwrap());
    assert_eq!(pages.next_page().await.unwrap().unwrap().items.len(), 2);
    assert_eq!(pages.next_page().await.unwrap().unwrap().items.len(), 2);
    let resume = match pages.next_page().await {
        Err(PagesError::Bounded { resume, .. }) => resume,
        other => panic!("expected a bounded traversal, got {other:?}"),
    };
    assert_eq!(resume, Resume::At(4));
    assert!(matches!(
        pages.next_page().await,
        Err(PagesError::Ended { .. })
    ));
    assert_eq!(starts(&server), ["0", "2"], "the budget is not exceeded");

    // Continue later from the reported offset, same term and page size.
    let rest = api
        .papers()
        .search(term)
        .set_size(size)
        .pages(resume, NonZeroUsize::new(1).unwrap())
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(
        rest.iter().map(abs_id).collect::<Vec<_>>(),
        ["2310.06824v1"]
    );
    assert_eq!(starts(&server), ["0", "2", "4"]);
}

#[tokio::test]
async fn search_failure_mid_traversal_keeps_the_collected_prefix() {
    let server = TestServer::start(|r: &Recorded| {
        if r.query_all("start") == ["2"] {
            Reply::text(
                503,
                "text/html",
                "<html><body>Service temporarily unavailable</body></html>",
            )
            .header("retry-after", "10")
        } else {
            atom(200, feed(5, 0, 2, &[corpus(0), corpus(1)]))
        }
    })
    .await;
    let api = client(&server);
    let incomplete = api
        .papers()
        .search(SearchTerm::new("electron").unwrap())
        .set_size(PageSize::new(2).unwrap())
        .pages(Resume::Start, NonZeroUsize::new(3).unwrap())
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(
        incomplete.collected.iter().map(abs_id).collect::<Vec<_>>(),
        ["2101.00001v2", "hep-ex/0307015"]
    );
    assert_eq!(incomplete.error.resume(), Some(&Resume::At(2)));
    let (error, failed, _) = incomplete
        .error
        .into_provider_error()
        .expect("a provider failure");
    assert_eq!(failed, Resume::At(2));
    let Error::Http(http) = error else {
        panic!("expected an HTTP failure, got {error:?}")
    };
    assert_eq!(http.meta.status().as_u16(), 503);
    assert_eq!(
        http.meta
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok()),
        Some("10")
    );
    let document = http
        .document
        .as_ref()
        .expect_err("HTML is not an Atom document");
    assert_eq!(document.kind(), ErrorKind::Metadata);
    assert_eq!(starts(&server), ["0", "2"], "no retry of the failed page");
}

#[tokio::test]
async fn search_paging_metadata_is_checked() {
    // A short page continues when the total says more records remain.
    let server = TestServer::start(|r: &Recorded| match r.query_all("start")[0].as_str() {
        "0" => atom(200, feed(5, 0, 2, &[corpus(0)])),
        start => {
            let start: u64 = start.parse().unwrap();
            let entries: Vec<String> = (start..(start + 2).min(5)).map(corpus).collect();
            atom(200, feed(5, start, 2, &entries))
        }
    })
    .await;
    let mut pages = client(&server)
        .papers()
        .search(SearchTerm::new("electron").unwrap())
        .set_size(PageSize::new(2).unwrap())
        .pages(Resume::Start, NonZeroUsize::new(4).unwrap());
    let first = pages.next_page().await.unwrap().unwrap();
    assert_eq!(first.items.len(), 1);
    assert_eq!(
        first.next,
        Some(1),
        "continues after the entries actually received"
    );
    let mut seen: Vec<String> = first.items.iter().map(|p| abs_id(p).to_owned()).collect();
    while let Some(page) = pages.next_page().await.unwrap() {
        seen.extend(page.items.iter().map(|p| abs_id(p).to_owned()));
    }
    assert_eq!(
        seen,
        [
            "2101.00001v2",
            "hep-ex/0307015",
            "2310.06822v1",
            "2310.06823v1",
            "2310.06824v1"
        ],
        "no gap and no duplicate after a short page"
    );
    assert_eq!(starts(&server), ["0", "1", "3"]);

    // An empty page while the total says more remain does not advance: explicit error.
    let server = TestServer::start(|_r: &Recorded| atom(200, feed(5, 0, 2, &[]))).await;
    let error = client(&server)
        .papers()
        .search(SearchTerm::new("electron").unwrap())
        .set_size(PageSize::new(2).unwrap())
        .pages(Resume::Start, NonZeroUsize::new(3).unwrap())
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert!(error.collected.is_empty());
    assert!(
        matches!(error.error.provider_error(), Some(Error::Pagination(_))),
        "{:?}",
        error.error
    );

    // The provider echoing a different offset or page size than requested is refused.
    for echoed in [
        feed(5, 2, 2, &[corpus(2), corpus(3)]),
        feed(5, 0, 10, &[corpus(0), corpus(1)]),
    ] {
        let server = TestServer::start(move |_r: &Recorded| atom(200, echoed.clone())).await;
        let error = client(&server)
            .papers()
            .search(SearchTerm::new("electron").unwrap())
            .set_size(PageSize::new(2).unwrap())
            .send()
            .await
            .unwrap_err();
        assert!(matches!(error, Error::Pagination(_)), "{error:?}");
    }

    // `Resume::Start` means the request's own `set_start`; `Resume::At` overrides it.
    let server = TestServer::start(paged_search(5, Arc::default())).await;
    let api = client(&server);
    let from_three = api
        .papers()
        .search(SearchTerm::new("electron").unwrap())
        .set_start(3)
        .set_size(PageSize::new(2).unwrap())
        .pages(Resume::Start, NonZeroUsize::new(1).unwrap())
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(
        from_three.iter().map(abs_id).collect::<Vec<_>>(),
        ["2310.06823v1", "2310.06824v1"]
    );
    assert_eq!(starts(&server), ["3"]);
}

#[tokio::test]
async fn search_traversal_stops_at_the_30000_window() {
    // 40,000 matches: the page ending at 30,000 is read, the next is refused locally.
    let server = TestServer::start(|r: &Recorded| {
        let start: u64 = r.query_all("start")[0].parse().unwrap();
        let page: Vec<String> = (0..10)
            .map(|i| plain_entry(&format!("2401.{i:05}v1"), "Tail result"))
            .collect();
        atom(200, feed(40_000, start, 10, &page))
    })
    .await;
    let mut pages = client(&server)
        .papers()
        .search(SearchTerm::new("electron").unwrap())
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
    assert_eq!(
        starts(&server),
        ["29990"],
        "the out-of-window page is never sent"
    );
}

#[tokio::test]
async fn search_refuses_bad_input_before_dispatch() {
    let server = TestServer::start(|_r: &Recorded| atom(200, feed(0, 0, 10, &[]))).await;
    let api = client(&server);
    for bad in ["", "   ", "say \"hi\"", "back\\slash", "tab\tseparated"] {
        assert!(
            matches!(SearchTerm::new(bad), Err(Error::Input(_))),
            "{bad:?}"
        );
    }
    assert!(matches!(
        SearchTerm::new("x".repeat(513)),
        Err(Error::Input(_))
    ));
    assert!(SearchTerm::new("x".repeat(512)).is_ok());
    assert!(matches!(PageSize::new(0), Err(Error::Input(_))));
    assert!(matches!(PageSize::new(101), Err(Error::Input(_))));

    // A window ending beyond 30,000 is refused without a request.
    let term = SearchTerm::new("electron").unwrap();
    let error = api
        .papers()
        .search(term.clone())
        .set_start(29_991)
        .set_size(PageSize::new(10).unwrap())
        .send()
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Input(_)), "{error:?}");
    assert!(server.requests().is_empty());

    // The last permitted window is sent.
    let server = TestServer::start(|_r: &Recorded| {
        let tail: Vec<String> = (0..10)
            .map(|i| plain_entry(&format!("2401.{i:05}v1"), "Tail result"))
            .collect();
        atom(200, feed(30_000, 29_990, 10, &tail))
    })
    .await;
    let feed = client(&server)
        .papers()
        .search(term)
        .set_start(29_990)
        .set_size(PageSize::new(10).unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(feed.entries.len(), 10);
    assert_eq!(server.single().query_all("start"), ["29990"]);
}

#[tokio::test]
async fn search_provider_and_transport_failures() {
    let term = || SearchTerm::new("electron").unwrap();

    // 200 with an Atom error entry: an API failure, provider detail kept but not printed.
    let server = TestServer::start(|_r: &Recorded| {
        atom(
            200,
            error_feed(
                "max_results_must_be_an_integer",
                "max_results must be an integer",
            ),
        )
    })
    .await;
    let error = client(&server)
        .papers()
        .search(term())
        .send()
        .await
        .unwrap_err();
    let shown = format!("{error} {error:?}");
    let Error::Api(api) = error else {
        panic!("expected Error::Api, got {error:?}")
    };
    assert_eq!(api.meta.status().as_u16(), 200);
    assert_eq!(
        api.problem.id,
        "http://arxiv.org/api/errors#max_results_must_be_an_integer"
    );
    assert_eq!(api.problem.summary, "max_results must be an integer");
    assert!(!shown.contains("must be an integer"), "{shown}");

    // 400 with the same error document: status first, document still available.
    let server = TestServer::start(|_r: &Recorded| {
        atom(
            400,
            error_feed("start_must_be_an_integer", "start must be an integer"),
        )
    })
    .await;
    let error = client(&server)
        .papers()
        .search(term())
        .send()
        .await
        .unwrap_err();
    let Error::Http(http) = error else {
        panic!("expected Error::Http, got {error:?}")
    };
    assert_eq!(http.meta.status().as_u16(), 400);
    let document = http.document.as_ref().expect("bounded Atom error document");
    let problem = document.problem.as_ref().expect("error entry");
    assert_eq!(problem.summary, "start must be an integer");

    // Success status, wrong media type: refused before the body is read.
    for media in [
        "text/html",
        "application/xml",
        "application/atom+xml; charset=ISO-8859-1",
    ] {
        let body = feed(1, 0, 10, &[corpus(0)]);
        let server =
            TestServer::start(move |_r: &Recorded| Reply::text(200, media, body.clone())).await;
        let error = client(&server)
            .papers()
            .search(term())
            .send()
            .await
            .unwrap_err();
        assert_eq!(
            concord_kind(&error),
            Some(ErrorKind::Metadata),
            "{media}: {error:?}"
        );
    }
    let server = TestServer::start(|_r: &Recorded| {
        Reply::text(
            200,
            "application/atom+xml; charset=utf-8",
            feed(1, 0, 10, &[corpus(0)]),
        )
    })
    .await;
    assert_eq!(
        client(&server)
            .papers()
            .search(term())
            .send()
            .await
            .unwrap()
            .entries
            .len(),
        1
    );

    // Malformed XML and a document cut short are decode failures.
    for body in [
        "<feed xmlns=\"http://www.w3.org/2005/Atom\"><entry>",
        "not xml at all",
    ] {
        let server = TestServer::start(move |_r: &Recorded| atom(200, body)).await;
        let error = client(&server)
            .papers()
            .search(term())
            .send()
            .await
            .unwrap_err();
        assert_eq!(concord_kind(&error), Some(ErrorKind::Decode), "{error:?}");
    }

    // The connection drops in the middle of the body: a transport failure.
    let server = TestServer::start(|_r: &Recorded| {
        let full = feed(5, 0, 10, &(0..5).map(corpus).collect::<Vec<_>>());
        let prefix = full.as_bytes()[..full.len() / 2].to_vec();
        atom(200, "").with_body(Body::FailAfter(vec![prefix.into()]))
    })
    .await;
    let error = client(&server)
        .papers()
        .search(term())
        .send()
        .await
        .unwrap_err();
    assert_eq!(
        concord_kind(&error),
        Some(ErrorKind::Transport),
        "{error:?}"
    );

    // A body over 2 MiB is not read past the limit.
    let server = TestServer::start(|_r: &Recorded| {
        let padding = " ".repeat(concord_sdk_arxiv::BODY_LIMIT);
        atom(200, format!("{}{padding}", feed(1, 0, 10, &[corpus(0)])))
    })
    .await;
    let error = client(&server)
        .papers()
        .search(term())
        .send()
        .await
        .unwrap_err();
    assert_eq!(concord_kind(&error), Some(ErrorKind::Limit), "{error:?}");

    // 429: no SDK retry, exactly one request.
    let server =
        TestServer::start(|_r: &Recorded| Reply::text(429, "text/plain", "slow down")).await;
    let error = client(&server)
        .papers()
        .search(term())
        .send()
        .await
        .unwrap_err();
    assert!(
        matches!(&error, Error::Http(h) if h.meta.status().as_u16() == 429),
        "{error:?}"
    );
    assert_eq!(server.requests().len(), 1);
}

// ---------------------------------------------------------------- workflow 2: fetch by ID

#[tokio::test]
async fn fetch_known_ids_and_read_metadata() {
    let requested = ["2101.00001v2", "hep-ex/0307015", "cond-mat/0207270v1"];
    // The provider answers in its own order, without the third paper, with renamed
    // namespace prefixes and feed-level extension elements.
    let server = TestServer::start(|_r: &Recorded| {
        let mut body = feed_with("os", "ax", 2, 0, 50, &[sparse_entry(), full_entry()]);
        body = body.replace(
            "<updated>2026-11-07",
            "<x:entry xmlns:x=\"urn:example:mirror\"><x:id>mirror-1</x:id></x:entry>\n  <updated>2026-11-07",
        );
        atom(200, body)
    })
    .await;
    let api = client(&server);
    let ids = IdList::new(&requested).unwrap();
    let request = api.papers().fetch(ids);
    let debug = format!("{request:?}");
    assert!(
        !debug.contains("0307015"),
        "request Debug withholds IDs: {debug}"
    );
    let feed = request.send().await.unwrap();

    let r = server.single();
    assert_ordinary_get(&r);
    assert_eq!(
        r.query(),
        Some(
            "id_list=2101.00001v2%2Chep-ex%2F0307015%2Ccond-mat%2F0207270v1&start=0&max_results=50"
        )
    );
    assert!(r.query_all("search_query").is_empty());
    assert_eq!(
        r.query_all("id_list"),
        ["2101.00001v2,hep-ex/0307015,cond-mat/0207270v1"]
    );

    assert_eq!(
        (feed.total_results, feed.start, feed.items_per_page),
        (2, 0, 50)
    );
    assert!(feed.problem.is_none());
    let returned: Vec<&str> = feed.entries.iter().map(abs_id).collect();
    assert_eq!(
        returned,
        ["hep-ex/0307015", "2101.00001v2"],
        "provider order, extensions ignored"
    );
    let missing: Vec<&str> = requested
        .iter()
        .copied()
        .filter(|id| !returned.contains(id))
        .collect();
    assert_eq!(missing, ["cond-mat/0207270v1"]);

    let full = &feed.entries[1];
    assert_eq!(full.doi.as_deref(), Some("10.1103/PhysRevB.000.000001"));
    assert_eq!(
        full.authors[0].affiliations.len(),
        2,
        "ax:affiliation is the arXiv namespace"
    );
    assert_eq!(
        full.primary_category.as_ref().unwrap().term,
        "cond-mat.str-el"
    );
    assert_eq!(
        full.journal_ref.as_deref(),
        Some("Phys. Rev. B 000, 000001 (2021)")
    );
    let sparse = &feed.entries[0];
    assert_eq!(
        sparse.title,
        "Multi-Electron Production at High Transverse Momenta in ep Collisions at HERA"
    );
    assert_eq!((sparse.comment.as_ref(), sparse.doi.as_ref()), (None, None));
    assert_eq!(sparse.published, "2003-07-07T13:46:39-04:00");
}

#[tokio::test]
async fn fetch_refuses_unsafe_lists_before_dispatch() {
    assert!(matches!(IdList::new(&[]), Err(Error::Input(_))));
    let many: Vec<String> = (0..51).map(|i| format!("2101.{i:05}")).collect();
    let many: Vec<&str> = many.iter().map(String::as_str).collect();
    assert!(matches!(IdList::new(&many), Err(Error::Input(_))));
    assert!(IdList::new(&many[..50]).is_ok());
    for bad in [
        "2101.00001,2101.00002",
        "http://arxiv.org/abs/2101.00001",
        "2101.00001 ",
        "",
        "a&b=c",
    ] {
        assert!(
            matches!(IdList::new(&[bad]), Err(Error::Input(_))),
            "{bad:?}"
        );
    }
    // Grammar is the provider's to judge.
    assert!(IdList::new(&["1234.12345"]).is_ok());
    assert_eq!(
        IdList::new(&["cs/9901002v1", "2101.00001"])
            .unwrap()
            .to_string(),
        "cs/9901002v1,2101.00001"
    );
}

#[tokio::test]
async fn fetch_provider_errors_and_required_fields() {
    // A malformed identifier, answered with the manual's error feed under 400.
    let server = TestServer::start(|_r: &Recorded| {
        atom(
            400,
            error_feed(
                "incorrect_id_format_for_1234.12345",
                "incorrect id format for 1234.12345",
            ),
        )
    })
    .await;
    let error = client(&server)
        .papers()
        .fetch(IdList::new(&["1234.12345"]).unwrap())
        .send()
        .await
        .unwrap_err();
    let shown = format!("{error} | {error:?}");
    assert!(
        !shown.contains("1234.12345"),
        "error formatting withholds IDs: {shown}"
    );
    let Error::Http(http) = error else {
        panic!("expected Error::Http, got {error:?}")
    };
    assert_eq!(http.meta.status().as_u16(), 400);
    let problem = http.document.as_ref().unwrap().problem.as_ref().unwrap();
    assert_eq!(
        problem.id,
        "http://arxiv.org/api/errors#incorrect_id_format_for_1234.12345"
    );
    assert_eq!(server.single().query_all("id_list"), ["1234.12345"]);

    // The same feed with a 200 status: an API failure, not an empty result.
    let server = TestServer::start(|_r: &Recorded| {
        atom(
            200,
            error_feed(
                "incorrect_id_format_for_1234.12345",
                "incorrect id format for 1234.12345",
            ),
        )
    })
    .await;
    let error = client(&server)
        .papers()
        .fetch(IdList::new(&["1234.12345"]).unwrap())
        .send()
        .await
        .unwrap_err();
    assert!(
        matches!(&error, Error::Api(a) if a.problem.summary == "incorrect id format for 1234.12345"),
        "{error:?}"
    );

    // An entry missing a required field fails the feed; missing optional ones do not.
    let without_summary = sparse_entry().replace(
        "<summary>Multi-electron production is studied at high electron transverse momentum.</summary>",
        "",
    );
    let server = TestServer::start(move |_r: &Recorded| {
        atom(
            200,
            feed(2, 0, 50, &[full_entry(), without_summary.clone()]),
        )
    })
    .await;
    let error = client(&server)
        .papers()
        .fetch(IdList::new(&["2101.00001v2", "hep-ex/0307015"]).unwrap())
        .send()
        .await
        .unwrap_err();
    assert_eq!(concord_kind(&error), Some(ErrorKind::Decode), "{error:?}");
}
