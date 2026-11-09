//! Secrets: the identifying User-Agent (`#[sensitive] header(USER_AGENT)`, `src/lib.rs:36`;
//! README: "User-Agent values are withheld in generated Debug output"), which may carry the
//! operator's contact. It has its own canary and travels as a header. Response-only values the
//! README says are withheld, each with its own canary reaching the decoder through a fixture:
//! the provider error `code` and `info` (`ApiProblem` Debug prints `<redacted>`,
//! `src/error.rs:15`), in the non-2xx `ErrorDocument` and in a 2xx error envelope; and the
//! values of a returned search and category continuation (`Continuation` "Debug/Display print
//! no values", `src/models.rs:13`), which the SDK sends back on the next request. The server
//! also echoes the User-Agent into headers (cookies are withheld) and into every malformed,
//! wrong-media and truncated body. The SDK is read-only: `search` (generated `send()` and the
//! native `Prepared`), `fetch_page`, `fetch_pages` and a `category_pages` traversal cover the
//! seven scenarios plus a 2xx API-error envelope. A User-Agent refused at build time is
//! checked separately. Not covered: native reqwest values; explicit unredacted accessors
//! (`ApiProblem::code`/`info`, `Continuation::fields`, `ResponseMeta::headers`); the
//! `Prepared` of a continued request (README: "public continuation pairs are public data;
//! prepared URLs can show them"); search terms, titles and page content (public data, printed
//! by design). Wire exemptions: the error `code`/`info` are response-only (never sent).

use concord_check_canary::Canaries;
use concord_sdk_mediawiki_action_api::{
    Error, MediaWiki, MediaWikiConfig, PageIdLimit, PageIds, concord::reqwest,
};
use concord_testserver::{Body, Recorded, Reply, TestServer};
use std::{num::NonZeroUsize, time::Duration};

fn refused() -> String {
    concord_testserver::refused_url()
}

fn api(base: &str, user_agent: &str) -> MediaWiki {
    let http = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();
    let config = MediaWikiConfig {
        user_agent: user_agent.into(),
    };
    MediaWiki::new(http, config)
        .with_base_url(format!("{base}/w/api.php"))
        .unwrap()
}

fn param(r: &Recorded, name: &str) -> Option<String> {
    r.query_pairs()
        .into_iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value)
}

// Echo the User-Agent in headers and in bodies whose formatting is withheld.
fn reply(case: usize, success: String, problem: &str, echo: &str) -> Reply {
    let response = match case {
        0 | 6 => Reply::json(200, success),
        1 => Reply::json(403, problem),
        2 => Reply::json(503, problem),
        3 => Reply::json(200, format!("{{invalid {echo}")),
        4 => Reply::text(200, "text/html", echo),
        5 => Reply::json(200, "{}").with_body(Body::FailAfter(vec![concord::bytes::Bytes::from(
            format!("{{\"incomplete\":\"{echo}"),
        )])),
        7 => Reply::json(200, problem),
        _ => unreachable!(),
    };
    response
        .header("x-request-id", echo)
        .header("set-cookie", format!("session={echo}"))
}

fn success(r: &Recorded, search_next: &str, category_next: &str) -> String {
    let hit = r#"{"pageid":2,"ns":0,"title":"Rust language"}"#;
    match param(r, "list").as_deref() {
        Some("search") if param(r, "sroffset").is_none() => format!(
            r#"{{"continue":{{"sroffset":"{search_next}","continue":"-||"}},"query":{{"search":[{hit}]}}}}"#
        ),
        Some("search") => format!(r#"{{"query":{{"search":[{hit}]}}}}"#),
        Some(_) if param(r, "cmcontinue").is_none() => format!(
            r#"{{"continue":{{"cmcontinue":"{category_next}","continue":"-||"}},"query":{{"categorymembers":[{hit}]}}}}"#
        ),
        Some(_) => format!(r#"{{"query":{{"categorymembers":[{hit}]}}}}"#),
        None => r#"{"query":{"pages":[{"pageid":2,"ns":0,"title":"Rust language","revisions":[{"revid":42,"slots":{"main":{"content":"text"}}}]}]}}"#.into(),
    }
}

async fn receive<T>(
    c: &mut Canaries,
    prepared: concord::Prepared<T>,
) -> Result<(), concord::Error> {
    let result = prepared.receive().await;
    c.assert_debug_absent("Received Result", &result);
    let received = result?;
    c.assert_debug_absent("response head", &received.head());
    c.assert_debug_absent("response metadata", &received.meta());
    received.decode().await.map(drop)
}

fn concord_error(c: &mut Canaries, error: &concord::Error) {
    c.assert_debug_absent("concord::Error", error);
    c.assert_error_absent("concord::Error and sources", error);
    if let Some(meta) = error.head_meta() {
        c.assert_debug_absent("error response metadata", &meta);
    }
}

fn error(c: &mut Canaries, error: &Error) {
    c.assert_debug_absent("SDK error", error);
    c.assert_error_absent("SDK error and sources", error);
    match error {
        Error::Concord(e) => concord_error(c, e),
        Error::Http(failure) => {
            c.assert_debug_absent("HTTP failure", failure);
            c.assert_debug_absent("HTTP failure metadata", &failure.meta);
            c.assert_debug_absent("error document", &failure.document);
            match &failure.document {
                Ok(document) => c.assert_debug_absent("API problem", &document.error),
                Err(e) => concord_error(c, e),
            }
        }
        Error::Api { meta, problem } => {
            c.assert_debug_absent("API error metadata", meta);
            c.assert_debug_absent("API problem", problem);
        }
        _ => {}
    }
}

#[tokio::test]
async fn secrets_never_print() {
    let mut c = Canaries::new("mediawiki-action-api");
    let user_agent = format!("CanaryReader/1.0 (contact: {}@example.test)", c.issue());
    let code = c.issue();
    let info = c.issue();
    let search_next = c.issue();
    let category_next = c.issue();
    let problem = format!(r#"{{"error":{{"code":"{code}","info":"{info}"}}}}"#);
    let config = MediaWikiConfig {
        user_agent: user_agent.clone(),
    };
    c.assert_debug_absent("config", &config);
    let ids = PageIds::new([2, 2], PageIdLimit::Standard).unwrap();
    c.assert_debug_absent("page ids", &ids);
    let cap = NonZeroUsize::new(3).unwrap();
    for case in 0..8 {
        let (echo, problem) = (user_agent.clone(), problem.clone());
        let (next_a, next_b) = (search_next.clone(), category_next.clone());
        let server = TestServer::start(move |r: &Recorded| {
            reply(case, success(r, &next_a, &next_b), &problem, &echo)
        })
        .await;
        let wiki = api(
            &if case == 6 { refused() } else { server.url() },
            &user_agent,
        );
        let query = wiki.query();
        let read = query.search("Rust language").set_srlimit(5);
        c.assert_debug_absent("client", &wiki);
        c.assert_debug_absent("query scope", &query);
        c.assert_debug_absent("search endpoint", &read);
        let prepared = read.clone().prepare().unwrap();
        c.assert_debug_absent("search Prepared", &prepared);
        let native = receive(&mut c, prepared).await;
        c.assert_debug_absent("native Result", &native);
        assert_eq!(native.is_ok(), matches!(case, 0 | 7));
        if let Err(e) = &native {
            concord_error(&mut c, e);
        }
        let result = read.send().await;
        c.assert_debug_absent("search Result", &result);
        assert_eq!(result.is_ok(), case == 0);
        match result {
            Ok(response) => {
                c.assert_debug_absent("search response", &response);
                c.assert_debug_absent("search hits", &response.data);
                c.assert_debug_absent("search metadata", &response.meta);
                let next = response.continuation.unwrap();
                c.assert_debug_absent("continuation", &next);
                c.assert_display_absent("continuation", &next);
                assert_eq!(next.fields()["sroffset"], search_next);
                let continued = query.search("Rust language").set_continuation(next);
                c.assert_debug_absent("continued search endpoint", &continued);
                let last = continued.send().await.unwrap();
                c.assert_debug_absent("last search response", &last);
                assert!(last.continuation.is_none());
            }
            Err(e) => {
                error(&mut c, &e);
                if case == 1 {
                    // Withholding keeps diagnosis: the status and the operation stay visible.
                    assert!(e.to_string().contains("403"), "{e}");
                    let Error::Http(failure) = &e else {
                        panic!("{e:?}")
                    };
                    let operation = failure.meta.operation().unwrap();
                    assert_eq!(operation.name(), "MediaWiki.query.search");
                    assert_eq!(failure.document.as_ref().unwrap().error.info, info);
                }
                if case == 3 {
                    let Error::Concord(inner) = &e else {
                        panic!("{e:?}")
                    };
                    assert_eq!(inner.endpoint(), Some("MediaWiki.query.search"));
                }
                if case == 7 {
                    assert!(matches!(&e, Error::Api { problem, .. } if problem.code == code));
                }
            }
        }
        let page = query.fetch_page(2).await;
        c.assert_debug_absent("fetch_page Result", &page);
        assert_eq!(page.is_ok(), case == 0);
        if let Err(e) = &page {
            error(&mut c, e);
        }
        let pages = query.fetch_pages(&ids).await;
        c.assert_debug_absent("fetch_pages Result", &pages);
        assert_eq!(pages.is_ok(), case == 0);
        if let Err(e) = &pages {
            error(&mut c, e);
        }
        let mut traversal = query.category_pages("Category:Rust", 1, None, cap);
        c.assert_debug_absent("category traversal", &traversal);
        loop {
            match traversal.next_page().await {
                Ok(Some(page)) => {
                    assert_eq!(case, 0);
                    c.assert_debug_absent("category page", &page);
                    c.assert_debug_absent("category cursor", &page.next);
                }
                Ok(None) => break,
                Err(e) => {
                    c.assert_debug_absent("PagesError", &e);
                    c.assert_display_absent("PagesError", &e);
                    c.assert_error_absent("PagesError and sources", &e);
                    error(&mut c, e.provider_error().unwrap());
                    break;
                }
            }
        }
        c.assert_debug_absent("finished traversal", &traversal);
        if case == 0 {
            let requests = server.requests();
            c.assert_on_wire_except(&requests, &[&code, &info]);
            for r in &requests {
                assert_eq!(r.header("user-agent"), Some(user_agent.as_str()));
            }
        }
        server.shutdown().await;
    }
    assert!(c.checked() >= 250);
    println!(
        "mediawiki-action-api: {} renderings; positive control passed (2 response-only canaries exempt)",
        c.checked()
    );
}

#[tokio::test]
async fn refused_user_agent_never_prints() {
    let mut c = Canaries::new("mediawiki-action-api");
    let user_agent = format!("CanaryReader/1.0\n{}", c.issue());
    let wiki = api("http://127.0.0.1:9", &user_agent);
    c.assert_debug_absent("client with invalid User-Agent", &wiki);
    let build = wiki.query().search("Rust").prepare().unwrap_err();
    concord_error(&mut c, &build);
    let e = wiki.query().search("Rust").send().await.unwrap_err();
    error(&mut c, &e);
}
