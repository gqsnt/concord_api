//! Sensitive inputs: papers.search SearchTerm, papers.query Query and papers.fetch IdList
//! (src/lib.rs:47,54,61), each with a distinct canary in its query value. No credentials.
//! Provider Atom Problem.summary and response headers echo the term only where withheld.
//! Read-only SDK; search drives seven scenarios, query/fetch also send successfully.
//! Excludes native reqwest, explicit unredacted accessors, clear Paper fields, and the
//! documented wire-syntax Display of SearchTerm/Query/IdList. No wire exemptions.

use concord_check_canary::Canaries;
use concord_testserver::{Body, Recorded, Reply, TestServer};
use std::time::Duration;

fn http() -> concord::reqwest::Client {
    concord::reqwest::Client::builder()
        .no_proxy()
        .redirect(concord::reqwest::redirect::Policy::none())
        .retry(concord::reqwest::retry::never())
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap()
}

fn refused() -> String {
    concord_testserver::refused_url()
}

fn reply(case: usize, media: &str, success: &str, problem: &str, echo: &str) -> Reply {
    let response = match case {
        0 | 6 => Reply::text(200, media, success),
        1 => Reply::text(400, media, problem),
        2 => Reply::text(503, media, problem),
        3 => Reply::text(200, media, format!("<{{invalid {echo}")),
        4 => Reply::text(200, "text/html", echo),
        5 => Reply::text(200, media, "").with_body(Body::FailAfter(vec![
            format!("<{{incomplete {echo}").into(),
        ])),
        _ => unreachable!(),
    };
    response
        .header("x-private", echo)
        .header("set-cookie", echo)
}

fn concord_error(c: &mut Canaries, e: &concord::Error) {
    c.assert_debug_absent("Concord error", e);
    c.assert_error_absent("Concord error/source chain", e);
    c.assert_debug_absent("error kind", &e.kind());
    c.assert_debug_absent("error head", &e.head());
    c.assert_debug_absent("error metadata", &e.head_meta());
}

async fn receive<T: std::fmt::Debug>(c: &mut Canaries, prepared: concord::Prepared<T>) {
    c.assert_debug_absent("Prepared", &prepared);
    let result = prepared.receive().await;
    c.assert_debug_absent("Received Result", &result);
    match result {
        Ok(received) => {
            c.assert_debug_absent("response head", &received.head());
            c.assert_debug_absent("response metadata", &received.meta());
            let decoded = received.decode().await;
            c.assert_debug_absent("native decoded Result", &decoded);
            if let Err(e) = decoded {
                concord_error(c, &e);
            }
        }
        Err(e) => concord_error(c, &e),
    }
}

use concord_ratelimit::Clock;
use concord_sdk_arxiv::concord_pagination::Resume;
use concord_sdk_arxiv::{Arxiv, ArxivConfig, Error, Field, IdList, Policy, Query, SearchTerm};
use std::{
    future::Future,
    num::NonZeroUsize,
    pin::Pin,
    sync::{Arc, Mutex},
};

#[derive(Clone, Default)]
struct VirtualClock(Arc<Mutex<Duration>>);
impl Clock for VirtualClock {
    fn now(&self) -> Duration {
        *self.0.lock().unwrap()
    }
    fn sleep(&self, delay: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        let elapsed = self.0.clone();
        Box::pin(async move {
            *elapsed.lock().unwrap() += delay;
        })
    }
}

fn error(c: &mut Canaries, e: &Error) {
    c.assert_debug_absent("SDK error", e);
    c.assert_error_absent("SDK error/source chain", e);
    match e {
        Error::Concord(e) => concord_error(c, e),
        Error::Http(h) => {
            c.assert_debug_absent("HTTP failure", h);
            c.assert_debug_absent("HTTP metadata", &h.meta);
            c.assert_debug_absent("Atom document", &h.document);
            if let Err(e) = &h.document {
                concord_error(c, e);
            }
        }
        Error::Api(a) => {
            c.assert_debug_absent("API failure", a);
            c.assert_debug_absent("Atom problem", &a.problem);
            c.assert_debug_absent("API metadata", &a.meta);
        }
        _ => {}
    }
}

#[tokio::test]
async fn secrets_never_print() {
    let mut c = Canaries::new("arxiv");
    let term_text = c.issue();
    let query_text = c.issue();
    let ids_text = c.issue();
    let term = SearchTerm::new(&term_text).unwrap();
    let query = Query::term(Field::Title, &query_text).unwrap();
    let ids = IdList::new(&[&ids_text]).unwrap();
    c.assert_debug_absent("search term", &term);
    c.assert_debug_absent("query", &query);
    c.assert_debug_absent("ID list", &ids);
    for case in 0..7 {
        let echo = term_text.clone();
        let server = TestServer::start(move |_: &Recorded| {
            let problem = include_str!("../fixtures/error-id.xml")
                .replace("incorrect id format for 1234.12345", &echo);
            reply(
                case,
                "application/atom+xml",
                include_str!("../fixtures/empty.xml"),
                &problem,
                &echo,
            )
        })
        .await;
        let config = ArxivConfig {
            policy: Policy::with_clock(VirtualClock::default()),
        };
        c.assert_debug_absent("config", &config);
        c.assert_debug_absent("policy", &config.policy);
        let api = Arxiv::new(http(), config)
            .with_base_url(if case == 6 { refused() } else { server.url() })
            .unwrap();
        let papers = api.papers();
        let read = papers.search(term.clone());
        c.assert_debug_absent("client", &api);
        c.assert_debug_absent("papers scope", &papers);
        c.assert_debug_absent("search endpoint", &read);
        receive(&mut c, read.clone().prepare().unwrap()).await;
        let result = read.clone().send().await;
        c.assert_debug_absent("search Result", &result);
        assert_eq!(result.is_ok(), case == 0);
        if let Err(e) = &result {
            error(&mut c, e);
            if case == 1 {
                assert!(e.to_string().contains("400"));
            }
        }
        let mut walk = read.pages(Resume::Start, NonZeroUsize::MIN);
        c.assert_debug_absent("paper traversal", &walk);
        let page = walk.next_page().await;
        c.assert_debug_absent("page Result", &page);
        if let Err(e) = page {
            c.assert_error_absent("PagesError", &e);
        }
        c.assert_debug_absent("traversal checkpoint", &walk.checkpoint());
        c.assert_debug_absent("traversal progress", &walk.progress());
        c.assert_debug_absent("traversal report", &walk.finish());
        if case == 0 {
            let op = papers.query(query.clone());
            c.assert_debug_absent("typed query endpoint", &op);
            receive(&mut c, op.clone().prepare().unwrap()).await;
            c.assert_debug_absent("typed query feed", &op.send().await.unwrap());
            let op = papers.fetch(ids.clone());
            c.assert_debug_absent("fetch endpoint", &op);
            receive(&mut c, op.clone().prepare().unwrap()).await;
            c.assert_debug_absent("fetch feed", &op.send().await.unwrap());
            c.assert_on_wire(&server.requests());
        }
        if case == 6 {
            assert!(server.requests().is_empty());
        }
        server.shutdown().await;
    }
    // A 2xx Atom error entry must reach the SDK's separate API-failure path too.
    let echo = term_text.clone();
    let server = TestServer::start(move |_| {
        Reply::text(
            200,
            "application/atom+xml",
            include_str!("../fixtures/error-id.xml")
                .replace("incorrect id format for 1234.12345", &echo),
        )
    })
    .await;
    let api = Arxiv::with_http(http())
        .with_base_url(server.url())
        .unwrap();
    let e = api.papers().search(term).send().await.unwrap_err();
    error(&mut c, &e);
    assert!(matches!(e, Error::Api(_)));
    server.shutdown().await;
    assert!(c.checked() >= 200);
    println!("arxiv: {} renderings; positive control passed", c.checked());
}
