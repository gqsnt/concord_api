//! What [`api!`](crate::api) generates for one small description. This module exists in the
//! documentation only: it is not part of the crate that you depend on.
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
//! What it generates, shown on the pages below:
//!
//! | Item | What it is |
//! |---|---|
//! | [`Library`] | The client: `Library::new(key)`, its settings (`with_timeout`, `with_retry`, …), and `books()`. |
//! | [`library::Books`] | The handle of the scope `books`: one method per endpoint. |
//! | [`library::books::Title`] | A call: `.await` sends it. It also has `send_full()`, `request()`, and the settings of one call. |
//! | [`library::books::Titles`] | A paginated call: `.pages()`, `.pages().items()`, and `send()` for one page. It has no `.await`. |
//! | [`library::books::Delete`] | A call with no arrow: `.await` returns `()`. |
//! | [`library::Error`], [`library::Result`], [`library::Retry`] | The aliases: `concord::Error<String>`, a `Result` of it, and `concord::Retry<String>`. |
//!
//! The aliases show a hidden trait, `<() as __Types>::__Problem`, which stands for the `problem`
//! type, here `String`. Names that begin with `__` belong to Concord.

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
