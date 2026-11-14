//! C8, pages: the machine of `.pages()` (docs/internals/engine.md, "Pages"), the checks of the origin
//! and of "the same request", the shipped pagers, `at_url`, `.items()`, and a secret and a
//! `base` (docs/reference/contracts.md C8.2 to C8.9; docs/reference/rules.md rule 17; table 8 of
//! docs/reference/cases.md, one test or one cell for each row).
//!
//! Everything goes through the public surface: the paged endpoints of `tests/tour` (`riot`'s
//! `matches.ids`, `feed`, and the paged endpoints of `Shapes`), `.pages()`, `try_next`, the
//! `Stream` form, `.items()`, and a `concord_testkit::Script`. The texts that a paged call
//! can print are in `c8_texts.rs`.

mod common;

use common::engine::*;
use common::{header, sensitive, stream};
use concord::error::Kind;
use concord::page::Call;
use concord::transport::{TransportError, TryInfo};
use concord::{http, reqwest, Middleware, Next, Retry, Secret};
use concord_testkit::Script;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::time::Instant;
use tour::{Batch, Item, ShapesProblem};

// What the tests share ----------------------------------------------------------------

/// A page of `Shapes`: the items with these identifiers, and the cursor or the address of
/// the next page.
fn batch(ids: &[u64], next: Option<&str>) -> String {
    let items: Vec<String> = ids
        .iter()
        .map(|id| format!(r#"{{"id":{id},"title":"t{id}"}}"#))
        .collect();
    match next {
        Some(next) => format!(r#"{{"items":[{}],"next":"{next}"}}"#, items.join(",")),
        None => format!(r#"{{"items":[{}]}}"#, items.join(",")),
    }
}

/// A page of `Feed`: entries with these names, and the next cursor.
fn feed_page(names: &[&str], next: Option<&str>) -> String {
    let data: Vec<String> = names
        .iter()
        .map(|name| format!(r#"{{"id":"{name}","name":"{name}"}}"#))
        .collect();
    let next = next.map_or("null".to_owned(), |next| format!(r#""{next}""#));
    format!(r#"{{"data":[{}],"next_cursor":{next}}}"#, data.join(","))
}

/// The next try of the script answers `200` with this JSON body.
fn give(script: &Script, body: String) {
    script
        .answer(200)
        .header("content-type", "application/json")
        .body(body);
}

/// The identifiers of the items of a page.
fn ids(page: &Batch) -> Vec<u64> {
    page.items.iter().map(|item| item.id).collect()
}

/// The page that `try_next` gives, or the test fails with the line of the error.
#[track_caller]
fn page<T, E>(next: Result<Option<T>, concord::Error<E>>) -> T {
    match next {
        Ok(Some(page)) => page,
        Ok(None) => panic!("expected a page, got the end"),
        Err(error) => panic!("expected a page, got: {error}"),
    }
}

/// The error that `try_next` gives, or the test fails.
#[track_caller]
fn error<T, E>(next: Result<Option<T>, concord::Error<E>>) -> concord::Error<E> {
    match next {
        Ok(Some(_)) => panic!("expected an error, got a page"),
        Ok(None) => panic!("expected an error, got the end"),
        Err(error) => error,
    }
}

/// The end that `try_next` gives, or the test fails.
#[track_caller]
fn end<T, E>(next: Result<Option<T>, concord::Error<E>>) {
    match next {
        Ok(None) => {}
        Ok(Some(_)) => panic!("expected the end, got a page"),
        Err(error) => panic!("expected the end, got: {error}"),
    }
}

/// The URLs of every request that the script was given.
fn urls(script: &Script) -> Vec<String> {
    script
        .requests()
        .iter()
        .map(|request| request.url().to_owned())
        .collect()
}

/// `build error: the URL is at another origin than the call, in <endpoint>`.
fn another_origin(endpoint: &str) -> String {
    format!("build error: the URL is at another origin than the call, in {endpoint}")
}

/// `mismatch: status 200, the next page cannot be asked, in <endpoint>`.
fn cannot_be_asked(endpoint: &str) -> String {
    format!("mismatch: status 200, the next page cannot be asked, in {endpoint}")
}

/// `Shapes` over this script.
fn shapes_on(script: &Script) -> tour::Shapes {
    shapes(script)
}

/// `Feed` over this script.
fn feed(script: &Script) -> tour::Feed {
    tour::Feed::new().with_transport(script.clone())
}

/// A middleware of the caller that writes the number of the request it sees in `x-seen`.
#[derive(Debug, Default)]
struct Stamp(AtomicUsize);

impl Middleware for Stamp {
    async fn handle(
        &self,
        mut request: reqwest::Request,
        info: TryInfo,
        next: Next<'_>,
    ) -> Result<reqwest::Response, TransportError> {
        let seen = self.0.fetch_add(1, Ordering::SeqCst) + 1;
        request.headers_mut().insert(
            "x-seen",
            http::HeaderValue::from_str(&seen.to_string()).expect("a number"),
        );
        next.send(request, info).await
    }
}

/// A middleware of the caller that replaces a secret while the first try is on its way.
#[derive(Debug)]
struct Replaces(Secret);

impl Middleware for Replaces {
    async fn handle(
        &self,
        request: reqwest::Request,
        info: TryInfo,
        next: Next<'_>,
    ) -> Result<reqwest::Response, TransportError> {
        if info.number() == 1 {
            self.0.replace("new");
        }
        next.send(request, info).await
    }
}

/// An `edit_request` that writes a header whose value changes with each request, marked
/// sensitive or not.
fn counted_header(
    sensitive: bool,
) -> impl Fn(&mut reqwest::Request) + Send + Sync + 'static + Clone {
    let count = Arc::new(AtomicUsize::new(0));
    move |request: &mut reqwest::Request| {
        let n = count.fetch_add(1, Ordering::SeqCst) + 1;
        let mut value = http::HeaderValue::from_str(&format!("v{n}")).expect("a value");
        value.set_sensitive(sensitive);
        request.headers_mut().insert("x-edit", value);
    }
}

/// An `edit_request` that sends every request to another host.
fn moved_to(host: &'static str) -> impl Fn(&mut reqwest::Request) + Send + Sync + 'static {
    move |request: &mut reqwest::Request| {
        request.url_mut().set_host(Some(host)).expect("a host");
    }
}

// C8.4: when a page is asked for -------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c8_4_nothing_is_sent_before_the_first_try_next() {
    let script = Script::new();
    give(&script, batch(&[1], None));
    let shapes = shapes_on(&script);

    let mut pages = shapes.guarded("pk").pages();
    tokio::task::yield_now().await;
    assert!(script.requests().is_empty(), "`.pages()` sends nothing");

    assert_eq!(ids(&page(pages.try_next().await)), [1]);
    assert_eq!(script.requests().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn c8_4_table_8_none_ends_with_the_try_next_after_the_last_page() {
    let script = Script::new();
    give(&script, batch(&[1], Some("b")));
    give(&script, batch(&[2], None));
    let mut pages = shapes_on(&script).guarded("pk").pages();

    assert_eq!(ids(&page(pages.try_next().await)), [1]);
    assert_eq!(ids(&page(pages.try_next().await)), [2]);
    end(pages.try_next().await);
    end(pages.try_next().await);
    assert_eq!(script.requests().len(), 2, "the end sends nothing");
    assert!(script.requests()[1].url().ends_with("cursor=b"));
}

#[tokio::test(start_paused = true)]
async fn c8_4_table_8_the_request_of_a_page_is_built_when_it_is_asked_for() {
    // A `Secret` replaced between two pages is used: in a piece of the path, a pair of the
    // query and a header (the next call moves by its cursor).
    let script = Script::new();
    give(&script, batch(&[1], Some("b")));
    give(&script, batch(&[2], None));
    let (path, query, header) = (Secret::new("p1"), Secret::new("q1"), Secret::new("h1"));
    let mut pages = shapes_on(&script)
        .guarded(&path)
        .query_key(&query)
        .header_key(&header)
        .pages();

    page(pages.try_next().await);
    path.replace("p2");
    query.replace("q2");
    header.replace("h2");
    page(pages.try_next().await);

    let requests = script.requests();
    assert_eq!(
        requests[0].url(),
        "https://shapes.example.com/guarded/p1?key=q1"
    );
    assert_eq!(requests[0].header("x-key"), Some("h1"));
    assert_eq!(
        requests[1].url(),
        "https://shapes.example.com/guarded/p2?key=q2&cursor=b"
    );
    assert_eq!(requests[1].header("x-key"), Some("h2"));
    assert!(requests[1].headers()["x-key"].is_sensitive());
}

#[tokio::test(start_paused = true)]
async fn c8_4_table_8_the_page_that_arrived_is_given_first_and_the_pagers_problem_after() {
    // A failure with the API's problem: a Refused error with that problem and the status of
    // the page that came. Then the pages are over.
    let script = Script::new();
    give(
        &script,
        r#"{"items":[{"id":1,"title":"t1"}],"refusal":{"code":"stop","message":"no more"}}"#
            .to_owned(),
    );
    let mut pages = shapes_on(&script).guarded("pk").pages();

    assert_eq!(ids(&page(pages.try_next().await)), [1]);
    let refused = error(pages.try_next().await);
    assert_eq!(refused.kind(), Kind::Refused);
    assert!(refused.is_status(200));
    assert_eq!(refused.problem().map(|p| p.code.as_str()), Some("stop"));
    assert_eq!(
        refused.to_string(),
        "refused: status 200, in shapes.guarded"
    );
    end(pages.try_next().await);
    assert_eq!(script.requests().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn c8_4_table_8_a_mismatch_of_the_pager_comes_after_the_page() {
    let script = Script::new();
    give(
        &script,
        r#"{"items":[{"id":1,"title":"t1"}],"more":true}"#.to_owned(),
    );
    let mut pages = shapes_on(&script).guarded("pk").pages();

    assert_eq!(ids(&page(pages.try_next().await)), [1]);
    let mismatch = error(pages.try_next().await);
    assert_eq!(mismatch.kind(), Kind::Mismatch);
    assert_eq!(mismatch.reason(), Some("more items, and no cursor"));
    assert_eq!(mismatch.to_string(), cannot_be_asked("shapes.guarded"));
    end(pages.try_next().await);
    end(pages.try_next().await);
}

#[tokio::test(start_paused = true)]
async fn c8_4_after_an_error_of_a_page_the_pages_are_over() {
    let script = Script::new();
    give(&script, batch(&[1], Some("b")));
    script.answer(404);
    give(&script, batch(&[3], None));
    let mut pages = shapes_on(&script).guarded("pk").pages();

    page(pages.try_next().await);
    let refused = error(pages.try_next().await);
    assert!(refused.is_status(404));
    end(pages.try_next().await);
    end(pages.try_next().await);
    assert_eq!(script.requests().len(), 2);
    assert_eq!(script.unused(), 1, "nothing is asked after the error");
}

// C8.4: the same request twice ---------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c8_4_table_8_the_same_request_again_is_a_mismatch_and_nothing_is_sent() {
    let script = Script::new();
    give(&script, batch(&[1], Some("a")));
    give(&script, batch(&[2], Some("a")));
    give(&script, batch(&[3], None));
    let mut pages = shapes_on(&script).guarded("pk").pages();

    page(pages.try_next().await); // no cursor; the next has `a`
    page(pages.try_next().await); // cursor `a`; the next has `a` again
    let mismatch = error(pages.try_next().await);
    assert_eq!(mismatch.kind(), Kind::Mismatch);
    assert_eq!(mismatch.to_string(), cannot_be_asked("shapes.guarded"));
    end(pages.try_next().await);
    assert_eq!(script.requests().len(), 2, "the same request is not sent");
}

#[tokio::test(start_paused = true)]
async fn c8_4_the_fragment_of_a_next_url_is_left_out_of_the_same_request() {
    let script = Script::new();
    script
        .answer(200)
        .header("link", "</linked-items#part>; rel=\"next\"")
        .header("content-type", "application/json")
        .body(feed_page(&["a"], None));
    give(&script, feed_page(&["b"], None));
    let mut pages = feed(&script).linked().pages();

    page(pages.try_next().await);
    assert_eq!(
        error(pages.try_next().await).to_string(),
        cannot_be_asked("feed.linked")
    );
    end(pages.try_next().await);
    assert_eq!(script.requests().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn c8_4_table_8_a_header_whose_value_holds_no_secret_moves() {
    // An API that pages by a header.
    let script = Script::new();
    give(&script, batch(&[1], Some("b")));
    give(&script, batch(&[2], Some("b")));
    let mut pages = shapes_on(&script).headed().cursor("a").pages();

    page(pages.try_next().await);
    page(pages.try_next().await);
    assert_eq!(
        error(pages.try_next().await).to_string(),
        cannot_be_asked("shapes.headed")
    );
    let requests = script.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].url(), requests[1].url());
    assert_eq!(requests[0].header("x-cursor"), Some("a"));
    assert_eq!(requests[1].header("x-cursor"), Some("b"));
}

#[tokio::test(start_paused = true)]
async fn c8_4_table_8_a_secret_replaced_in_a_header_a_pair_or_a_path_is_not_a_move() {
    let mut cells = Vec::new();
    for place in ["header", "query pair", "path"] {
        cells.push(cell(
            "A call that differs only by a Secret that was replaced",
            place,
            async move {
                let script = Script::new();
                give(&script, batch(&[1], Some("a")));
                give(&script, batch(&[2], None));
                let key = Secret::new("old");
                let shapes = shapes_on(&script);
                let call = match place {
                    "header" => shapes.guarded("pk").header_key(&key),
                    "query pair" => shapes.guarded("pk").query_key(&key),
                    _ => shapes.guarded(&key),
                };
                let mut pages = call.cursor("a").pages();
                page(pages.try_next().await);
                key.replace("new");
                let line = match pages.try_next().await {
                    Err(error) => error.to_string(),
                    Ok(Some(_)) => return Err("a page was asked".to_owned()),
                    Ok(None) => return Err("the end".to_owned()),
                };
                check(line == cannot_be_asked("shapes.guarded"), || {
                    format!("{line:?}")
                })?;
                check(matches!(pages.try_next().await, Ok(None)), || {
                    "the pages go on".to_owned()
                })?;
                check(script.requests().len() == 1, || "it was sent".to_owned())
            },
        ));
    }
    run_cells("table 8", cells).await;
}

#[tokio::test(start_paused = true)]
async fn c8_4_table_8_a_secret_replaced_in_the_host_is_another_origin() {
    let script = Script::new();
    give(&script, batch(&[1], Some("a")));
    give(&script, batch(&[2], None));
    let key = Secret::new("k1");
    let mut pages = shapes_on(&script).keyed_host(&key).cursor("a").pages();

    page(pages.try_next().await);
    key.replace("k2");
    let built = error(pages.try_next().await);
    assert_eq!(built.kind(), Kind::Build);
    assert_eq!(built.to_string(), another_origin("shapes.keyed_host"));
    end(pages.try_next().await);
    assert_eq!(
        urls(&script),
        ["https://k1.shapes.example.com/keyed?cursor=a"]
    );
}

#[tokio::test(start_paused = true)]
async fn c8_4_table_8_a_body_is_compared_as_its_bytes() {
    // A secret in an entry of the body, replaced between two pages, looks like a move, once.
    let script = Script::new();
    give(&script, batch(&[1], Some("a")));
    give(&script, batch(&[2], Some("a")));
    give(&script, batch(&[3], None));
    let token = Secret::new("t1");
    let mut pages = shapes_on(&script)
        .posted()
        .token(&token)
        .cursor("a")
        .pages();

    page(pages.try_next().await);
    token.replace("t2");
    page(pages.try_next().await);
    assert_eq!(
        error(pages.try_next().await).to_string(),
        cannot_be_asked("shapes.posted")
    );
    let requests = script.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].url(), requests[1].url());
    assert_eq!(
        requests[0].body(),
        Some(&br#"{"token":"t1","cursor":"a"}"#[..])
    );
    assert_eq!(
        requests[1].body(),
        Some(&br#"{"token":"t2","cursor":"a"}"#[..])
    );
}

#[tokio::test(start_paused = true)]
async fn c8_4_the_same_bytes_of_a_body_and_another_cursor_is_a_move() {
    // `streamed` with a body in memory: the bytes are the same, and the query moves.
    let script = Script::new();
    give(&script, batch(&[1], Some("b")));
    give(&script, batch(&[2], Some("b")));
    let mut pages = shapes_on(&script)
        .streamed(concord::Body::new("abc", "text/plain"))
        .cursor("a")
        .pages();

    page(pages.try_next().await);
    page(pages.try_next().await);
    assert_eq!(
        error(pages.try_next().await).to_string(),
        cannot_be_asked("shapes.streamed")
    );
    let requests = script.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].body(), Some(&b"abc"[..]));
    assert_eq!(requests[1].body(), Some(&b"abc"[..]));
}

#[tokio::test(start_paused = true)]
async fn c8_4_table_8_after_an_edit_that_changed_the_url_the_url_is_compared_as_it_is() {
    let script = Script::new();
    give(&script, batch(&[1], Some("a")));
    give(&script, batch(&[2], None));
    let key = Secret::new("p1");
    let mut pages = shapes_on(&script)
        .guarded(&key)
        .cursor("a")
        .edit_request(|request| {
            request
                .url_mut()
                .query_pairs_mut()
                .append_pair("edited", "1");
        })
        .pages();

    page(pages.try_next().await);
    key.replace("p2");
    page(pages.try_next().await);
    end(pages.try_next().await);
    assert_eq!(
        urls(&script),
        [
            "https://shapes.example.com/guarded/p1?cursor=a&edited=1",
            "https://shapes.example.com/guarded/p2?cursor=a&edited=1",
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn c8_4_table_8_a_value_that_edit_request_marks_sensitive_is_left_out() {
    let mut cells = Vec::new();
    for sensitive in [true, false] {
        cells.push(cell(
            "A call that differs only by a header that edit_request wrote",
            if sensitive {
                "marked sensitive"
            } else {
                "not marked"
            },
            async move {
                let script = Script::new();
                give(&script, batch(&[1], Some("a")));
                give(&script, batch(&[2], None));
                let mut pages = shapes_on(&script)
                    .guarded("pk")
                    .cursor("a")
                    .edit_request(counted_header(sensitive))
                    .pages();
                page(pages.try_next().await);
                let second = pages.try_next().await;
                if sensitive {
                    let line = match second {
                        Err(error) => error.to_string(),
                        Ok(_) => return Err("not a Mismatch".to_owned()),
                    };
                    check(line == cannot_be_asked("shapes.guarded"), || {
                        format!("{line:?}")
                    })?;
                    check(script.requests().len() == 1, || "it was sent".to_owned())
                } else {
                    check(matches!(second, Ok(Some(_))), || "no page".to_owned())?;
                    let requests = script.requests();
                    check(requests.len() == 2, || "not sent".to_owned())?;
                    check(requests[1].header("x-edit") == Some("v2"), || {
                        "the edit ran once for each page".to_owned()
                    })
                }
            },
        ));
    }
    run_cells("table 8", cells).await;
}

#[tokio::test(start_paused = true)]
async fn c8_4_table_8_what_a_middleware_changes_is_not_looked_at() {
    let script = Script::new();
    give(&script, batch(&[1], Some("a")));
    give(&script, batch(&[2], None));
    let mut pages = shapes_on(&script)
        .with_middleware(Stamp::default())
        .guarded("pk")
        .cursor("a")
        .pages();

    page(pages.try_next().await);
    assert_eq!(
        error(pages.try_next().await).to_string(),
        cannot_be_asked("shapes.guarded")
    );
    let requests = script.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].header("x-seen"), Some("1"));
}

#[tokio::test(start_paused = true)]
async fn c8_4_table_8_a_body_made_anew_for_each_send_is_never_the_same_request() {
    let script = Script::new();
    script
        .answer(200)
        .read_upload()
        .header("content-type", "application/json")
        .body(batch(&[1], Some("a")));
    script
        .answer(200)
        .read_upload()
        .header("content-type", "application/json")
        .body(batch(&[2], None));
    let mut pages = shapes_on(&script).renewed().cursor("a").pages();

    page(pages.try_next().await);
    page(pages.try_next().await);
    end(pages.try_next().await);
    let requests = script.requests();
    assert_eq!(requests.len(), 2, "the same method, URL and headers: sent");
    assert_eq!(requests[0].url(), requests[1].url());
    assert_eq!(requests[0].upload().bytes(), b"fresh");
    assert_eq!(requests[1].upload().bytes(), b"fresh");
}

#[tokio::test(start_paused = true)]
async fn c8_4_table_8_a_one_shot_body_fails_at_the_second_page() {
    let script = Script::new();
    script
        .answer(200)
        .read_upload()
        .header("content-type", "application/json")
        .body(batch(&[1], Some("b")));
    give(&script, batch(&[2], None));
    let mut pages = shapes_on(&script).streamed(stream("abc")).pages();

    assert_eq!(ids(&page(pages.try_next().await)), [1]);
    let built = error(pages.try_next().await);
    assert_eq!(built.kind(), Kind::Build);
    assert_eq!(
        built.to_string(),
        "build error: the body is a stream that was already sent, in shapes.streamed"
    );
    end(pages.try_next().await);
    let requests = script.requests();
    assert_eq!(requests.len(), 1, "nothing of the second page is sent");
    assert_eq!(requests[0].upload().bytes(), b"abc");
}

// C8.4: the origin -----------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c8_4_table_8_a_pager_that_moves_the_host_is_another_origin() {
    let script = Script::new();
    give(&script, batch(&[1], Some("other")));
    give(&script, batch(&[2], None));
    let mut pages = shapes_on(&script).roaming("first").pages();

    page(pages.try_next().await);
    let built = error(pages.try_next().await);
    assert_eq!(built.to_string(), another_origin("shapes.roaming"));
    end(pages.try_next().await);
    assert_eq!(urls(&script), ["https://first.shapes.example.com/roaming"]);
}

#[tokio::test(start_paused = true)]
async fn c8_4_table_8_an_edit_that_moves_every_request_keeps_the_pages_there() {
    // The origin of the pages is the one of the first request as it was finished.
    let script = Script::new();
    give(&script, batch(&[1], Some("b")));
    give(&script, batch(&[2], None));
    let mut pages = shapes_on(&script)
        .guarded("pk")
        .edit_request(moved_to("other.example.com"))
        .pages();

    page(pages.try_next().await);
    page(pages.try_next().await);
    end(pages.try_next().await);
    assert_eq!(
        urls(&script),
        [
            "https://other.example.com/guarded/pk",
            "https://other.example.com/guarded/pk?cursor=b",
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn c8_4_at_url_is_checked_on_its_own_before_the_edits() {
    // The pages are held to the origin of their first request (after the edit), and
    // `at_url` to the origin that the call builds (before the edit): two checks, apart.
    let mut cells = Vec::new();
    for (row, link, sent) in [
        (
            "A next URL at the origin that the call builds",
            "<https://feed.example.com/linked-items?page=2>; rel=\"next\"",
            true,
        ),
        (
            "A next URL at the origin of the first request, after the edit",
            "<https://other.example.com/linked-items?page=2>; rel=\"next\"",
            false,
        ),
    ] {
        cells.push(cell(row, "the second page", async move {
            let script = Script::new();
            script
                .answer(200)
                .header("link", link)
                .header("content-type", "application/json")
                .body(feed_page(&["a"], None));
            give(&script, feed_page(&["b"], None));
            let mut pages = feed(&script)
                .linked()
                .edit_request(moved_to("other.example.com"))
                .pages();
            page(pages.try_next().await);
            let second = pages.try_next().await;
            if sent {
                check(matches!(second, Ok(Some(_))), || "no page".to_owned())?;
                check(
                    urls(&script)[1] == "https://other.example.com/linked-items?page=2",
                    || format!("{:?}", urls(&script)),
                )
            } else {
                let line = match second {
                    Err(error) => error.to_string(),
                    Ok(_) => return Err("not an error".to_owned()),
                };
                check(line == another_origin("feed.linked"), || {
                    format!("{line:?}")
                })?;
                check(script.requests().len() == 1, || "it was sent".to_owned())
            }
        }));
    }
    run_cells("table 8", cells).await;
}

#[tokio::test(start_paused = true)]
async fn c8_4_table_8_a_call_given_a_url_at_another_origin_fails_at_the_first_page() {
    let script = Script::new();
    give(&script, feed_page(&["a"], None));
    let mut pages = feed(&script)
        .linked()
        .at_url("https://elsewhere.example.com/linked-items")
        .pages();

    let built = error(pages.try_next().await);
    assert_eq!(built.kind(), Kind::Build);
    assert_eq!(built.to_string(), another_origin("feed.linked"));
    end(pages.try_next().await);
    assert!(script.requests().is_empty(), "nothing is sent");
}

#[tokio::test(start_paused = true)]
async fn c8_4_table_8_a_link_to_another_origin_is_a_build_error() {
    let script = Script::new();
    script
        .answer(200)
        .header("link", "<https://elsewhere.example.com/x>; rel=\"next\"")
        .header("content-type", "application/json")
        .body(feed_page(&["a"], None));
    give(&script, feed_page(&["b"], None));
    let mut pages = feed(&script).linked().pages();

    page(pages.try_next().await);
    assert_eq!(
        error(pages.try_next().await).to_string(),
        another_origin("feed.linked")
    );
    end(pages.try_next().await);
    assert_eq!(script.requests().len(), 1);
}

// C8.4: a page and its items are `Send`; the `Stream` form ------------------------------

#[test]
fn c8_4_a_page_its_items_and_the_futures_of_try_next_are_send() {
    fn send<T: Send>(_: &T) {}
    let shapes = tour::Shapes::new();
    let mut pages: concord::Pages<Batch, ShapesProblem> = shapes.guarded("pk").pages();
    send(&pages);
    send(&pages.try_next());
    let mut items: concord::page::Items<Item, ShapesProblem> = shapes.guarded("pk").pages().items();
    send(&items);
    send(&items.try_next());
    fn sync<T: Sync>(_: &T) {}
    sync(&pages);
}

#[tokio::test(start_paused = true)]
async fn c8_4_the_stream_form_polls_the_same_state_and_ends_after_an_error() {
    use futures_core::Stream;
    use std::pin::Pin;

    let script = Script::new();
    give(&script, batch(&[1], Some("b")));
    give(&script, batch(&[2], Some("c")));
    script.answer(500);
    let mut pages = shapes_on(&script).guarded("pk").pages();

    // One page by `try_next`, the next by the `Stream`, then the error, then the end.
    assert_eq!(ids(&page(pages.try_next().await)), [1]);
    let next = std::future::poll_fn(|cx| Pin::new(&mut pages).poll_next(cx)).await;
    assert_eq!(ids(&next.expect("a page").expect("a page")), [2]);
    let next = std::future::poll_fn(|cx| Pin::new(&mut pages).poll_next(cx)).await;
    assert!(next.expect("an error").unwrap_err().is_status(500));
    let next = std::future::poll_fn(|cx| Pin::new(&mut pages).poll_next(cx)).await;
    assert!(next.is_none(), "the stream ended");
    end(pages.try_next().await);
    assert_eq!(script.requests().len(), 3);
}

// C8.5: what is in flight ----------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c8_5_a_try_next_that_is_dropped_loses_nothing() {
    let script = Script::new();
    script
        .answer(200)
        .after(secs(5))
        .header("content-type", "application/json")
        .body(batch(&[1], None));
    let mut pages = shapes_on(&script).guarded("pk").pages();

    tokio::select! {
        biased;
        _ = pages.try_next() => panic!("the answer comes after 5 s"),
        () = tokio::time::sleep(SECOND) => {}
    }
    assert_eq!(script.requests().len(), 1, "the request is in flight");

    let start = Instant::now();
    assert_eq!(ids(&page(pages.try_next().await)), [1]);
    assert_eq!(start.elapsed(), secs(4), "the same exchange went on");
    end(pages.try_next().await);
    assert_eq!(script.requests().len(), 1, "nothing is sent twice");
}

#[tokio::test(start_paused = true)]
async fn c8_5_each_page_is_one_call_with_the_retry_policy_of_the_call() {
    let script = Script::new();
    give(&script, r#"["a","b"]"#.to_owned());
    script.answer(503);
    give(&script, r#"["c"]"#.to_owned());
    let mut items = riot(&script)
        .matches(tour::Europe)
        .ids(tour::Puuid::from("p"))
        .count(2)
        .with_retry(Retry::standard())
        .pages()
        .items();

    let mut got = Vec::new();
    while let Some(id) = items.try_next().await.unwrap() {
        got.push(id.to_string());
    }
    assert_eq!(got, ["a", "b", "c"]);
    let requests = script.requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(
        requests[1].url(),
        requests[2].url(),
        "the second page, tried again"
    );
    assert_eq!(requests[2].info().number(), 2);
}

#[tokio::test(start_paused = true)]
async fn c8_5_the_timeout_of_a_page_starts_when_it_is_asked_for() {
    let script = Script::new();
    give(&script, batch(&[1], Some("b")));
    script
        .answer(200)
        .after(secs(5))
        .header("content-type", "application/json")
        .body(batch(&[2], Some("c")));
    script.answer(200).after(secs(15)).body(batch(&[3], None));
    let mut pages = shapes_on(&script)
        .guarded("pk")
        .with_timeout(secs(10))
        .pages();

    page(pages.try_next().await);
    // The time between two pages is not counted.
    tokio::time::sleep(secs(60)).await;
    page(pages.try_next().await);
    let late = error(pages.try_next().await);
    assert!(late.is_timeout());
    assert_eq!(
        late.to_string(),
        "transport error: the call took longer than 10 s (with_timeout), in shapes.guarded"
    );
    end(pages.try_next().await);
}

// C8.6: the shipped pagers, and `at_url` -------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c8_6_by_offset_grows_by_the_items_that_came_and_ends_on_an_empty_page() {
    let script = Script::new();
    give(
        &script,
        r#"[{"id":1,"title":"a"},{"id":2,"title":"b"}]"#.to_owned(),
    );
    give(&script, r#"[{"id":3,"title":"c"}]"#.to_owned());
    give(&script, "[]".to_owned());
    let mut items = shapes_on(&script).offset().pages().items();

    let mut got = Vec::new();
    while let Some(item) = items.try_next().await.unwrap() {
        got.push(item.id);
    }
    assert_eq!(got, [1, 2, 3]);
    assert_eq!(
        urls(&script),
        [
            "https://shapes.example.com/offset",
            "https://shapes.example.com/offset?start=2",
            "https://shapes.example.com/offset?start=3",
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn c8_6_an_offset_that_its_type_cannot_hold_is_a_mismatch() {
    let script = Script::new();
    give(
        &script,
        r#"[{"id":1,"title":"a"},{"id":2,"title":"b"}]"#.to_owned(),
    );
    let mut pages = shapes_on(&script).offset().start(u32::MAX - 1).pages();

    page(pages.try_next().await);
    let mismatch = error(pages.try_next().await);
    assert_eq!(mismatch.kind(), Kind::Mismatch);
    assert_eq!(mismatch.to_string(), cannot_be_asked("shapes.offset"));
    end(pages.try_next().await);
    assert_eq!(script.requests().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn c8_6_by_offset_limit_also_ends_on_a_page_shorter_than_the_count() {
    let mut cells = Vec::new();
    for (row, count, answers, expected) in [
        (
            "count 2: a full page, then a short one",
            Some(2),
            vec![r#"["a","b"]"#, r#"["c"]"#],
            vec!["by-puuid/p/ids?count=2", "by-puuid/p/ids?start=2&count=2"],
        ),
        (
            "no count: an empty page ends",
            None,
            vec![r#"["a"]"#, "[]"],
            vec!["by-puuid/p/ids", "by-puuid/p/ids?start=1"],
        ),
    ] {
        cells.push(cell(row, "the requests", async move {
            let script = Script::new();
            for answer in answers {
                give(&script, answer.to_owned());
            }
            let mut call = riot(&script)
                .matches(tour::Europe)
                .ids(tour::Puuid::from("p"));
            if let Some(count) = count {
                call = call.count(count);
            }
            let mut items = call.pages().items();
            while items.try_next().await.map_err(|e| e.to_string())?.is_some() {}
            let got: Vec<String> = urls(&script)
                .iter()
                .map(|url| {
                    url.trim_start_matches("https://europe.api.riotgames.com/lol/match/v5/matches/")
                        .to_owned()
                })
                .collect();
            check(got == expected, || format!("{got:?}"))?;
            check(script.unused() == 0, || "an answer is left".to_owned())
        }));
    }
    run_cells("by_offset_limit", cells).await;
}

#[tokio::test(start_paused = true)]
async fn c8_6_by_page_asks_the_next_number_and_ends_on_an_empty_page() {
    let mut cells = Vec::new();
    for (row, first, expected) in [
        (
            "no page given: the page that was answered was number 1",
            None,
            [
                "https://shapes.example.com/numbered",
                "https://shapes.example.com/numbered?page=2",
            ],
        ),
        (
            "page 5 given",
            Some(5),
            [
                "https://shapes.example.com/numbered?page=5",
                "https://shapes.example.com/numbered?page=6",
            ],
        ),
    ] {
        cells.push(cell(row, "the requests", async move {
            let script = Script::new();
            give(&script, r#"[{"id":1,"title":"a"}]"#.to_owned());
            give(&script, "[]".to_owned());
            let mut call = shapes_on(&script).numbered();
            if let Some(first) = first {
                call = call.page(first);
            }
            let mut pages = call.pages();
            while pages.try_next().await.map_err(|e| e.to_string())?.is_some() {}
            check(urls(&script) == expected, || format!("{:?}", urls(&script)))
        }));
    }
    run_cells("by_page", cells).await;
}

#[tokio::test(start_paused = true)]
async fn c8_6_by_cursor_never_looks_at_the_length_of_a_page() {
    let script = Script::new();
    give(&script, feed_page(&[], Some("x")));
    give(&script, feed_page(&["a"], Some("y")));
    give(&script, feed_page(&[], None));
    let mut items = feed(&script).by_cursor().pages().items();

    assert_eq!(items.try_next().await.unwrap().unwrap().name, "a");
    assert!(items.try_next().await.unwrap().is_none());
    assert_eq!(
        urls(&script),
        [
            "https://feed.example.com/cursor-items",
            "https://feed.example.com/cursor-items?starting_after=x",
            "https://feed.example.com/cursor-items?starting_after=y",
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn c8_6_by_link_reads_the_link_field_as_rfc_8288_says() {
    let mut cells = Vec::new();
    for (row, field, outcome) in [
        (
            "A link whose relations hold next",
            r#"<https://feed.example.com/linked-items?page=2>; rel="prev next""#,
            Some("https://feed.example.com/linked-items?page=2"),
        ),
        (
            "A ; and a , inside a quoted string end nothing",
            r#"</a>; title="x;y, z"; rel="next", </b>; rel="last""#,
            Some("https://feed.example.com/a"),
        ),
        (
            "A link with an anchor is not taken",
            r#"</a>; rel="next"; anchor="/other""#,
            None,
        ),
        (
            "A target that a strict grammar refuses is read as reqwest reads it",
            r#"</a|b>; rel="next""#,
            Some("https://feed.example.com/a|b"),
        ),
        (
            "A field that cannot be read is a Mismatch, not the end",
            r#"</a; rel="next""#,
            Some("mismatch"),
        ),
    ] {
        cells.push(cell(row, "the next request", async move {
            let script = Script::new();
            script
                .answer(200)
                .header("link", field)
                .header("content-type", "application/json")
                .body(feed_page(&["a"], None));
            give(&script, feed_page(&["b"], None));
            let mut pages = feed(&script).linked().pages();
            page(pages.try_next().await);
            let second = pages.try_next().await;
            match outcome {
                None => check(matches!(second, Ok(None)), || "not the end".to_owned()),
                Some("mismatch") => {
                    let line = match second {
                        Err(error) => error.to_string(),
                        Ok(_) => return Err("not an error".to_owned()),
                    };
                    check(line == cannot_be_asked("feed.linked"), || {
                        format!("{line:?}")
                    })
                }
                Some(url) => {
                    check(matches!(second, Ok(Some(_))), || "no page".to_owned())?;
                    check(urls(&script)[1] == url, || format!("{:?}", urls(&script)))
                }
            }
        }));
    }
    run_cells("by_link", cells).await;
}

#[tokio::test(start_paused = true)]
async fn c8_6_table_8_by_next_url_resolves_a_relative_url_against_the_answer() {
    let mut cells = Vec::new();
    for (row, next, expected) in [
        (
            "An absolute URL at the origin",
            "https://shapes.example.com/addressed?page=2",
            Ok("https://shapes.example.com/addressed?page=2"),
        ),
        (
            "A relative URL, from an answer, with a fragment",
            "more/2?k=v#part",
            Ok("https://shapes.example.com/more/2?k=v"),
        ),
        (
            "A URL at another origin",
            "https://elsewhere.example.com/addressed",
            Err(another_origin("shapes.addressed")),
        ),
    ] {
        cells.push(cell(row, "the second page", async move {
            let script = Script::new();
            give(&script, batch(&[1], Some(next)));
            give(&script, batch(&[2], None));
            let mut pages = shapes_on(&script).addressed().pages();
            page(pages.try_next().await);
            let second = pages.try_next().await;
            match expected {
                Ok(url) => {
                    check(matches!(second, Ok(Some(_))), || "no page".to_owned())?;
                    check(urls(&script)[1] == url, || format!("{:?}", urls(&script)))
                }
                Err(line) => {
                    let got = match second {
                        Err(error) => error.to_string(),
                        Ok(_) => return Err("not an error".to_owned()),
                    };
                    check(got == line, || format!("{got:?}"))?;
                    check(script.requests().len() == 1, || "it was sent".to_owned())
                }
            }
        }));
    }
    run_cells("table 8", cells).await;
}

#[tokio::test(start_paused = true)]
async fn c8_6_table_8_a_next_call_whose_request_does_not_build_is_its_build_error() {
    // A URL that does not parse is a Build error when the call is sent.
    let script = Script::new();
    give(&script, batch(&[1], Some("https://[::1")));
    let mut pages = shapes_on(&script).addressed().pages();

    page(pages.try_next().await);
    let built = error(pages.try_next().await);
    assert_eq!(built.kind(), Kind::Build);
    end(pages.try_next().await);
    assert_eq!(script.requests().len(), 1);
}

#[test]
fn c8_6_table_8_a_callers_own_at_url_is_resolved_against_the_url_that_the_call_builds() {
    let shapes = tour::Shapes::new();
    // A relative URL, and its fragment dropped.
    let request = shapes
        .guarded("pk")
        .at_url("../other/x?y=1#part")
        .request()
        .unwrap();
    assert_eq!(
        request.url().as_str(),
        "https://shapes.example.com/other/x?y=1"
    );
    // Its path and its query replace the ones of the description.
    let request = shapes
        .guarded("pk")
        .cursor("c")
        .at_url("/elsewhere?z=2")
        .request()
        .unwrap();
    assert_eq!(
        request.url().as_str(),
        "https://shapes.example.com/elsewhere?z=2"
    );
    // Another origin, and a URL that does not parse: Build errors, outside pages too.
    let built = failure(
        shapes
            .guarded("pk")
            .at_url("https://elsewhere.example.com/x")
            .request(),
    );
    assert_eq!(built.to_string(), another_origin("shapes.guarded"));
    let built = failure(shapes.guarded("pk").at_url("https://[::1").request());
    assert_eq!(built.kind(), Kind::Build);
    // `with_base` is the origin that the call builds.
    let request = shapes
        .clone()
        .with_base("http://localhost:8080/api")
        .guarded("pk")
        .at_url("http://localhost:8080/api/x")
        .request()
        .unwrap();
    assert_eq!(request.url().as_str(), "http://localhost:8080/api/x");
}

#[test]
fn c8_2_c8_6_a_secret_pair_is_written_once_on_a_next_url_and_no_other_pair_is_added() {
    let request = tour::Shapes::new()
        .guarded("pk")
        .query_key("secret")
        .cursor("c")
        .at_url("/guarded/pk?key=first&x=1&key=second")
        .request()
        .unwrap();
    let pairs: Vec<(String, String)> = request
        .url()
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    assert_eq!(
        pairs.iter().filter(|(k, _)| k == "key").collect::<Vec<_>>(),
        [&("key".to_owned(), "secret".to_owned())],
        "every pair of that key is taken out, and the secret is written once"
    );
    assert!(pairs.contains(&("x".to_owned(), "1".to_owned())));
    assert!(
        !pairs.iter().any(|(k, _)| k == "cursor"),
        "a pair of the description is not added again"
    );
}

// C8.7: `.items()` ------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c8_7_items_go_over_several_pages_and_an_empty_one() {
    let script = Script::new();
    give(&script, batch(&[1, 2], Some("b")));
    give(&script, batch(&[], Some("c")));
    give(&script, batch(&[3], None));
    let mut items = shapes_on(&script).guarded("pk").pages().items();

    let mut got = Vec::new();
    while let Some(item) = items.try_next().await.unwrap() {
        got.push(item.id);
    }
    assert_eq!(got, [1, 2, 3]);
    assert_eq!(script.requests().len(), 3);
    assert!(items.try_next().await.unwrap().is_none());
}

#[tokio::test(start_paused = true)]
async fn c8_7_items_give_an_error_in_the_middle_once_and_then_end() {
    let script = Script::new();
    give(&script, batch(&[1, 2], Some("b")));
    script.answer(503);
    let mut items = shapes_on(&script).guarded("pk").pages().items();

    assert_eq!(items.try_next().await.unwrap().unwrap().id, 1);
    assert_eq!(
        script.requests().len(),
        1,
        "the next page is asked when one is used up"
    );
    assert_eq!(items.try_next().await.unwrap().unwrap().id, 2);
    let refused = items.try_next().await.unwrap_err();
    assert!(refused.is_status(503));
    assert!(items.try_next().await.unwrap().is_none());
    assert!(items.try_next().await.unwrap().is_none());
}

// C8.8: a paged call is not awaited ------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c8_8_send_gives_one_page_on_purpose() {
    let script = Script::new();
    give(&script, batch(&[1], Some("b")));
    give(&script, batch(&[2], None));

    let first = shapes_on(&script).guarded("pk").send().await.unwrap();
    assert_eq!(ids(&first), [1]);
    assert_eq!(first.next.as_deref(), Some("b"));
    assert_eq!(script.requests().len(), 1);
}

// C8.3 and C8.9: a secret ----------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c8_3_a_request_reads_a_secret_once_and_a_later_try_sends_the_same() {
    let script = Script::new();
    script.answer(503);
    give(&script, r#"{"id":"EUW1_1"}"#.to_owned());
    let key = Secret::new("old");
    tour::Riot::new(&key)
        .with_transport(script.clone())
        .with_middleware(Replaces(key.clone()))
        .with_retry(Retry::standard())
        .matches(tour::Europe)
        .get(tour::MatchId::from("EUW1_1"))
        .await
        .unwrap();

    let requests = script.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].header("x-riot-token"), Some("old"));
    assert_eq!(requests[1].header("x-riot-token"), Some("old"));
    assert_eq!(key.expose().to_string(), "new");
}

#[test]
fn c8_9_a_key_on_the_client_goes_to_the_base_of_each_scope() {
    let riot = tour::Riot::new("key");
    let ids = riot
        .matches(tour::Asia)
        .ids(tour::Puuid::from("p"))
        .request()
        .unwrap();
    let account = riot
        .account(tour::Europe)
        .by_riot_id("Faker", "KR1")
        .request()
        .unwrap();
    for request in [&ids, &account] {
        assert_eq!(header(request, "x-riot-token"), ["key"]);
        assert!(sensitive(request, "x-riot-token"));
    }
    // An endpoint that removes it, and writes its own.
    let me = riot.account(tour::Europe).me("token").request().unwrap();
    assert!(header(&me, "x-riot-token").is_empty());
    assert_eq!(header(&me, "authorization"), ["Bearer token"]);
    // `with_base` is the caller's own word: everything follows it.
    let moved = riot
        .clone()
        .with_base("http://localhost:8080")
        .matches(tour::Asia)
        .get(tour::MatchId::from("A_1"))
        .request()
        .unwrap();
    assert_eq!(
        moved.url().as_str(),
        "http://localhost:8080/lol/match/v5/matches/A_1"
    );
    assert_eq!(header(&moved, "x-riot-token"), ["key"]);
}
