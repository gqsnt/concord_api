//! The description of the bookshop, and its types. `main.rs` uses it.

use concord::read::Stream;
use concord::{by_cursor, json, Body, ByteStream, Json, Secret};
use serde::Deserialize;

/// A book, as the API writes it.
#[derive(Debug, Deserialize)]
pub struct Book {
    pub isbn: String,
    pub title: String,
    pub year: Option<u16>,
}

/// How the API says no, on every 4xx and 5xx: `{ "code": "…", "message": "…" }`.
#[derive(Debug, Deserialize)]
pub struct Problem {
    pub code: String,
    pub message: String,
}

/// One page of a list: `{ "items": [Book], "next": "cursor or null" }`.
#[derive(Debug, Deserialize)]
pub struct BookPage {
    pub items: Vec<Book>,
    pub next: Option<String>,
}

impl BookPage {
    /// Where the next page starts, for `by_cursor`. `None` ends the pages.
    fn next(&self) -> Option<String> {
        self.next.clone()
    }
}

// `.pages().items()` needs to know where the items of a page are.
impl concord::page::IntoItems for BookPage {
    type Item = Book;
    fn into_items(self) -> Vec<Book> {
        self.items
    }
    fn count(&self) -> usize {
        self.items.len()
    }
}

/// What a creation gives: the book, and where it now lives.
#[derive(Debug)]
pub struct Created {
    pub book: Book,
    pub location: String,
}

concord::api! {
    /// The bookshop: one client for one region, with one key.
    pub client Bookshop(region: String, key: Secret) {
        // A hole in the host is one label: `eu` gives `eu.bookshop.example`.
        base "https://{region}.bookshop.example/v2";
        // A `Secret` in a header: the header is marked sensitive, and never printed.
        headers { "x-api-key" = key; }
        // No format by default: JSON both ways is said once, here.
        read Json;
        // Every refusal whose body is a `Problem` gives it in `error.problem()`.
        problem Problem;

        scope books {
            path "books";

            // 1. A 2xx is read as a `Book` and given as `Some`; a 404 is `None`, not an error.
            GET get(isbn: String) -> Option<Book> {
                path "{isbn}";
                status { 404 => None }
            }

            // 2. `year` is optional (a method); `tag` is a list: the key repeats, and an
            // empty list sends nothing. In `paginate`, `cursor` names where the pager writes.
            GET list(author: String, year?: u16, tag: Vec<String>, cursor?: String) -> BookPage {
                query { author; year; tag; cursor; }
                paginate by_cursor(cursor, BookPage::next);
            }

            // 3. The body is made of inputs; a `year` that is not set is left out. One arm
            // reads the body and takes a header: no reader of your own is needed.
            // `2xx`, not `201`: another 2xx would else be read into `Created` itself.
            POST create(title: String, author: String, year?: u16) -> Created {
                body json { title; author; year; }
                status {
                    2xx { body, headers } => Created {
                        book: body,
                        location: headers.require("location")?.to_owned(),
                    },
                }
            }

            // 4. Bytes with their content type. In an expression an input is a reference
            // (`&Bytes`), so it is cloned. No arrow: a 204, or any 2xx, gives `()`.
            PUT upload_cover(isbn: String, png: concord::bytes::Bytes) {
                path "{isbn}/cover";
                body Body::new(png.clone(), "image/png");
            }

            // 5. A `DELETE` may be sent again; a `POST` may not, unless it says `idempotent;`.
            DELETE delete(isbn: String) {
                path "{isbn}";
            }
        }

        // 8. The call ends at the head, and gives the body as it comes.
        GET export() -> ByteStream {
            path "export";
            read Stream;
        }
    }
}
