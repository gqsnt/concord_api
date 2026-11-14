//! A whole client, nine behaviours of a made-up bookshop, with no
//! socket. The description is in `api.rs`; this file uses it, and prints what went out.
//!
//! Run it with `cargo run --example bookshop`. Its `Cargo.toml` lines, in a project of yours:
//!
//! ```toml
//! [dependencies]
//! concord-api = { git = "https://github.com/gqsnt/concord_api" }
//! serde = { version = "1", features = ["derive"] }
//! tokio = { version = "1", features = ["macros", "rt", "test-util"] }
//!
//! # Here for the example; in a project, under [dev-dependencies], for the tests.
//! concord-testkit = { git = "https://github.com/gqsnt/concord_api" }
//! # `test-util` enables `start_paused` for this example's virtual time.
//! ```
#![allow(clippy::print_stdout)] // An example prints what it shows.

mod api;
mod server;

use api::{bookshop, Bookshop};
use concord::{bytes::Bytes, Retry, Secret};
use concord_testkit::{script::Request, Script};
use server::{answer, wire};
use std::time::Duration;
use tokio::time::Instant;

const BOOK: &str = r#"{ "isbn": "978-0", "title": "Kindred", "year": 1979 }"#;
const FIRST: &str = r#"{ "items": [{ "isbn": "978-0", "title": "Kindred" }], "next": "c 2" }"#;
const LAST: &str = r#"{ "items": [], "next": null }"#;
const NO_BOOK: &str = r#"{ "code": "not_found", "message": "no such book" }"#;
const BUSY: &str = r#"{ "code": "busy", "message": "later" }"#;
const BAD_ISBN: &str = r#"{ "code": "bad_isbn", "message": "an ISBN has 13 digits" }"#;

// `start_paused`: Tokio's clock moves only when everything waits. A wait of 2 s takes no time.
#[tokio::main(flavor = "current_thread", start_paused = true)]
async fn main() -> bookshop::Result<()> {
    // The script plays the server: each try gets the next answer written on it.
    let script = Script::new();

    // Defaults for every call are settings of the client, not statements of the description:
    // 5 s for a whole call, and the standard retry (at most 3 tries, only what may be repeated).
    // A real program leaves `with_transport` out: it then sends with `concord::Http`.
    let shop = Bookshop::new("eu", Secret::new("k-123"))
        .with_timeout(Duration::from_secs(5))
        .with_retry(Retry::standard())
        .with_transport(script.clone());
    let books = shop.books();
    let mut seen = 0;

    println!("1. a book, and no book");
    answer(&script, 200, BOOK);
    answer(&script, 404, NO_BOOK);
    println!("   {:?}", books.get("978-0").await?.map(|book| book.title));
    println!("   {:?}", books.get("978-9").await?.map(|book| book.title));
    seen = wire(&script, seen);

    println!("2. pages, then items");
    answer(&script, 200, FIRST);
    answer(&script, 200, LAST);
    let mut pages = books
        .list("Octavia Butler", ["sf", "classic"])
        .year(1979u16)
        .pages();
    while let Some(page) = pages.try_next().await? {
        println!("   a page of {} book(s)", page.items.len());
    }
    // An empty list of tags, and a `year` not set, write nothing in the query.
    answer(&script, 200, FIRST);
    answer(&script, 200, LAST);
    let mut items = books
        .list("Octavia Butler", Vec::<String>::new())
        .pages()
        .items();
    while let Some(book) = items.try_next().await? {
        println!("   {} ({:?})", book.title, book.year);
    }
    seen = wire(&script, seen);

    println!("3. a creation: the body, and a header");
    script
        .answer(201)
        .header("location", "/v2/books/978-1")
        .body(BOOK);
    let created = books.create("Kindred", "Octavia Butler").await?;
    let book = created.book;
    println!("   {} ({}) at {}", book.title, book.isbn, created.location);
    seen = wire(&script, seen);

    println!("4. an upload, with 60 s for this call only");
    // The clock for silence (30 s by default) is another clock: a slower server needs
    // `.with_idle_timeout(…)` on the call too.
    script.answer(204).after(Duration::from_secs(12));
    let start = Instant::now();
    let png = Bytes::from_static(b"\x89PNG...");
    books
        .upload_cover("978-1", png)
        .with_timeout(Duration::from_secs(60))
        .await?;
    println!("   done after {:?}", start.elapsed());
    if let Some(sent) = script.requests().last() {
        let kind = sent.header("content-type").unwrap_or_default();
        println!("   {kind}, {} bytes", sent.body().map_or(0, <[u8]>::len));
    }
    seen = wire(&script, seen);

    println!("5. a DELETE is sent again; a POST is not");
    answer(&script, 503, BUSY);
    script.answer(204);
    books.delete("978-1").await?;
    answer(&script, 503, BUSY);
    if let Err(error) = books.create("Dawn", "Octavia Butler").year(1987u16).await {
        println!("   the POST: {error}, after {} try", error.tries());
    }
    seen = wire(&script, seen);

    println!("6. the API's problem, and a 429 that is waited for");
    answer(&script, 400, BAD_ISBN);
    if let Err(error) = books.get("1").await {
        // `Display` shows names, never values; the problem is the API's own body.
        match error.problem() {
            Some(problem) if problem.code == "bad_isbn" => {
                println!("   {error}: {} ({})", problem.code, problem.message);
            }
            _ => return Err(error),
        }
    }
    script.answer(429).header("retry-after", "2").body("{}");
    answer(&script, 200, BOOK);
    let start = Instant::now();
    books.get("978-0").await?;
    println!("   a book after {:?} of waiting", start.elapsed());
    // A wait longer than what is left of the 5 s is not started: the 429 is the result.
    script.answer(429).header("retry-after", "10").body("{}");
    if let Err(error) = books.get("978-0").await {
        println!("   {error}, retry-after {:?}", error.retry_after());
    }
    seen = wire(&script, seen);

    println!("7. 5 s for a whole call");
    script.answer(200).after(Duration::from_secs(8)).body(BOOK);
    let start = Instant::now();
    if let Err(error) = books.get("978-0").await {
        println!(
            "   {error}: timeout {}, after {:?}",
            error.is_timeout(),
            start.elapsed()
        );
    }
    seen = wire(&script, seen);

    println!("8. an export, as a stream of bytes");
    // The call ends at the head; its 5 s still run while the stream is read.
    let piece = vec![b'x'; 1024 * 1024];
    script
        .answer(200)
        .body(piece.clone())
        .body(piece.clone())
        .body(piece);
    let mut export = shop.export().await?;
    let mut total = 0;
    while let Some(chunk) = export.try_next().await? {
        total += chunk.len();
        println!("   a piece of {} bytes ({total} in all)", chunk.len());
    }
    seen = wire(&script, seen);

    println!("9. one client, several tasks");
    // A client and its calls are `Send + Sync + Clone`; a clone shares the transport.
    answer(&script, 200, BOOK);
    answer(&script, 200, BOOK);
    let (one, two) = (shop.clone(), shop.clone());
    let first = tokio::spawn(async move { one.books().get("978-0").await });
    let second = tokio::spawn(async move { two.books().get("978-0").await });
    for task in [first, second] {
        if let Ok(book) = task.await {
            println!("   {:?}", book?.map(|book| book.title));
        }
    }
    wire(&script, seen);

    // The key went out in every request, marked sensitive, and is printed nowhere.
    let marked = |r: &Request| {
        r.headers()
            .get("x-api-key")
            .is_some_and(|k| k.is_sensitive())
    };
    println!(
        "key marked in every request: {}",
        script.requests().iter().all(marked)
    );
    println!("the client: {shop:?}"); // `Debug` shows no input
    Ok(())
}
