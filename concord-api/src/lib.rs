#![doc = include_str!("../README.md")]
//!
//! ## What this crate holds
//!
//! This crate is a facade. It holds no code of its own: the macro [`api!`], and the items of
//! `concord-core` and `concord-extras` that a description or a caller names.
//!
//! | From | Items |
//! |---|---|
//! | `concord-macros` | [`api!`]. It knows the tree, the statements and the inputs, and nothing of what stands on their right side. |
//! | `concord-core` | The values and traits of a call: [`Secret`], [`Base`], [`Body`], [`Reader`], [`Retry`], [`Error`], [`Pages`], [`Middleware`], [`Transport`], [`Http`]; [`fake`] and [`reply`] for tests. |
//! | `concord-extras` | What is shipped: [`json`], [`form`], optional multipart; [`struct@Json`], [`struct@Text`], [`struct@Binary`], [`read::Stream`]; [`by_offset`], [`by_offset_limit`], [`by_page`], [`by_cursor`], [`by_link`], [`by_next_url`]; [`basic`]. Readers, writers and pagers use public traits and helpers; the Tower adapter also uses a hidden helper for the URL of an answer. |
//!
//! The reader `Stream` is named by its module, [`read::Stream`]: at the root, its name would
//! meet the trait `Stream` that most async code imports.
//!
//! With feature `multipart`, chain `.text(...)` and `.file(...)` on `multipart()`,
//! then call `.finish()` for `Result<Body, MultipartError>`, which `body` takes as it is.
//! The builder and error types are available at the root and in [`body`]. The page of
//! [`api!`] has an example, with the bytes that go out.
//!
//! ## Where to read
//!
//! - The language, with an example for each statement: the page of [`api!`].
//! - What the macro writes, on a real page: [`sample`] (in the documentation only).
//! - A whole client of nine behaviours, with no socket: `examples/bookshop`
//!   (`cargo run --example bookshop`). Each boundary below is seen there.
//!
//! ## The boundaries
//!
//! What a client does at its edges, each in one sentence.
//!
//! - **What is tried again.** Nothing, until `with_retry` gives a policy such as
//!   [`Retry::standard`]; then a request goes out again only when it may be repeated
//!   (`GET`, `HEAD`, `OPTIONS`, `PUT`, `DELETE`, or an endpoint marked `idempotent;`) and the
//!   failure is worth it (a failed connection, a timeout, a body that broke, `408`, `429`,
//!   `502`, `503`, `504`), so a `POST` is not sent again, not even after a lost connection,
//!   unless it says `idempotent;`.
//! - **A wait that does not fit.** A wait before another try (the backoff, or a
//!   `Retry-After`) that would end after the call's `timeout` is not started: the call
//!   returns the last outcome at once, such as the `429` itself with its
//!   [`Error::retry_after`], not a timeout; a `Retry-After` over 60 seconds is never waited.
//! - **A stream and the timeout.** The `timeout` of a call (`with_timeout`) bounds the whole
//!   call, so it still runs while a [`ByteStream`] from `read Stream;` is read, until its end.
//! - **Two clocks.** Beside `timeout`, which has no limit until you set one, the clock for
//!   silence ends a call when the server says nothing for 30 seconds (`with_idle_timeout`, or
//!   `idle_timeout` in the description); a timeout of either kind is
//!   [`Error::is_timeout`].
//! - **Sharing.** A client, its handles and its calls are `Send + Sync + Clone` (see
//!   [`sample::Library`]); clones share the transport and the middlewares, so one client
//!   serves many tasks.
//! - **A query.** A value is written by `serde`: a list repeats its key (`tag=a&tag=b`), an
//!   empty list and a `None` write nothing, and a space is `%20`, never `+` (the
//!   [`api!`] page, "Entries").
//! - **Secrets.** The `Debug` of a client and of a call, and the `Display` and `Debug` of an
//!   error, never print a [`Secret`], and its header is marked sensitive; [`Error::details`], which you call to
//!   see what the server sent, may echo one back.

#![forbid(unsafe_code)]

// The page `sample` is written by the macro, whose code names `::concord`.
#[cfg(doc)]
extern crate self as concord;

#[cfg(doc)]
pub mod sample;

#[doc(inline)]
pub use concord_macros::api;

#[doc(hidden)]
pub use concord_core::__private;

#[doc = include_str!("retry.md")]
pub use concord_core::Retry;
pub use concord_core::{bytes, http, reqwest};
pub use concord_core::{fake, reply, reply_with};
pub use concord_core::{
    mismatch, Answer, Base, Body, ByteStream, Error, Fail, Headers, Http, Items, Middleware, Next,
    Pages, Reader, Secret, Sending, Transport,
};
pub use concord_core::{KIB, MIB};

pub use concord_extras::basic;
#[cfg(feature = "form")]
pub use concord_extras::body::form;
#[cfg(feature = "json")]
pub use concord_extras::body::json;
#[cfg(feature = "multipart")]
pub use concord_extras::body::{multipart, Multipart, MultipartError};
pub use concord_extras::page::{
    by_cursor, by_link, by_next_url, by_offset, by_offset_limit, by_page,
};
#[cfg(feature = "json")]
pub use concord_extras::read::Json;
pub use concord_extras::read::{Binary, Text};

/// Text that goes on the wire and nowhere else.
pub mod secret {
    pub use concord_core::secret::*;
}

/// Where a URL starts, when the user gives it.
pub mod base {
    pub use concord_core::base::*;
}

/// The body of a request, as a value; JSON, form and optional multipart writers.
pub mod body {
    pub use concord_core::body::*;
    #[cfg(any(feature = "json", feature = "form", feature = "multipart"))]
    pub use concord_extras::body::*;
}

/// How an answer becomes a value: the trait [`Reader`], and four readers.
///
/// | Reader | After the arrow | What it reads |
/// |---|---|---|
/// | [`Json`](struct@Json) | any type that `serde` reads | the whole body, at most 10 MiB |
/// | [`Text`](struct@Text) | `String` | the whole body as UTF-8, at most 10 MiB |
/// | [`Binary`](struct@Binary) | `bytes::Bytes` | the whole body, at most 10 MiB |
/// | [`Stream`](read::Stream) | [`ByteStream`] | nothing: the body is handed over as it comes |
///
/// **`Stream` is named by its module:** `read concord::read::Stream;`, or
/// `use concord::read::Stream;`. There is no `concord::Stream`: at the root it would meet
/// the trait `Stream` that most async code imports.
///
/// An endpoint with no arrow gives `()` and reads no body, so a `204` (or any 2xx)
/// needs nothing. Under an arrow, `Text` reads an empty body as `""` and `Binary`
/// as empty bytes; `Json` needs a JSON value. An arm can give the value of an
/// empty answer without reading it: `status { 204 => None }`.
///
/// A reader of yours is a value with [`Reader`]; it takes the same place:
/// `read Mine;`.
pub mod read {
    pub use concord_core::read::*;
    pub use concord_extras::read::*;
}

/// What came back.
pub mod answer {
    pub use concord_core::answer::*;
}

/// The one error of a client.
pub mod error {
    pub use concord_core::error::*;
}

/// Whether and how to try again.
pub mod retry {
    pub use concord_core::retry::*;
}

/// A list that comes in several answers: what a pager is made with, and six pagers.
pub mod page {
    pub use concord_core::page::*;
    pub use concord_extras::page::*;
}

/// What every request goes through, such as a signer or a token that is renewed.
pub mod middleware {
    pub use concord_core::middleware::*;
}

/// What really sends a request. A fake for tests.
pub mod transport {
    pub use concord_core::transport::*;
}

/// What an argument of a generated method may be.
pub mod arg {
    #[doc(inline)]
    pub use concord_core::arg::Arg;
}

/// A `tower` service as a transport.
#[cfg(feature = "tower")]
pub mod tower {
    pub use concord_extras::tower::*;
}

#[cfg(test)]
mod package_tests {
    #[test]
    fn c10_9_core_and_macros_are_pinned_to_the_same_exact_release() {
        let manifest = include_str!("../Cargo.toml");
        let mut versions = Vec::new();
        for name in ["concord-core", "concord-macros"] {
            let dependency = manifest
                .lines()
                .find(|line| line.starts_with(&format!("{name} =")))
                .unwrap();
            let version = dependency
                .split("version = \"")
                .nth(1)
                .unwrap()
                .split('"')
                .next()
                .unwrap();
            assert!(version.starts_with('='));
            versions.push(version);
        }
        assert_eq!(versions.first(), versions.get(1));
        assert_eq!(
            versions.first().copied(),
            Some(concat!("=", env!("CARGO_PKG_VERSION")))
        );
    }
}
