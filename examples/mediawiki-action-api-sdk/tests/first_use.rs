//! Fresh-consumer application tests, written from the README and rustdoc only.
//!
//! A small "physics reading list" application: (1) search a wiki, pick the exact-title hit
//! (the second result) and fetch its raw content; (2) list every member of a category,
//! following every field of each `continue` object. Every response comes from a synthetic
//! loopback fixture modelled on API:Continue/en and API:Query (read 2026-11-07); page IDs,
//! revisions and continuation values are invented. Live provider calls: NOT_RUN.

use concord_sdk_mediawiki_action_api::{
    CategoryPages, Continuation, Error, MediaWiki, MediaWikiConfig, MediaWikiQuery, PageIdentity,
    concord::{ErrorKind, reqwest},
    pagination_engine::{PagesError, Resume},
};
use concord_testserver::{Body, Recorded, Reply, TestServer};
use std::{collections::BTreeMap, num::NonZeroUsize, time::Duration};

const UA: &str =
    "PhysicsReadingList/0.3 (https://example.invalid/reading-list; ops@example.invalid)";
const CATEGORY: &str = "Category:Physics";

fn client(server: &TestServer) -> MediaWiki {
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();
    MediaWiki::new(
        http,
        MediaWikiConfig {
            user_agent: UA.into(),
        },
    )
    .with_base_url(format!("{}/w/api.php", server.url()))
    .unwrap()
}

fn pairs(req: &Recorded) -> Vec<(String, String)> {
    req.query_pairs()
}

fn owned(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
}

/// Headers every request of this application must carry: identity, JSON, nothing else.
fn assert_public_headers(req: &Recorded) {
    assert_eq!(req.method, "GET");
    assert_eq!(req.path(), "/w/api.php");
    assert_eq!(req.header("user-agent"), Some(UA));
    assert_eq!(req.header("accept"), Some("application/json"));
    for absent in ["authorization", "cookie", "content-type"] {
        assert!(req.header_all(absent).is_empty(), "{absent} sent");
    }
    assert!(req.body.is_empty());
}

fn value(req: &Recorded, name: &str) -> Option<String> {
    let all = req.query_all(name);
    assert!(all.len() <= 1, "{name} sent {} times", all.len());
    all.into_iter().next()
}

fn titles(items: &[PageIdentity]) -> Vec<&str> {
    items.iter().map(|p| p.title.as_str()).collect()
}

// ---------------------------------------------------------------------------------------
// Workflow 1: search → choose a hit → fetch its content
// ---------------------------------------------------------------------------------------

const ARTICLE: &str = "{{Short description|Geometric theory of gravitation}}\n'''General relativity''' is the geometric theory of gravitation published by [[Albert Einstein]] in 1915.\n";

fn search_wiki(req: &Recorded) -> Reply {
    let base_ok = value(req, "action").as_deref() == Some("query")
        && value(req, "format").as_deref() == Some("json")
        && value(req, "formatversion").as_deref() == Some("2");
    if !base_ok {
        return Reply::json(400, r#"{"error":{"code":"badrequest","info":"fixture"}}"#);
    }
    if value(req, "list").as_deref() == Some("search") {
        assert_eq!(
            value(req, "srsearch").as_deref(),
            Some("General relativity")
        );
        return match (
            value(req, "sroffset").as_deref(),
            value(req, "continue").as_deref(),
        ) {
            (None, None) => Reply::json(
                200,
                r#"{"batchcomplete":true,"continue":{"sroffset":3,"continue":"-||"},"query":{"search":[
                    {"ns":0,"title":"Introduction to general relativity","pageid":9105},
                    {"ns":0,"title":"General relativity","pageid":12024},
                    {"ns":0,"title":"Tests of general relativity","pageid":1215403}]}}"#,
            ),
            (Some("3"), Some("-||")) => Reply::json(
                200,
                r#"{"batchcomplete":true,"query":{"search":[
                    {"ns":0,"title":"Mathematics of general relativity","pageid":249371}]}}"#,
            ),
            _ => Reply::json(
                200,
                r#"{"error":{"code":"badcontinue","info":"Invalid continue param."}}"#,
            )
            .header("mediawiki-api-error", "badcontinue"),
        };
    }
    if value(req, "prop").as_deref() == Some("revisions") {
        let page = value(req, "pageids").unwrap();
        return match page.as_str() {
            "12024" => Reply::json(
                200,
                serde_json::json!({
                    "batchcomplete": true,
                    "query": {"pages": [{
                        "pageid": 12024, "ns": 0, "title": "General relativity",
                        "revisions": [{
                            "revid": 1187650211, "parentid": 1187600004,
                            "slots": {"main": {
                                "contentmodel": "wikitext", "contentformat": "text/x-wiki",
                                "content": ARTICLE
                            }}
                        }]
                    }]}
                })
                .to_string(),
            ),
            _ => Reply::json(
                200,
                format!(
                    r#"{{"batchcomplete":true,"query":{{"pages":[{{"pageid":{page},"missing":true}}]}}}}"#
                ),
            ),
        };
    }
    Reply::json(400, r#"{"error":{"code":"badrequest","info":"fixture"}}"#)
}

#[tokio::test]
async fn search_choose_exact_title_and_fetch_its_content() {
    let server = TestServer::start(search_wiki).await;
    let api = client(&server);
    let query = api.query();

    let found = query
        .search("General relativity")
        .set_srlimit(3)
        .send()
        .await
        .unwrap();
    assert_eq!(found.meta.status(), 200);
    assert_eq!(
        found.meta.operation().unwrap().name(),
        "MediaWiki.query.search"
    );
    assert_eq!(
        titles(&found.data.search),
        [
            "Introduction to general relativity",
            "General relativity",
            "Tests of general relativity"
        ]
    );
    // The application chooses the exact-title hit, which is the second result.
    let chosen = found
        .data
        .search
        .iter()
        .find(|p| p.title == "General relativity")
        .unwrap();
    assert_eq!((chosen.pageid, chosen.ns), (12024, 0));

    let next = found
        .continuation
        .clone()
        .expect("the provider said there is more");
    let expected: BTreeMap<String, String> = [("continue", "-||"), ("sroffset", "3")]
        .iter()
        .map(|(k, v)| ((*k).into(), (*v).into()))
        .collect();
    assert_eq!(
        next.fields(),
        &expected,
        "numeric sroffset kept as its decimal text"
    );
    let shown = format!("{next:?} {next}");
    assert!(
        !shown.contains("-||") && !shown.contains("sroffset"),
        "{shown}"
    );

    let page = query.fetch_page(chosen.pageid).await.unwrap();
    assert_eq!(page.pageid, 12024);
    assert_eq!(page.title, "General relativity");
    assert_eq!(page.revision, 1187650211);
    assert_eq!(page.content, ARTICLE);

    // Second search page: every continuation field goes back; no `continue` means the end.
    let rest = query
        .search("General relativity")
        .set_srlimit(3)
        .set_continuation(next)
        .send()
        .await
        .unwrap();
    assert_eq!(
        titles(&rest.data.search),
        ["Mathematics of general relativity"]
    );
    assert!(rest.continuation.is_none());

    let reqs = server.requests();
    assert_eq!(reqs.len(), 3, "one wire request per call");
    reqs.iter().for_each(assert_public_headers);
    assert_eq!(
        pairs(&reqs[0]),
        owned(&[
            ("action", "query"),
            ("format", "json"),
            ("formatversion", "2"),
            ("srsearch", "General relativity"),
            ("srlimit", "3"),
            ("list", "search"),
            ("srprop", ""),
        ])
    );
    assert_eq!(
        pairs(&reqs[1]),
        owned(&[
            ("action", "query"),
            ("format", "json"),
            ("formatversion", "2"),
            ("pageids", "12024"),
            ("prop", "revisions"),
            ("rvprop", "ids|content"),
            ("rvslots", "main"),
            ("rvlimit", "1"),
        ])
    );
    assert_eq!(
        pairs(&reqs[2]),
        owned(&[
            ("action", "query"),
            ("format", "json"),
            ("formatversion", "2"),
            ("srsearch", "General relativity"),
            ("srlimit", "3"),
            // Observed: continuation pairs sit where `continuation` is declared, not at the end.
            ("continue", "-||"),
            ("sroffset", "3"),
            ("list", "search"),
            ("srprop", ""),
        ])
    );

    // The identifying User-Agent is not printed by the client's Debug.
    assert!(!format!("{api:?} {query:?}").contains("PhysicsReadingList"));
}

#[tokio::test]
async fn search_and_fetch_failures_are_typed_and_quiet() {
    // Each request's srsearch picks the provider behaviour; content IDs pick content cases.
    let server = TestServer::start(|req: &Recorded| {
        let term = value(req, "srsearch");
        let id = value(req, "pageids");
        match (term.as_deref(), id.as_deref()) {
            (Some("throttled"), _) => Reply::json(
                429,
                r#"{"error":{"code":"ratelimited","info":"You've exceeded your rate limit. secret-ish-detail"}}"#,
            )
            .header("retry-after", "7")
            .header("mediawiki-api-error", "ratelimited"),
            (Some("proxy"), _) => Reply::text(503, "text/html", "<html><body>Wikimedia Error</body></html>"),
            (Some("bad"), _) => Reply::json(
                200,
                r#"{"error":{"code":"srsearch-text-disabled","info":"text search is disabled: secret-ish-detail"},"servedby":"mw1234"}"#,
            )
            .header("mediawiki-api-error", "srsearch-text-disabled"),
            (Some("warned"), _) => Reply::json(
                200,
                r#"{"batchcomplete":true,"warnings":{"search":{"warnings":"srlimit may not be over 500"}},"query":{"search":[{"ns":0,"title":"A","pageid":1}]}}"#,
            ),
            (Some("noquery"), _) => Reply::json(200, r#"{"batchcomplete":true}"#),
            (Some("cut"), _) => Reply::json(200, r#"{"batchcomplete":true,"query":{"search":[{"ns":0,"ti"#),
            (Some("html"), _) => Reply::text(200, "text/html; charset=utf-8", "<!doctype html><p>login wall</p>"),
            (Some("huge"), _) => Reply::json(200, format!(r#"{{"query":{{"search":[]}},"pad":"{}"}}"#, "x".repeat(1_100_000))),
            (Some("hangup"), _) => Reply::hang_up(),
            (Some("dropped"), _) => Reply::json(200, "").with_body(Body::FailAfter(vec![
                r#"{"batchcomplete":true,"query":{"search":[{"ns":0,"#.into(),
            ])),
            (Some("badtoken"), _) => Reply::json(
                200,
                r#"{"continue":{"sroffset":3},"query":{"search":[{"ns":0,"title":"A","pageid":1}]}}"#,
            ),
            (None, Some("404")) => Reply::json(
                200,
                r#"{"batchcomplete":true,"query":{"pages":[{"pageid":404,"ns":0,"title":"Deleted article","missing":true}]}}"#,
            ),
            (None, Some("405")) => Reply::json(200, r#"{"batchcomplete":true,"query":{"pages":[{"pageid":405,"ns":0,"title":"No revisions"}]}}"#),
            (None, Some("406")) => Reply::json(
                200,
                r#"{"batchcomplete":true,"query":{"pages":[{"pageid":406,"ns":0,"title":"A","revisions":[{"revid":1,"slots":{"main":{"content":"x"}}}]},{"pageid":407,"ns":0,"title":"B"}]}}"#,
            ),
            (None, Some("777")) => Reply::json(
                200,
                r#"{"batchcomplete":true,"query":{"pages":[{"pageid":777,"ns":0,"title":"Hidden","revisions":[{"revid":5,"slots":{"main":{"texthidden":true}}}]}]}}"#,
            ),
            (None, Some("555")) => Reply::json(
                200,
                r#"{"batchcomplete":true,"query":{"pages":[{"pageid":555,"ns":0,"title":"Blank","revisions":[{"revid":9,"slots":{"main":{"contentmodel":"wikitext","content":""}}}]}]}}"#,
            ),
            (None, Some("600")) => Reply::json(
                200,
                r#"{"batchcomplete":true,"query":{"pages":[{"pageid":601,"ns":0,"title":"Other","revisions":[{"revid":9,"slots":{"main":{"content":"x"}}}]}]}}"#,
            ),
            _ => Reply::json(400, r#"{"error":{"code":"badrequest","info":"fixture"}}"#),
        }
    })
    .await;
    let api = client(&server);
    let query = api.query();
    let search = |term: &'static str| query.search(term).send();

    // 429: an HTTP failure with authoritative status/head and the typed provider document.
    let Err(Error::Http(failure)) = search("throttled").await else {
        panic!("429 is Http")
    };
    assert_eq!(failure.meta.status(), 429);
    assert_eq!(failure.meta.headers()["retry-after"], "7");
    let doc = failure.document.as_ref().expect("JSON error document");
    assert_eq!(doc.error.code, "ratelimited");
    let shown = format!("{failure:?}");
    assert!(!shown.contains("secret-ish-detail"), "{shown}");

    // 503 from a proxy with HTML: still Http; the document attempt failed, typed.
    let Err(Error::Http(failure)) = search("proxy").await else {
        panic!("503 is Http")
    };
    assert_eq!(failure.meta.status(), 503);
    let refused = failure
        .document
        .as_ref()
        .expect_err("HTML is not the error document");
    assert_eq!(refused.kind(), ErrorKind::Metadata);

    // MediaWiki error envelope inside HTTP 200.
    let err = search("bad").await.unwrap_err();
    let Error::Api { meta, problem } = &err else {
        panic!("Api expected: {err:?}")
    };
    assert_eq!(meta.status(), 200);
    assert_eq!(
        meta.headers()["mediawiki-api-error"],
        "srsearch-text-disabled"
    );
    assert_eq!(problem.code, "srsearch-text-disabled");
    assert!(
        problem.info.contains("secret-ish-detail"),
        "explicit access is unredacted"
    );
    let shown = format!("{err} {err:?}");
    assert!(!shown.contains("secret-ish-detail"), "{shown}");

    // Warnings are refused even though `query` is present.
    let err = search("warned").await.unwrap_err();
    assert!(
        matches!(&err, Error::Warnings(meta) if meta.status() == 200),
        "{err:?}"
    );

    // Missing `query`.
    assert!(matches!(
        search("noquery").await,
        Err(Error::InvalidResponse(_))
    ));

    // Truncated JSON, an HTML 200 and an oversized body are Concord errors with their stage.
    for (term, kind) in [
        ("cut", ErrorKind::Decode),
        ("html", ErrorKind::Metadata),
        ("huge", ErrorKind::Limit),
    ] {
        let Err(Error::Concord(e)) = search(term).await else {
            panic!("{term}: Concord expected")
        };
        assert_eq!(e.kind(), kind, "{term}");
        assert_eq!(
            e.head().map(|h| h.status()),
            Some(reqwest::StatusCode::OK),
            "{term}"
        );
        assert_eq!(
            e.operation().unwrap().name(),
            "MediaWiki.query.search",
            "{term}"
        );
    }

    // Not predicted (added after the first run): transport failures before a head and while
    // reading a 200 body. Both are Concord Transport errors; only the second has a head.
    let Err(Error::Concord(e)) = search("hangup").await else {
        panic!("hang-up: Concord")
    };
    assert_eq!(e.kind(), ErrorKind::Transport);
    assert!(e.head().is_none());
    let Err(Error::Concord(e)) = search("dropped").await else {
        panic!("dropped: Concord")
    };
    assert_eq!(e.kind(), ErrorKind::Transport);
    assert_eq!(e.head().map(|h| h.status()), Some(reqwest::StatusCode::OK));

    // A continuation object without the generic `continue` is refused, not followed later.
    let err = search("badtoken").await.unwrap_err();
    assert!(
        matches!(&err, Error::Concord(e) if e.kind() == ErrorKind::Decode),
        "{err:?}"
    );

    // Content projection.
    assert!(matches!(
        query.fetch_page(404).await,
        Err(Error::ContentUnavailable)
    ));
    assert!(
        matches!(query.fetch_page(405).await, Err(Error::ContentUnavailable)),
        "no revision"
    );
    let err = query.fetch_page(406).await.unwrap_err();
    assert!(
        matches!(&err, Error::InvalidResponse(meta) if meta.status() == 200),
        "two pages: {err:?}"
    );
    assert!(matches!(
        query.fetch_page(777).await,
        Err(Error::ContentUnavailable)
    ));
    assert!(
        matches!(query.fetch_page(600).await, Err(Error::ContentUnavailable)),
        "page ID mismatch"
    );
    let blank = query.fetch_page(555).await.unwrap();
    assert_eq!(
        (blank.title.as_str(), blank.revision, blank.content.as_str()),
        ("Blank", 9, "")
    );

    // One wire request per call, all with the same public headers; none carries the UA in errors.
    let reqs = server.requests();
    assert_eq!(reqs.len(), 17);
    reqs.iter().for_each(assert_public_headers);
}

// ---------------------------------------------------------------------------------------
// Workflow 2: category members, following every continuation field
// ---------------------------------------------------------------------------------------

/// What the synthetic wiki does when the generic `continue` is missing from a follow-up.
#[derive(Clone, Copy)]
enum WithoutGeneric {
    /// Ignore the module token and start over (repeats page 1).
    Restart,
    /// Answer an empty, final-looking result (silent truncation).
    LookFinished,
}

struct CatPage {
    /// The `continue` object that leads to this page (None for the first page).
    via: Option<&'static [(&'static str, &'static str)]>,
    members: &'static [(u64, i32, &'static str)],
}

const PAGE_1_TOKEN: &[(&str, &str)] = &[
    ("cmcontinue", "page|4b494e454d4154494353|61015"),
    ("continue", "-||"),
];
const PAGE_2_TOKEN: &[(&str, &str)] = &[
    ("cmcontinue", "page|4f5054494353|22498"),
    ("continue", "-||"),
];
// Synthetic: a changed generic value and an extra field the SDK cannot know about.
const PAGE_3_TOKEN: &[(&str, &str)] = &[
    ("cmcontinue", "subcat|50415254|880412"),
    ("cmshard", "s2"),
    ("continue", "-||categorymembers"),
];
const PAGE_4_TOKEN: &[(&str, &str)] = &[("cmcontinue", "file|504859|1004"), ("continue", "-||")];

const CAT_PAGES: &[CatPage] = &[
    CatPage {
        via: None,
        members: &[(22939, 0, "Physics"), (18963910, 0, "Classical mechanics")],
    },
    CatPage {
        via: Some(PAGE_1_TOKEN),
        members: &[(61015, 0, "Kinematics"), (19555586, 0, "Optics")],
    },
    // Empty page that still has a continuation: must advance.
    CatPage {
        via: Some(PAGE_2_TOKEN),
        members: &[],
    },
    CatPage {
        via: Some(PAGE_3_TOKEN),
        members: &[
            (880412, 14, "Category:Particle physics"),
            (1004, 6, "File:Pendulum.svg"),
        ],
    },
    CatPage {
        via: Some(PAGE_4_TOKEN),
        members: &[(23535, 0, "Quantum mechanics")],
    },
];

fn continue_json(token: &[(&str, &str)]) -> serde_json::Value {
    serde_json::Value::Object(
        token
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).into()))
            .collect(),
    )
}

fn category_page_reply(index: usize) -> Reply {
    let members: Vec<_> = CAT_PAGES[index]
        .members
        .iter()
        .map(|(id, ns, title)| serde_json::json!({"pageid": id, "ns": ns, "title": title}))
        .collect();
    let mut body =
        serde_json::json!({"batchcomplete": true, "query": {"categorymembers": members}});
    if let Some(next) = CAT_PAGES.get(index + 1) {
        body["continue"] = continue_json(next.via.unwrap());
    }
    Reply::json(200, body.to_string()).header("x-fixture-page", index.to_string())
}

fn category_wiki(req: &Recorded, without_generic: WithoutGeneric) -> Reply {
    let fixed = [
        ("action", "query"),
        ("format", "json"),
        ("formatversion", "2"),
        ("cmtitle", CATEGORY),
        ("cmlimit", "2"),
        ("list", "categorymembers"),
    ];
    for (k, v) in fixed {
        if value(req, k).as_deref() != Some(v) {
            return Reply::json(
                400,
                format!(r#"{{"error":{{"code":"badrequest","info":"{k}"}}}}"#),
            );
        }
    }
    let sent: BTreeMap<String, String> = pairs(req)
        .into_iter()
        .filter(|(k, _)| !fixed.iter().any(|(f, _)| f == k))
        .collect();
    if sent.is_empty() {
        return category_page_reply(0);
    }
    if !sent.contains_key("continue") {
        return match without_generic {
            WithoutGeneric::Restart => category_page_reply(0),
            WithoutGeneric::LookFinished => Reply::json(
                200,
                r#"{"batchcomplete":true,"query":{"categorymembers":[]}}"#,
            ),
        };
    }
    for (index, page) in CAT_PAGES.iter().enumerate().skip(1) {
        let via: BTreeMap<String, String> = page
            .via
            .unwrap()
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        if via == sent {
            return category_page_reply(index);
        }
    }
    Reply::json(200, r#"{"error":{"code":"badcontinue","info":"Invalid continue param. You should pass the original value returned by the previous query."}}"#)
        .header("mediawiki-api-error", "badcontinue")
}

/// The whole listing in order: what a correct traversal must return.
fn all_members() -> Vec<&'static str> {
    CAT_PAGES
        .iter()
        .flat_map(|p| p.members.iter().map(|m| m.2))
        .collect()
}

fn continuation_of(req: &Recorded) -> BTreeMap<String, String> {
    let fixed = [
        "action",
        "format",
        "formatversion",
        "cmtitle",
        "cmlimit",
        "list",
    ];
    for k in fixed {
        assert_eq!(req.query_all(k).len(), 1, "{k} exactly once");
    }
    let mut out = BTreeMap::new();
    for (k, v) in pairs(req)
        .into_iter()
        .filter(|(k, _)| !fixed.contains(&k.as_str()))
    {
        assert!(out.insert(k.clone(), v).is_none(), "{k} sent twice");
    }
    out
}

fn token_map(token: &[(&str, &str)]) -> BTreeMap<String, String> {
    token
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
}

fn category(query: &MediaWikiQuery, from: Option<Continuation>, max: usize) -> CategoryPages {
    query.category_pages(CATEGORY, 2, from, NonZeroUsize::new(max).unwrap())
}

#[tokio::test]
async fn category_traversal_carries_every_continue_field_to_the_end() {
    let server =
        TestServer::start(|req: &Recorded| category_wiki(req, WithoutGeneric::Restart)).await;
    let api = client(&server);
    let query = api.query();

    let members = category(&query, None, 10)
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(titles(&members), all_members());
    let namespaces: Vec<i32> = members.iter().map(|m| m.ns).collect();
    assert_eq!(
        namespaces,
        [0, 0, 0, 0, 14, 6, 0],
        "subcategories and files are members too"
    );

    // Five pages, five requests, each carrying exactly the previous `continue` object once.
    let reqs = server.requests();
    assert_eq!(reqs.len(), CAT_PAGES.len());
    reqs.iter().for_each(assert_public_headers);
    for (req, page) in reqs.iter().zip(CAT_PAGES) {
        assert_eq!(
            continuation_of(req),
            page.via.map(token_map).unwrap_or_default()
        );
    }

    // Incremental reading: one page per call, the empty page is a page, then Ok(None) for good.
    let mut pages = category(&query, None, 10);
    let mut sizes = Vec::new();
    while let Some(page) = pages.next_page().await.unwrap() {
        sizes.push(page.items.len());
    }
    assert_eq!(sizes, [2, 2, 0, 2, 1]);
    assert!(pages.next_page().await.unwrap().is_none());
    assert!(pages.checkpoint().is_none());
    assert_eq!(
        server.requests().len(),
        2 * CAT_PAGES.len(),
        "no request after exhaustion"
    );

    // Per-page metadata is only available through the single-page endpoint.
    let first = query
        .category_members(CATEGORY)
        .set_cmlimit(2)
        .send()
        .await
        .unwrap();
    assert_eq!(first.meta.headers()["x-fixture-page"], "0");
    assert_eq!(
        first.meta.operation().unwrap().name(),
        "MediaWiki.query.category_members"
    );
    assert_eq!(
        first.continuation.unwrap().fields(),
        &token_map(PAGE_1_TOKEN)
    );
}

#[tokio::test]
async fn category_bound_resume_and_failure_keep_the_prefix() {
    // The page behind PAGE_3_TOKEN fails once with a 503; everything else is the normal wiki.
    let failing_once = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let flag = failing_once.clone();
    let server = TestServer::start(move |req: &Recorded| {
        let sent = continuation_of(req);
        if sent == token_map(PAGE_3_TOKEN) && flag.swap(false, std::sync::atomic::Ordering::SeqCst)
        {
            return Reply::json(
                503,
                r#"{"error":{"code":"internal_api_error_DBQueryTimeoutError","info":"timeout"}}"#,
            )
            .header("retry-after", "30");
        }
        category_wiki(req, WithoutGeneric::Restart)
    })
    .await;
    let api = client(&server);
    let query = api.query();

    // Bound of 2 requests while the provider has more: Incomplete with the prefix and the key.
    let incomplete = category(&query, None, 2)
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(
        titles(&incomplete.collected),
        ["Physics", "Classical mechanics", "Kinematics", "Optics"]
    );
    let PagesError::Bounded { resume, .. } = &incomplete.error else {
        panic!("a bound is not exhaustion: {:?}", incomplete.error)
    };
    let Resume::At(key) = resume else {
        panic!("resume at a key")
    };
    assert_eq!(key.fields(), &token_map(PAGE_2_TOKEN));
    assert_eq!(server.requests().len(), 2);

    // Resume from the README's recipe; the empty page, then the 503 on the next page.
    let from = incomplete.error.resume().and_then(|at| at.key()).cloned();
    let mut collected = incomplete.collected;
    let failed = category(&query, from, 10)
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert!(
        failed.collected.is_empty(),
        "the empty page contributes nothing"
    );
    let PagesError::Provider {
        error, failed: at, ..
    } = &failed.error
    else {
        panic!("provider failure expected: {:?}", failed.error)
    };
    let Error::Http(http) = error else {
        panic!("503 is Http: {error:?}")
    };
    assert_eq!(http.meta.status(), 503);
    assert_eq!(http.meta.headers()["retry-after"], "30");
    assert_eq!(
        http.document.as_ref().unwrap().error.code,
        "internal_api_error_DBQueryTimeoutError"
    );
    assert_eq!(
        at.key().unwrap().fields(),
        &token_map(PAGE_3_TOKEN),
        "the failed page is retryable"
    );

    // Retry the failed page later (the application decides when) and finish the listing.
    let retry_from = failed.error.resume().and_then(|at| at.key()).cloned();
    collected.extend(failed.collected);
    let rest = category(&query, retry_from, 10)
        .items()
        .collect_complete()
        .await
        .unwrap();
    collected.extend(rest);
    assert_eq!(
        titles(&collected),
        all_members(),
        "no duplicates, nothing skipped"
    );

    // After a failure, a traversal reports Ended — never a fake end of the listing.
    let mut pages = category(&query, None, 1);
    assert_eq!(pages.next_page().await.unwrap().unwrap().items.len(), 2);
    assert!(matches!(
        pages.next_page().await,
        Err(PagesError::Bounded { .. })
    ));
    assert!(matches!(
        pages.next_page().await,
        Err(PagesError::Ended { .. })
    ));

    let wire: Vec<_> = server.requests().iter().map(continuation_of).collect();
    let expected: Vec<BTreeMap<String, String>> = vec![
        BTreeMap::new(),
        token_map(PAGE_1_TOKEN), // bounded run
        token_map(PAGE_2_TOKEN),
        token_map(PAGE_3_TOKEN), // resumed run, 503
        token_map(PAGE_3_TOKEN),
        token_map(PAGE_4_TOKEN), // retry run
        BTreeMap::new(),         // bound of one
    ];
    assert_eq!(wire, expected);
}

/// A hand-written consumer that forwards only the module token (`cmcontinue`), dropping the
/// generic `continue`. Needs the native escape: `prepare()`, `Prepared::follow`, `send_raw()`.
async fn broken_consumer(query: &MediaWikiQuery, max_requests: usize) -> Vec<String> {
    let mut seen = Vec::new();
    let mut next: Option<Continuation> = None;
    for _ in 0..max_requests {
        let prepared = query
            .category_members(CATEGORY)
            .set_cmlimit(2)
            .maybe_continuation(next.take())
            .prepare()
            .unwrap();
        let mut url = prepared.request().url().clone();
        let kept: Vec<(String, String)> = url
            .query_pairs()
            .filter(|(k, _)| k != "continue")
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        url.query_pairs_mut().clear().extend_pairs(kept);
        let response = prepared
            .follow(url.as_str())
            .unwrap()
            .send_raw()
            .await
            .unwrap();
        let body: serde_json::Value =
            serde_json::from_slice(&response.bytes().await.unwrap()).unwrap();
        for m in body["query"]["categorymembers"].as_array().unwrap() {
            seen.push(m["title"].as_str().unwrap().to_owned());
        }
        match body.get("continue") {
            Some(token) => next = Some(serde_json::from_value(token.clone()).unwrap()),
            None => break,
        }
    }
    seen
}

#[tokio::test]
async fn dropping_the_generic_continue_repeats_or_truncates() {
    for mode in [WithoutGeneric::Restart, WithoutGeneric::LookFinished] {
        let server = TestServer::start(move |req: &Recorded| category_wiki(req, mode)).await;
        let api = client(&server);
        let query = api.query();

        // Broken control: repeats page 1 forever (bounded here), or stops after page 1 silently.
        let broken = broken_consumer(&query, 4).await;
        let broken_requests = server.requests();
        match mode {
            WithoutGeneric::Restart => {
                assert_eq!(broken.len(), 8);
                assert_eq!(broken[..2], broken[2..4], "page 1 repeated");
                assert!(!broken.iter().any(|t| t == "Kinematics"), "never advances");
            }
            WithoutGeneric::LookFinished => {
                assert_eq!(
                    broken,
                    ["Physics", "Classical mechanics"],
                    "silently truncated"
                );
                assert_eq!(broken_requests.len(), 2);
            }
        }
        assert!(
            broken_requests[1..]
                .iter()
                .all(|r| r.query_all("continue").is_empty())
        );
        assert!(
            broken_requests[1..]
                .iter()
                .all(|r| r.query_all("cmcontinue").len() == 1)
        );

        // The SDK over the very same fixture: every member exactly once, then exhaustion.
        let members = category(&query, None, 10)
            .items()
            .collect_complete()
            .await
            .unwrap();
        assert_eq!(titles(&members), all_members());
        let sdk_requests = &server.requests()[broken_requests.len()..];
        assert!(
            sdk_requests[1..]
                .iter()
                .all(|r| r.query_all("continue").len() == 1)
        );
    }

    // The ordinary path cannot express the broken token: a `continue` object without the
    // generic field (or null), an empty object, or one overwriting a fixed input, does not
    // deserialize. (Observed, not asserted: an empty-string generic value IS accepted.)
    for bad in [
        serde_json::json!({"cmcontinue": "page|4b494e454d4154494353|61015"}),
        serde_json::json!({}),
        serde_json::json!({"cmcontinue": "page|x|1", "continue": null}),
        serde_json::json!({"cmtitle": "Category:Chemistry", "continue": "-||"}),
    ] {
        assert!(
            serde_json::from_value::<Continuation>(bad.clone()).is_err(),
            "{bad}"
        );
    }

    // A provider page whose `continue` lacks the generic field ends the traversal with a typed
    // failure at that page, keeping the prefix — it is not treated as the end of the listing.
    let server = TestServer::start(|req: &Recorded| {
        if continuation_of(req).is_empty() {
            Reply::json(
                200,
                r#"{"batchcomplete":true,"continue":{"cmcontinue":"page|4b494e454d4154494353|61015"},"query":{"categorymembers":[{"pageid":22939,"ns":0,"title":"Physics"}]}}"#,
            )
        } else {
            Reply::json(400, r#"{"error":{"code":"unexpected","info":"fixture"}}"#)
        }
    })
    .await;
    let api = client(&server);
    let incomplete = category(&api.query(), None, 10)
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert!(
        incomplete.collected.is_empty(),
        "the refused page's items are not emitted"
    );
    let PagesError::Provider { error, failed, .. } = &incomplete.error else {
        panic!("typed failure expected: {:?}", incomplete.error)
    };
    assert!(
        matches!(error, Error::Concord(e) if e.kind() == ErrorKind::Decode),
        "{error:?}"
    );
    assert_eq!(failed, &Resume::Start);
    assert_eq!(
        server.requests().len(),
        1,
        "the broken token was never followed"
    );
}

/// FIRST_USE finding F1. For an unknown page ID MediaWiki's page set reports only
/// `{"pageid": N, "missing": true}` (no `ns`/`title`; consumer's knowledge of MediaWiki
/// output, not shown on the two cited pages). The README promises `ContentUnavailable` for a
/// missing page ID; observed on 2026-11-07: `Error::Concord` with `ErrorKind::Decode`.
/// Fixed by the batch-content maintenance (MAINTENANCE.md), so it now runs by default.
#[tokio::test]
async fn missing_page_id_without_title_is_content_unavailable() {
    let server = TestServer::start(|_req: &Recorded| {
        Reply::json(
            200,
            r#"{"batchcomplete":true,"query":{"pages":[{"pageid":404,"missing":true}]}}"#,
        )
    })
    .await;
    let api = client(&server);
    let result = api.query().fetch_page(404).await;
    assert!(
        matches!(result, Err(Error::ContentUnavailable)),
        "{result:?}"
    );
}
