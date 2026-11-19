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
//! | `concord-core` | The values and traits of a call: [`Secret`], [`Base`], [`Body`], [`Reader`], [`Retry`], [`Error`], [`Pages`], [`page::Pager`], [`Middleware`], [`Transport`], [`Http`]; [`fake`] and [`reply`] for tests. |
//! | `concord-extras` | The shipped implementations: [`json`], [`form`], [`bracket_form`], [`bracket_pairs`], [`json_lines`], optional `gzip`, `multipart`, `multipart_mixed` and `multipart_related`; [`struct@Json`], [`struct@Text`], [`struct@Binary`], [`read::Stream`], [`read::JsonLines`], [`read::Sse`]; [`by_offset`], [`by_offset_limit`], [`by_page`], [`by_cursor`], [`by_link`], [`by_next_url`], [`page::by_offset_total`] and what they return, [`page::Paging`]; [`basic`]; optional `auth` (with rotating credentials under `rotating`), `sign` and `limit`; [`transfer`]. These use public traits and helpers, including [`Body::get_content_type`] for `gzip`. Only the Tower adapter also uses a hidden helper for the URL of a response. |
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
//!   [`sample::library::Call`] documents the shared
//!   methods. [`sample::library::books::Titles`] documents the request, inputs, options and
//!   result of `Books::titles`, which returns `Call<Titles>`.
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
#![cfg_attr(docsrs, feature(doc_cfg))]

// The page `sample` is written by the macro, whose code names `::concord`.
#[cfg(doc)]
extern crate self as concord;

#[cfg(doc)]
pub mod sample;

#[doc(inline)]
pub use concord_macros::api;

#[doc(hidden)]
pub use concord_core::__private;
#[doc(hidden)]
pub use concord_core::call;

/// A request body, including a file reopened at each authorized attempt.
///
/// ```no_run
/// use concord::{Body, Retry};
/// use std::{io, path::PathBuf};
/// use tokio_util::io::ReaderStream;
///
/// concord::api! {
///     client Uploads {
///         base "https://uploads.example.test";
///         PUT put(file: Body, size: u64) {
///             path "snapshot";
///             headers { "content-length" = size; }
///             body file;
///         }
///     }
/// }
///
/// async fn upload_snapshot(
///     snapshot: PathBuf,
///     returned_headers: concord::http::HeaderMap,
/// ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
///     let size = tokio::fs::metadata(&snapshot).await?.len();
///     let body = Body::repeatable(move || {
///         let snapshot = snapshot.clone();
///         async move {
///             let file = tokio::fs::File::open(snapshot).await?;
///             Ok::<_, io::Error>(ReaderStream::new(file))
///         }
///     }).content_type("application/octet-stream");
///
///     Uploads::new()
///         .put(body, size)
///         .edit_request(move |request| {
///             request.headers_mut().extend(returned_headers.clone());
///         })
///         .with_retry(Retry::standard())
///         .await?;
///     Ok(())
/// }
/// ```
#[doc(inline)]
pub use concord_core::Body;
#[doc = include_str!("retry.md")]
pub use concord_core::Retry;
pub use concord_core::{bytes, http, reqwest};
pub use concord_core::{fake, reply, reply_with};
pub use concord_core::{
    mismatch, Answer, Base, ByteStream, Error, Fail, Headers, Http, Items, Middleware, Next, Pages,
    Reader, Secret, Sending, Transport,
};
pub use concord_core::{KIB, MIB};

pub use concord_extras::basic;
#[cfg(feature = "form")]
#[cfg_attr(docsrs, doc(cfg(feature = "form")))]
pub use concord_extras::body::form;
#[cfg(feature = "json")]
#[cfg_attr(docsrs, doc(cfg(feature = "json")))]
pub use concord_extras::body::json;
#[cfg(feature = "form")]
#[cfg_attr(docsrs, doc(cfg(feature = "form")))]
pub use concord_extras::body::{bracket_form, bracket_pairs, BracketFormError};
#[cfg(feature = "gzip-body")]
#[cfg_attr(docsrs, doc(cfg(feature = "gzip-body")))]
pub use concord_extras::body::{gzip, GzipError};
#[cfg(feature = "json")]
#[cfg_attr(docsrs, doc(cfg(feature = "json")))]
pub use concord_extras::body::{json_lines, JsonLinesError};
#[cfg(feature = "multipart")]
#[cfg_attr(docsrs, doc(cfg(feature = "multipart")))]
pub use concord_extras::body::{multipart, Multipart, MultipartError};
#[cfg(feature = "multipart")]
#[cfg_attr(docsrs, doc(cfg(feature = "multipart")))]
pub use concord_extras::body::{multipart_mixed, multipart_related, MimeWriteError};
pub use concord_extras::page::{
    by_cursor, by_link, by_next_url, by_offset, by_offset_limit, by_page, by_page_bounds,
};
#[cfg(feature = "json")]
#[cfg_attr(docsrs, doc(cfg(feature = "json")))]
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

/// The body of a request as a value, and the optional request body writers.
pub mod body {
    pub use concord_core::body::*;
    // Without a writer feature this import is empty. It has no condition of its own,
    // so that docs.rs shows only the feature of each writer.
    // PANIC-SAFETY: No panic lint is relaxed; an empty import is allowed.
    #[allow(unused_imports)]
    pub use concord_extras::body::*;
}

/// How a response becomes a value: whole bodies, byte streams and bounded records.
///
/// | Reader | After the arrow | What it reads |
/// |---|---|---|
/// | [`Json`](struct@Json) | any type that `serde` reads | the whole body, at most 10 MiB |
/// | [`Text`](struct@Text) | `String` | the whole body as UTF-8, at most 10 MiB |
/// | [`Binary`](struct@Binary) | `bytes::Bytes` | the whole body, at most 10 MiB |
/// | [`Stream`](read::Stream) | [`ByteStream`] | nothing: the body is handed over as it comes |
/// | [`JsonLines`](read::JsonLines) | [`RecordStream<T>`](read::RecordStream) | one JSON value per line, with an explicit byte bound |
/// | [`Sse`](read::Sse) | [`RecordStream<SseEvent>`](read::RecordStream) | server-sent events, with an explicit byte bound |
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

/// Lists that span several responses: the building blocks of a pager, seven pagers, and the
/// `Paging` that they return.
pub mod page {
    pub use concord_core::page::*;
    pub use concord_extras::page::*;
}

/// Middleware that every request passes through, such as a signer, `auth::Renewed` for a token
/// that is renewed, or `limit::Limit`.
pub mod middleware {
    pub use concord_core::middleware::*;
}

/// What actually sends a request, and a fake transport for tests.
pub mod transport {
    pub use concord_core::transport::*;
}

#[cfg(feature = "auth")]
#[cfg_attr(docsrs, doc(cfg(feature = "auth")))]
#[doc(inline)]
pub use concord_extras::auth;

#[cfg(feature = "sign")]
#[cfg_attr(docsrs, doc(cfg(feature = "sign")))]
#[doc(inline)]
pub use concord_extras::sign;

#[cfg(feature = "limit")]
#[cfg_attr(docsrs, doc(cfg(feature = "limit")))]
#[doc(inline)]
pub use concord_extras::limit;

#[doc(inline)]
pub use concord_extras::transfer;

/// The types that a generated method accepts as an argument.
pub mod arg {
    #[doc(inline)]
    pub use concord_core::arg::Arg;
}

/// A `tower` service used as a transport.
#[cfg(feature = "tower")]
#[cfg_attr(docsrs, doc(cfg(feature = "tower")))]
pub mod tower {
    pub use concord_extras::tower::*;
}

#[cfg(test)]
mod package_tests {
    fn version(line: &str) -> Option<&str> {
        let (_, rest) = line.split_once("version = \"")?;
        rest.split_once('"').map(|(version, _)| version)
    }

    fn dependency_version<'a>(manifest: &'a str, name: &str) -> Option<&'a str> {
        let inline = format!("{name} =");
        let dependency = manifest.lines().find(|line| line.starts_with(&inline));
        if let Some(line) = dependency {
            return version(line);
        }
        let table = format!("[dependencies.{name}]");
        let (_, body) = manifest.split_once(&table)?;
        body.lines()
            .take_while(|line| !line.starts_with('['))
            .find_map(version)
    }

    #[test]
    fn c10_9_core_and_macros_are_pinned_to_the_same_exact_release() {
        let manifest = include_str!("../Cargo.toml");
        for name in ["concord-core", "concord-macros"] {
            assert_eq!(
                dependency_version(manifest, name),
                Some(concat!("=", env!("CARGO_PKG_VERSION"))),
                "{name}"
            );
        }
    }
}
