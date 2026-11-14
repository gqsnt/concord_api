#![doc = include_str!("../README.md")]
//!
//! ## What this crate holds
//!
//! This crate is a facade. It holds no code of its own: the macro [`api!`], and the items of
//! `concord-core` and `concord-extras` that a description or a caller names.
//!
//! | From | Items |
//! |---|---|
//! | `concord-macros` | [`api!`]. It knows the tree, the statements and the inputs, but nothing about the expressions on their right-hand side. |
//! | `concord-core` | The values and traits of a call: [`Secret`], [`Base`], [`Body`], [`Reader`], [`Retry`], [`Error`], [`Pages`], [`Middleware`], [`Transport`], [`Http`]; [`fake`] and [`reply`] for tests. |
//! | `concord-extras` | The shipped implementations: [`json`], [`form`], optional multipart; [`struct@Json`], [`struct@Text`], [`struct@Binary`], [`read::Stream`]; [`by_offset`], [`by_offset_limit`], [`by_page`], [`by_cursor`], [`by_link`], [`by_next_url`]; [`basic`]. Readers, writers and pagers use public traits and helpers; the Tower adapter also uses a hidden helper for the URL of a response. |
//!
//! The reader `Stream` is named through its module, [`read::Stream`]: at the root, its name would
//! clash with the `Stream` trait that most async code imports.
//!
//! With the `multipart` feature, chain `.text(...)` and `.file(...)` on `multipart()`,
//! then call `.finish()` to get a `Result<Body, MultipartError>`, which `body` accepts as it is.
//! The builder and error types are available at the root and in [`body`]. See
//! [`api!`] for an example, with the bytes that are sent.
//!
//! ## Optional HTTP features
//!
//! These features are off by default and forward to the same feature of `reqwest`:
//!
//! | Feature | Makes available |
//! |---|---|
//! | `gzip` | Gzip response decompression with [`Http::with`] and `builder.gzip(true)` |
//! | `deflate` | Deflate response decompression with [`Http::with`] and `builder.deflate(true)` |
//! | `socks` | SOCKS proxies configured with `reqwest::Proxy` in [`Http::with`] |
//!
//! A Cargo feature only makes a capability available: Concord keeps decompression off until
//! your client turns it on, and once it is on, a reader's limit counts the decoded bytes. See
//! [`Http::with`] for the Cargo line and a gzip example.
//!
//! ## Where to read
//!
//! - The language, with an example for each statement: [`api!`].
//! - The code that the macro generates, shown on a real page: [`sample`] (in the documentation only).
//! - A whole client that shows nine behaviours without opening a socket: `examples/bookshop`
//!   (`cargo run --example bookshop`). Each boundary below appears there.
//!
//! ## The boundaries
//!
//! How a client behaves at its edges, one sentence each.
//!
//! - **What is retried.** Nothing, until `with_retry` sets a policy such as
//!   [`Retry::standard`]; then a request is retried only when it may be repeated
//!   (`GET`, `HEAD`, `OPTIONS`, `PUT`, `DELETE`, an endpoint marked `idempotent;`, or a
//!   request rejected with a status listed in [`Retry::not_applied_on`]) and the failure is
//!   worth retrying (a failed connection, a timeout, a response body that broke off, `408`,
//!   `429`, `502`, `503`, `504`), so a `POST` whose endpoint is not marked `idempotent;` is
//!   retried only after such a listed status, never after a lost connection.
//! - **A wait longer than the time left.** A wait before another attempt (the backoff, or a
//!   `Retry-After`) that would end after the call's `timeout` is not started: the call
//!   returns the last outcome at once, such as the `429` itself with its
//!   [`Error::retry_after`], rather than a timeout; a `Retry-After` over 60 seconds is never
//!   waited for.
//! - **A stream and the timeout.** The call timeout (`with_timeout`) bounds the whole
//!   call, so it keeps running while you read a [`ByteStream`] from `read Stream;`, until the
//!   stream ends.
//! - **Two timeouts.** Besides the call timeout, which has no limit until you set one, the
//!   idle timeout ends a call when the server sends nothing for 30 seconds (`with_idle_timeout`,
//!   or `idle_timeout` in the description); [`Error::is_timeout`] is true for a timeout of
//!   either kind.
//! - **Sharing.** A client, its handles and its calls are `Send + Sync + Clone` (see
//!   [`sample::Library`]); clones share the transport and the middleware, so one client
//!   serves many tasks.
//! - **A query.** A value is serialized with `serde`: a list repeats its key (`tag=a&tag=b`),
//!   an empty list and a `None` add nothing, and a space is encoded as `%20`, never `+` (see
//!   "Entries" in [`api!`]).
//! - **Secrets.** The `Debug` of a client and of a call, and the `Display` and `Debug` of an
//!   error, never print a [`Secret`], and the header that carries one is marked sensitive;
//!   [`Error::details`], which you call to see what the server sent, may echo one back.

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

/// Text that is sent on the wire and shown nowhere else.
pub mod secret {
    pub use concord_core::secret::*;
}

/// The start of a URL, when the caller provides it.
pub mod base {
    pub use concord_core::base::*;
}

/// The body of a request as a value, and the JSON, form and optional multipart writers.
pub mod body {
    pub use concord_core::body::*;
    #[cfg(any(feature = "json", feature = "form", feature = "multipart"))]
    pub use concord_extras::body::*;
}

/// How a response becomes a value: the [`Reader`] trait and four readers.
///
/// | Reader | After the arrow | What it reads |
/// |---|---|---|
/// | [`Json`](struct@Json) | any type that `serde` reads | the whole body, at most 10 MiB |
/// | [`Text`](struct@Text) | `String` | the whole body as UTF-8, at most 10 MiB |
/// | [`Binary`](struct@Binary) | `bytes::Bytes` | the whole body, at most 10 MiB |
/// | [`Stream`](read::Stream) | [`ByteStream`] | nothing: the body is handed over as it comes |
///
/// **`Stream` is named through its module:** `read concord::read::Stream;`, or
/// `use concord::read::Stream;`. There is no `concord::Stream`: at the root it would clash
/// with the `Stream` trait that most async code imports.
///
/// An endpoint with no arrow returns `()` and reads no body, so a `204` (or any 2xx)
/// needs nothing. Under an arrow, `Text` reads an empty body as `""` and `Binary`
/// as empty bytes; `Json` needs a JSON value. An arm can produce the value for an
/// empty response without reading it: `status { 204 => None }`.
///
/// Your own reader is a value that implements [`Reader`]; it goes in the same place:
/// `read Mine;`.
pub mod read {
    pub use concord_core::read::*;
    pub use concord_extras::read::*;
}

/// A call's value together with the response's status and headers.
pub mod answer {
    pub use concord_core::answer::*;
}

/// The single error type of a client.
pub mod error {
    pub use concord_core::error::*;
}

/// Whether and how a failed request is retried.
pub mod retry {
    pub use concord_core::retry::*;
}

/// Lists that span several responses: the building blocks of a pager, and six pagers.
pub mod page {
    pub use concord_core::page::*;
    pub use concord_extras::page::*;
}

/// Middleware that every request passes through, such as a signer or a token refresher.
pub mod middleware {
    pub use concord_core::middleware::*;
}

/// What actually sends a request, and a fake transport for tests.
pub mod transport {
    pub use concord_core::transport::*;
}

/// The types that a generated method accepts as an argument.
pub mod arg {
    #[doc(inline)]
    pub use concord_core::arg::Arg;
}

/// A `tower` service used as a transport.
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
