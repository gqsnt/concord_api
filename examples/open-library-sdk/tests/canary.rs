//! No credential: the client has no authentication configuration (`src/lib.rs:23`, README:127).
//! Secrets: the `#[sensitive]` search text `q` of `search.works` and of `search.authors`
//! (`src/lib.rs:35,42`; README:123 "generated request and `Prepared` Debug redact it"), each with
//! its own canary, sent as the `q` query value. Withheld by the SDK (README:123-125,
//! `src/error.rs:17,80-96`): error documents, response header values and parser sources; the
//! server echoes the works canary into the 4xx/5xx JSON document, into headers (`x-private`,
//! `set-cookie`, `retry-after`) and into every malformed, wrong-media and truncated body.
//! `search.works` is the representative read (the SDK has no writes): it covers the seven
//! scenarios through the SDK `send()` and the native `Prepared`, with `search.authors`,
//! `works.get` and `author.works` sent beside it on the same replies. An identifier refused by
//! `WorkId::parse`/`AuthorId::parse` ("never prints the input", README:102) gets its own canary
//! in a separate test.
//! Not covered: native reqwest values; explicit accessors (`HttpError::document_unredacted`,
//! `meta().headers()`, Concord's native response/body/source accessors); success models
//! (`WorkSearch`, `Work`, ...), whose derived `Debug` exposes data by design (README:126), so the
//! success bodies carry no canary; valid `WorkId`/`AuthorId`, which are public identifiers and
//! printed. No wire exemptions.

use concord_check_canary::Canaries;
use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use std::{num::NonZeroU32, time::Duration};

use concord_sdk_open_library::{AuthorId, Error, OpenLibrary, WorkId, WorkSort};

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
    concord_example_testserver::refused_url()
}

// Echo the canary into headers and into every body whose content the SDK withholds.
fn reply(case: usize, success: &str, problem: &str, echo: &str) -> Reply {
    let response = match case {
        0 | 6 => Reply::json(200, success),
        1 => Reply::json(404, problem),
        2 => Reply::json(503, problem),
        3 => Reply::json(200, format!("{{invalid {echo}")),
        4 => Reply::text(200, "text/html", echo),
        5 => Reply::json(200, "{}").with_body(Body::FailAfter(vec![concord::bytes::Bytes::from(
            format!("{{\"incomplete\":\"{echo}"),
        )])),
        _ => unreachable!(),
    };
    response
        .header("x-private", echo)
        .header("set-cookie", echo)
        .header("retry-after", echo)
}

async fn receive<T, D: 'static>(
    c: &mut Canaries,
    prepared: concord::Prepared<T, D>,
) -> Result<T, concord::Error<D>> {
    let result = prepared.receive().await;
    c.assert_debug_absent("Received Result", &result);
    match result {
        Ok(received) => {
            c.assert_debug_absent("response head", &received.head());
            c.assert_debug_absent("response metadata", &received.meta());
            received.decode().await
        }
        Err(error) => Err(error.widen()),
    }
}

fn concord_error<D: 'static>(c: &mut Canaries, error: &concord::Error<D>) {
    c.assert_debug_absent("concord::Error", error);
    c.assert_error_absent("concord::Error and sources", error);
    c.assert_debug_absent("error kind", &error.kind());
    if let Some(meta) = error.head_meta() {
        c.assert_debug_absent("error response metadata", &meta);
    }
}

fn error(c: &mut Canaries, error: &Error) {
    c.assert_debug_absent("SDK error", error);
    c.assert_error_absent("SDK error and sources", error);
    c.assert_debug_absent("error status", &error.status());
    match error {
        Error::Concord(e) => concord_error(c, e),
        Error::Http(http) => {
            c.assert_debug_absent("HTTP error", http);
            c.assert_error_absent("HTTP error and sources", http.as_ref());
            c.assert_debug_absent("HTTP error metadata", http.meta());
            if let Err(e) = http.document_unredacted() {
                concord_error(c, e);
            }
        }
        Error::InvalidId(kind) => c.assert_debug_absent("record kind", kind),
        _ => {}
    }
}

fn sent<T>(c: &mut Canaries, label: &str, result: &Result<T, Error>) {
    // Success models print data by design; only the failure is formatted.
    if let Err(e) = result {
        c.assert_debug_absent(label, e);
        error(c, e);
    }
}

#[tokio::test]
async fn secrets_never_print() {
    let mut c = Canaries::new("open-library");
    let works_q = c.issue();
    let authors_q = c.issue();
    let work = WorkId::parse("/works/OL27448W").unwrap();
    let author = AuthorId::parse("OL23919A").unwrap();
    for case in 0..7 {
        let echo = works_q.clone();
        let server = TestServer::start(move |r: &Recorded| {
            let success = match r.path() {
                "/search.json" => r#"{"numFound":1,"start":0,"numFoundExact":true,"docs":[{"key":"/works/OL27448W","title":"The Lord of the Rings"}]}"#,
                "/search/authors.json" => r#"{"numFound":1,"start":0,"docs":[{"key":"OL23919A","name":"J. K. Rowling","work_count":162}]}"#,
                "/works/OL27448W.json" => r#"{"key":"/works/OL27448W","title":"The Lord of the Rings"}"#,
                _ => r#"{"size":1,"entries":[{"key":"/works/OL82563W"}]}"#,
            };
            let problem = format!(r#"{{"error":"{echo}","detail":{{"token":"{echo}"}}}}"#);
            reply(case, success, &problem, &echo)
        })
        .await;
        let base = if case == 6 { refused() } else { server.url() };
        let api = OpenLibrary::new(http()).with_base_url(&base).unwrap();
        let search = api.search();
        let read = search
            .works(works_q.clone())
            .set_page(NonZeroU32::new(2).unwrap())
            .set_sort(WorkSort::New);
        let authors = search.authors(authors_q.clone());
        let works = api.works();
        let get = works.get(work.clone());
        let scope = api.author(author.clone());
        let author_works = scope.works().set_limit(NonZeroU32::new(2).unwrap());
        c.assert_debug_absent("client", &api);
        c.assert_debug_absent("search scope", &search);
        c.assert_debug_absent("works scope", &works);
        c.assert_debug_absent("author scope", &scope);
        c.assert_debug_absent("works search endpoint", &read);
        c.assert_debug_absent("authors search endpoint", &authors);
        c.assert_debug_absent("work endpoint", &get);
        c.assert_debug_absent("author works endpoint", &author_works);
        c.assert_debug_absent(
            "authors search Prepared",
            &authors.clone().prepare().unwrap(),
        );
        c.assert_debug_absent("work Prepared", &get.clone().prepare().unwrap());
        let prepared = read.clone().prepare().unwrap();
        c.assert_debug_absent("works search Prepared", &prepared);
        let native = receive(&mut c, prepared).await;
        assert_eq!(native.is_ok(), case == 0);
        if let Err(e) = &native {
            concord_error(&mut c, e);
        }

        let result = read.send().await;
        assert_eq!(result.is_ok(), case == 0);
        sent(&mut c, "works search Result", &result);
        match (case, &result) {
            (0, Ok(page)) => assert_eq!(page.num_found, Some(1)),
            // Safe diagnosis survives: the status, and the document on purpose.
            (1, Err(e @ Error::Http(http))) => {
                assert_eq!(e.to_string(), "Open Library HTTP 404");
                assert_eq!(e.status().map(|s| s.as_u16()), Some(404));
                let document = http.document_unredacted().unwrap();
                assert_eq!(document["error"], works_q.as_str());
            }
            (3, Err(e @ Error::Concord(source))) => {
                assert_eq!(source.kind(), concord::ErrorKind::Decode);
                assert_eq!(e.status().map(|s| s.as_u16()), Some(200));
            }
            (6, Err(Error::Concord(source))) => {
                assert_eq!(source.kind(), concord::ErrorKind::Transport);
            }
            (n, other) => assert!(matches!(n, 2 | 4 | 5), "case {n}: {other:?}"),
        }
        let result = authors.send().await;
        assert_eq!(result.is_ok(), case == 0);
        sent(&mut c, "authors search Result", &result);
        let result = get.send().await;
        assert_eq!(result.is_ok(), case == 0);
        sent(&mut c, "work Result", &result);
        let result = author_works.send().await;
        assert_eq!(result.is_ok(), case == 0);
        sent(&mut c, "author works Result", &result);
        if case == 0 {
            let requests = server.requests();
            c.assert_on_wire(&requests);
            let searches: Vec<_> = requests.iter().flat_map(|r| r.query_all("q")).collect();
            assert!(searches.contains(&works_q) && searches.contains(&authors_q));
        }
        server.shutdown().await;
    }
    assert!(c.checked() >= 150);
    println!(
        "open-library: {} renderings; positive control passed",
        c.checked()
    );
}

#[test]
fn refused_identifiers_never_print() {
    let mut c = Canaries::new("open-library");
    let input = c.issue();
    for e in [
        WorkId::parse(&input).unwrap_err(),
        WorkId::parse(&format!("/works/OL{input}W")).unwrap_err(),
        AuthorId::parse(&format!("OL1{input}")).unwrap_err(),
    ] {
        error(&mut c, &e);
        c.assert_display_absent("refused identifier", &e);
    }
}
