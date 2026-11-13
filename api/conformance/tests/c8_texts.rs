//! C8.4 and C6.2: the texts of docs/messages.md section 2 (with ANSWERS.md A15, A19, A33,
//! A36 and A37) that a paged call can print, each through `try_next`, each compared as a
//! whole line, and each followed by the end of the pages.
//!
//! What a paged endpoint of `api/tour` cannot meet is not here, and is named in the
//! comment of `c8_4_the_texts_that_pages_can_produce`.

mod common;

use common::engine::*;
use common::stream;
use concord::error::Class;
use concord::MIB;
use concord_testkit::Script;

/// The first error of the pages, as its printed line; then the pages must be over.
async fn first_line<T: Send, E: Send>(mut pages: concord::Pages<T, E>) -> Result<String, String> {
    loop {
        match pages.try_next().await {
            Ok(Some(_)) => continue,
            Ok(None) => return Err("the end, and no error".to_owned()),
            Err(error) => {
                let line = error.to_string();
                return match pages.try_next().await {
                    Ok(None) => Ok(line),
                    Ok(Some(_)) => Err(format!("{line:?}, then a page")),
                    Err(again) => Err(format!("{line:?}, then {again}")),
                };
            }
        }
    }
}

/// A cell: the pages that `make` gives print `expected` as their first error.
fn text<T, E, F>(row: &'static str, expected: &'static str, make: F) -> Cell
where
    T: Send + 'static,
    E: Send + 'static,
    F: std::future::Future<Output = concord::Pages<T, E>> + Send + 'static,
{
    cell(row, "printed text", async move {
        let line = first_line(make.await).await?;
        check(line == expected, || format!("{line:?}"))
    })
}

/// A page of `Feed` with a cursor, so that a second page is asked.
const FEED_MORE: &str = r#"{"data":[{"id":"a","name":"a"}],"next_cursor":"b"}"#;

/// The next try answers `200` with this JSON body.
fn give(script: &Script, body: &'static str) {
    script
        .answer(200)
        .header("content-type", "application/json")
        .body(body);
}

/// `Feed` over this script.
fn feed(script: &Script) -> tour::Feed {
    tour::Feed::new().with_transport(script.clone())
}

#[tokio::test(start_paused = true)]
async fn c8_4_the_texts_that_pages_can_produce() {
    // Not here, because no paged endpoint of `api/tour` has the shape that prints them:
    // row 4 (a struct in a query), 5 (a number that is not finite), 7 (a body that could
    // not be written), 13 (a reader that holds the server to its label), 17 (a `HEAD`),
    // 25 (a `content-length` written by the description), A15 (an entry whose expression
    // fails), and A19's arm that refused the answer. Row 8 (`request()` on a stream) is
    // not a page: `.pages()` never calls `request()`. Row 22 (a connection refused) needs
    // a socket: `c8_4_a_connection_that_was_refused_ends_the_pages`.
    let mut cells = Vec::new();

    // 2. A host hole.
    cells.push(text(
        "2. Invalid host hole",
        "build error: input tenant is not one label of a host, in shapes.roaming",
        async {
            let script = Script::new();
            shapes(&script).roaming("a.b").pages()
        },
    ));
    // 3. An empty path value.
    cells.push(text(
        "3. An empty path value",
        "build error: input path_key gives an empty or dot path segment, in shapes.guarded",
        async {
            let script = Script::new();
            shapes(&script).guarded("").pages()
        },
    ));
    // 6. A header value.
    cells.push(text(
        "6. Invalid final header value",
        "build error: the value of header x-cursor is not a legal header value, in shapes.headed",
        async {
            let script = Script::new();
            shapes(&script).headed().cursor("a\nb").pages()
        },
    ));
    // 9. A one-shot body at the second page.
    cells.push(text(
        "9. A body that is a one-shot stream, sent a second time",
        "build error: the body is a stream that was already sent, in shapes.streamed",
        async {
            let script = Script::new();
            script
                .answer(200)
                .read_upload()
                .header("content-type", "application/json")
                .body(r#"{"items":[],"next":"b"}"#);
            shapes(&script).streamed(stream("abc")).pages()
        },
    ));
    // 10. A middleware that will not send.
    cells.push(text(
        "10. A middleware or a transport that will not send",
        "transport error: blocked before it was sent, in feed.list",
        async {
            let script = Script::new();
            feed(&script)
                .with_middleware(Blocks::default())
                .list()
                .pages()
        },
    ));
    // 11. A status with no arm, at the second page.
    cells.push(text(
        "11. A status with no arm, not a 2xx",
        "refused: status 404, in feed.list",
        async {
            let script = Script::new();
            give(&script, FEED_MORE);
            script.answer(404);
            feed(&script).list().pages()
        },
    ));
    // 12. After tries.
    cells.push(text(
        "12. The same, after tries",
        "refused: status 429, in riot.matches.ids, after 3 tries",
        async {
            let script = Script::new();
            for _ in 0..3 {
                script.answer(429);
            }
            riot(&script)
                .with_retry(concord::Retry::standard())
                .matches(tour::Europe)
                .ids(tour::Puuid::from("p"))
                .pages()
        },
    ));
    // 14. Larger than the limit.
    cells.push(text(
        "14. A body larger than its limit",
        "mismatch: status 200, the body is larger than the limit of the reader, in feed.list",
        async {
            let script = Script::new();
            script
                .answer(200)
                .header("content-type", "application/json")
                .declare_length(10 * MIB as u64 + 1)
                .stall();
            feed(&script).list().pages()
        },
    ));
    // 15. A body that does not decode, with its place.
    cells.push(text(
        "15. A body that does not decode",
        "mismatch: status 200, data[0].name is not what the type says, in feed.list",
        async {
            let script = Script::new();
            give(
                &script,
                r#"{"data":[{"id":"a","name":5}],"next_cursor":null}"#,
            );
            feed(&script).list().pages()
        },
    ));
    // 16. And its label was not the reader's.
    cells.push(text(
        "16. A body that does not read, when its label was not the reader's either",
        "mismatch: status 200, the body is not what the type says and its content type is not the reader's, in feed.list",
        async {
            let script = Script::new();
            script
                .answer(200)
                .header("content-type", "text/html")
                .body("<html></html>");
            feed(&script).list().pages()
        },
    ));
    // A19 (30): at the root, with the reader's label.
    cells.push(text(
        "A19. A Mismatch at the root with the reader's label",
        "mismatch: status 200, the body is not what the type says, in feed.list",
        async {
            let script = Script::new();
            give(&script, "<html></html>");
            feed(&script).list().pages()
        },
    ));
    // 18. A body that breaks.
    cells.push(text(
        "18. A body that breaks or ends early",
        "transport error: the body broke after status 200, in feed.list",
        async {
            let script = Script::new();
            script
                .answer(200)
                .header("content-type", "application/json")
                .body("{")
                .cut();
            feed(&script).list().pages()
        },
    ));
    // 19. A pager that refuses a page.
    cells.push(text(
        "19. A pager that refuses a page",
        "mismatch: status 200, the next page cannot be asked, in shapes.guarded",
        async {
            let script = Script::new();
            give(&script, r#"{"items":[],"more":true}"#);
            shapes(&script).guarded("pk").pages()
        },
    ));
    // 19. Or asks for the request that was just sent.
    cells.push(text(
        "19. A pager that asks for the request that was just sent",
        "mismatch: status 200, the next page cannot be asked, in feed.list",
        async {
            let script = Script::new();
            give(&script, FEED_MORE);
            give(&script, FEED_MORE);
            feed(&script).list().pages()
        },
    ));
    // 20. A pager that fails with the API's problem.
    cells.push(text(
        "20. A pager that fails with the API's problem",
        "refused: status 200, in shapes.guarded",
        async {
            let script = Script::new();
            give(
                &script,
                r#"{"items":[],"refusal":{"code":"stop","message":"no"}}"#,
            );
            shapes(&script).guarded("pk").pages()
        },
    ));
    // 21. Another origin.
    cells.push(text(
        "21. A page that leaves the origin of the first request",
        "build error: the URL is at another origin than the call, in shapes.roaming",
        async {
            let script = Script::new();
            give(&script, r#"{"items":[],"next":"other"}"#);
            shapes(&script).roaming("first").pages()
        },
    ));
    // 23. Silence before a head.
    cells.push(text(
        "23. Silence for longer than idle_timeout",
        "transport error: no answer for 30 s (with_idle_timeout changes it), in feed.list",
        async {
            let script = Script::new();
            script.never();
            feed(&script).list().pages()
        },
    ));
    // 24. The whole call.
    cells.push(text(
        "24. The whole call longer than timeout",
        "transport error: the call took longer than 60 s (with_timeout), in feed.list",
        async {
            let script = Script::new();
            script.never();
            feed(&script)
                .with_timeout(secs(60))
                .with_idle_timeout(secs(600))
                .list()
                .pages()
        },
    ));
    // A19 (27 to 29): what a transport says.
    for (row, class, expected) in [
        (
            "A19. Class Other",
            Class::Other,
            "transport error: the transport failed, in feed.list",
        ),
        (
            "A19. A timeout that a transport reported",
            Class::Timeout,
            "transport error: a timeout was reported by the transport, in feed.list",
        ),
        (
            "A19. A body that broke with no head known",
            Class::Body,
            "transport error: the body broke, in feed.list",
        ),
    ] {
        cells.push(text(row, expected, async move {
            tour::Feed::new()
                .with_transport(FailsWith(class))
                .list()
                .pages()
        }));
    }

    run_cells("the texts of pages", cells).await;
}

#[tokio::test(start_paused = true)]
async fn c8_4_the_texts_of_a33_a36_and_a37_through_pages() {
    // ANSWERS.md A33: "After a head, while a body is read: `transport error: the body was
    // silent for 30 s after status 200 (<hint>), in <endpoint>`." A36: "The label sentence
    // [...] is printed only when the answer had a `Content-Type`." A37: "A `with_base` that
    // is no base: `build error: the base is not a URL that can be used (with_base), in
    // <endpoint>`."
    let mut cells = Vec::new();
    // 1, with A37: a `with_base` that is no base.
    cells.push(text(
        "1. Invalid base (with_base)",
        "build error: the base is not a URL that can be used (with_base), in feed.list",
        async {
            let script = Script::new();
            feed(&script).with_base("not a url").list().pages()
        },
    ));
    // A36: a body of no bytes.
    cells.push(text(
        "A36. A body of no bytes that the reader refuses",
        "mismatch: status 204, the body is empty, in feed.list",
        async {
            let script = Script::new();
            script.answer(204);
            feed(&script).list().pages()
        },
    ));
    // A33: silence while a body is read.
    cells.push(text(
        "A33. A body that was silent after its head",
        "transport error: the body was silent for 30 s after status 200 (with_idle_timeout changes it), in feed.list",
        async {
            let script = Script::new();
            script
                .answer(200)
                .header("content-type", "application/json")
                .body("{")
                .stall();
            feed(&script).list().pages()
        },
    ));

    run_cells("the texts of pages (A33, A36, A37)", cells).await;
}

#[tokio::test]
async fn c8_4_a_connection_that_was_refused_ends_the_pages() {
    // Row 22 of the texts, through a page.
    let (_bound, port) = refusing_port();
    let http = concord::Http::with(|builder| builder.no_proxy()).unwrap();
    let pages = tour::Feed::new()
        .with_transport(http)
        .with_base(format!("http://127.0.0.1:{port}"))
        .list()
        .pages();

    assert_eq!(
        first_line(pages).await.unwrap(),
        "transport error: connection refused, in feed.list"
    );
}
