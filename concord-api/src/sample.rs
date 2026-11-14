//! What [`api!`](crate::api) writes, for one small description. This module is in the
//! documentation only: it does not exist in the crate that you depend on.
//!
//! The description:
//!
//! ```text
//! use concord::{by_page, Json, Secret, Text};
//!
//! concord::api! {
//!     /// A small client, to show what the macro writes.
//!     pub client Library(key: Secret) {
//!         base "https://library.example/v1";
//!         headers { "authorization" = "Bearer {key}"; }
//!         read Json;
//!         problem String read Text;
//!
//!         /// The books of the library.
//!         scope books {
//!             path "books";
//!
//!             /// The titles, one page at a time.
//!             GET titles(author?: String, page?: u32) -> Vec<String> {
//!                 query { author; page; }
//!                 paginate by_page(page, 1);
//!             }
//!
//!             /// One title. `None` when there is no such book.
//!             GET title(isbn: String) -> Option<String> {
//!                 path "{isbn}";
//!                 status { 404 => None }
//!             }
//!
//!             /// Takes a book away.
//!             DELETE delete(isbn: String) {
//!                 path "{isbn}";
//!             }
//!         }
//!     }
//! }
//! ```
//!
//! What it gives, on the pages below:
//!
//! | Item | What it is |
//! |---|---|
//! | [`Library`] | The client: `Library::new(key)`, its settings (`with_timeout`, `with_retry`, …), and `books()`. |
//! | [`library::Books`] | The handle of the scope `books`: one method for each endpoint. |
//! | [`library::books::Title`] | A call: `.await` sends it. `send_full()`, `request()`, and the settings of one call. |
//! | [`library::books::Titles`] | A paged call: `.pages()`, `.pages().items()`, and `send()` for one page. No `.await`. |
//! | [`library::books::Delete`] | A call with no arrow: `.await` gives `()`. |
//! | [`library::Error`], [`library::Result`], [`library::Retry`] | The aliases: `concord::Error<String>`, a `Result` of it, and `concord::Retry<String>`. |
//!
//! The aliases show a hidden trait, `<() as __Types>::__Problem`: it is the `problem`
//! type, here `String`. Names that begin with `__` are Concord's.

use crate::{by_page, Json, Secret, Text};

concord::api! {
    /// A small client, to show what the macro writes.
    pub client Library(key: Secret) {
        base "https://library.example/v1";
        headers { "authorization" = "Bearer {key}"; }
        read Json;
        problem String read Text;

        /// The books of the library.
        scope books {
            path "books";

            /// The titles, one page at a time.
            GET titles(author?: String, page?: u32) -> Vec<String> {
                query { author; page; }
                paginate by_page(page, 1);
            }

            /// One title. `None` when there is no such book.
            GET title(isbn: String) -> Option<String> {
                path "{isbn}";
                status { 404 => None }
            }

            /// Takes a book away.
            DELETE delete(isbn: String) {
                path "{isbn}";
            }
        }
    }
}
