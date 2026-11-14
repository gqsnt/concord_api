//! A paged endpoint, read in its two loops: page by page, and item by item.
//!
//! Run it with `cargo run --example pages`.
#![allow(clippy::print_stdout)] // An example prints what it shows.

use concord::reqwest::{Request, Response};
use concord::transport::TransportError;
use concord::{by_offset, fake, reply, Json};

// How the API pages is a fact of the API, so the description says it, once.
// `by_offset(start)` is an ordinary function of Concord: after each page it asks the next
// one with `start` moved by the number of items that came, and stops at an empty page.
concord::api! {
    client Names {
        base "https://names.example.com";
        read Json;

        GET list(start?: u32, count?: u32) -> Vec<String> {
            path "names";
            query { start; count; }
            paginate by_offset(start);
        }
    }
}

const NAMES: [&str; 5] = ["ada", "alan", "barbara", "edsger", "grace"];

// The server returns `count` names from `start`, defaulting to two. No socket is opened.
fn server(request: &Request) -> Result<Response, TransportError> {
    println!("  GET {}", request.url());
    let start: usize = request
        .url()
        .query_pairs()
        .find(|(key, _)| key == "start")
        .and_then(|(_, value)| value.parse().ok())
        .unwrap_or(0);
    let count: usize = request
        .url()
        .query_pairs()
        .find(|(key, _)| key == "count")
        .and_then(|(_, value)| value.parse().ok())
        .unwrap_or(2);
    let page: Vec<String> = NAMES
        .iter()
        .skip(start)
        .take(count)
        .map(|name| format!("\"{name}\""))
        .collect();
    Ok(reply(200, format!("[{}]", page.join(","))))
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> names::Result<()> {
    let names = Names::new().with_transport(fake(server));

    // A paged call is not awaited directly: `.pages()` says that you want them all.
    // Nothing is sent before the first `try_next()`.
    println!("page by page:");
    let mut pages = names.list().pages();
    while let Some(page) = pages.try_next().await? {
        // The last page is empty: `by_offset` ends there, and the page that came is still given.
        println!("{page:?}");
    }

    // `.items()` flattens the pages. It asks the next page when one is used up.
    println!("item by item:");
    let mut items = names.list().pages().items();
    while let Some(name) = items.try_next().await? {
        println!("{name}");
    }

    // `.send()` gives one page, on purpose.
    let first = names.list().count(5).send().await?;
    println!("up to five names: {first:?}");
    Ok(())
}
