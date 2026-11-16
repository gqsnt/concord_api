//! A URL that the API returned.
//!
//! Declare the upload host separately, with its own base and no API credentials
//! or middleware. [`at`] validates the original URL text, then calls the public
//! [`Call::at_url`]. The engine checks the origin against the upload call's base
//! when it builds the request, before any transport send. This helper creates no
//! new call type, reader, retry loop or engine state.
//!
//! Only an absolute HTTP(S) URL with identical parsed and HTTP-URI spelling is
//! accepted. Fragments, a lone `?` or a query containing only `&` separators are
//! refused because `at_url` discards them. User information is also refused:
//! `at_url` can retain it, and it is not part of the engine's origin comparison.
//! Other query spelling, escape case, ordering and repeated keys are preserved
//! for a credential-free call. This function cannot inspect an opaque call's
//! secret query entries, middleware, custom transport or later edits; use the
//! separate description below and keep edits confined to returned headers.
//!
//! Give the second description a [`Body`](concord_core::Body) input and write
//! `body bytes;`, as below. Inputs are borrowed inside a description: construct
//! a `Body` from your bytes before passing it to the call, rather than passing a
//! `Bytes` input to `Body::new` inside the description.
//!
//! This upload uses a ticket from the API, the independently chosen upload base,
//! and header names and values returned at run time. The ticket reader enables `json`;
//! transfer itself needs no feature.
//!
//! ```no_run
//! use concord::{Base, Body, Json, Secret};
//! use concord::http::{HeaderMap, HeaderName, HeaderValue};
//! use serde::Deserialize;
//!
//! #[derive(Deserialize)]
//! struct ReturnedHeader { name: String, value: String }
//! #[derive(Deserialize)]
//! struct UploadTicket { url: String, headers: Vec<ReturnedHeader> }
//! concord::api! {
//!     client Files(token: Secret) {
//!         base "https://files.example.test";
//!         headers { "authorization" = token; }
//!         POST ticket() -> UploadTicket { path "uploads"; read Json; }
//!     }
//! }
//! concord::api! {
//!     client Uploads(origin: Base) {
//!         base origin;
//!         POST send(bytes: Body) { body bytes; }
//!     }
//! }
//! async fn upload(token: Secret, bytes: Body) -> Result<(), Box<dyn std::error::Error>> {
//!     let ticket = Files::new(token).ticket().await?;
//!     let origin: Base = "https://uploads.example.test".parse()?;
//!     let uploads = Uploads::new(origin);
//!     let mut returned = HeaderMap::new();
//!     for header in ticket.headers {
//!         returned.append(HeaderName::from_bytes(header.name.as_bytes())?,
//!             HeaderValue::from_str(&header.value)?);
//!     }
//!     concord::transfer::at(uploads.send(bytes), &ticket.url)?
//!         .edit_request(move |request| request.headers_mut().extend(returned.clone()))
//!         .await?;
//!     Ok(())
//! }
//! ```
//!
//! Concord's default HTTP client follows no redirects. The generated upload call
//! retains ordinary timeout, retry, reader and one-shot-body rules. Returned headers
//! must be trusted and consistent with the actual body; raw `edit_request` is the
//! author's responsibility and runs after core validation. No upload-session
//! protocol, reconnect or download resumption is performed.

use concord_core::page::Call;
use std::error::Error as StdError;
use std::fmt;
use std::sync::Arc;

/// A URL-text validation error containing no URL or other request value.
///
/// Parser failures retain their original, value-free cause in `source()`.
/// Semantic rejection reasons have no lower cause. `Display` and `Debug` omit
/// causes. Errors from `at` are returned immediately, before there is a call error;
/// an origin mismatch instead arrives as the engine's `concord::Error` at send time.
///
/// Handle the `Result` of [`at`] directly: this error is never installed inside
/// a `concord::Error` by `at`. A later generated-call error follows the core's
/// source contract. See [`at`] for the two-line form that handles both error types.
///
/// Checks run in this order: the byte bound; the literal authority form; written
/// user information and fragments; URL parsing; the scheme and parsed authority;
/// parsed user information; URL and absolute-join spelling; an empty query; HTTP
/// URI parsing and spelling. The first failed check determines the variant, even
/// when later checks would also refuse the input.
///
/// ```no_run
/// use concord::transfer::{at, TransferError};
/// fn checked<C: concord::page::Call>(call: C, url: &str) -> Result<C, TransferError> {
///     at(call, url) // Validation errors are returned here, not by the later await.
/// }
/// ```
#[non_exhaustive]
#[derive(Clone)]
pub enum TransferError {
    /// URL parsing failed after the length, literal-authority and written-part checks.
    ///
    /// This includes malformed hosts or ports, such as `https://[invalid]/p` or
    /// `https://storage.test:70000/p`, a literal host space
    /// (`https://sto rage.test/p`), or U+0001 in a host. Whitespace or controls
    /// that parse successfully but change spelling instead produce
    /// [`ChangedUrl`](Self::ChangedUrl).
    /// The original parser cause contains no input text.
    InvalidUrl(Arc<dyn StdError + Send + Sync>),
    /// The canonical URL passed the earlier checks but the HTTP URI parser refused it.
    ///
    /// For example, ``https://sto`rage.test/p`` retains its spelling as a URL,
    /// but its host's backtick is invalid in an HTTP URI. The original,
    /// value-free URI parser cause is retained. A successful URI conversion that
    /// changes spelling instead produces [`ChangedUrl`](Self::ChangedUrl).
    InvalidUri(Arc<dyn StdError + Send + Sync>),
    /// The text lacks a literal `://` followed by a nonempty authority, or the
    /// parsed HTTP(S) URL lacks a host or cannot serve as a base.
    ///
    /// This covers empty and relative input, `mailto:user@example.test`, missing
    /// slashes (`https:storage.test/p`, `https:/storage.test/p`), extra slashes
    /// before the host (`https:///storage.test/p`), and a backslash in place of
    /// an authority slash (`https:\\storage.test/p`). The written authority ends
    /// at the first `/`, `?`, `#` or backslash; an empty one fails before parsing.
    NotAbsolute,
    /// The parsed scheme is neither `http` nor `https`, for example
    /// `ftp://storage.test/p` or `data://storage.test/p`.
    ///
    /// Input without the literal authority form, such as `mailto:user@example.test`,
    /// produces [`NotAbsolute`](Self::NotAbsolute) first. Written user information
    /// or a fragment produces [`ForbiddenUrlPart`](Self::ForbiddenUrlPart) first.
    UnsupportedScheme,
    /// The written authority contains `@`, the text contains a literal `#`, or
    /// parsed user information is present.
    ///
    /// An explicit empty user (`https://@storage.test/p`) and an empty trailing
    /// fragment (`https://storage.test/p#`) are refused too. Encoded `%23` in a
    /// path or query is ordinary data. The length and literal-authority checks
    /// take precedence, so a fragment-only reference is [`NotAbsolute`](Self::NotAbsolute).
    ForbiddenUrlPart,
    /// URL parsing, resolving the absolute URL, or successful HTTP URI conversion
    /// would change spelling after the earlier checks passed.
    ///
    /// This covers uppercase scheme/host, noncanonical host/port spelling, an
    /// inserted root path, removed dot segments, leading/trailing ASCII spaces
    /// or controls, removed tabs/LF/CR, and encoded spaces, controls (such as
    /// U+0001) or non-ASCII path/query characters.
    /// A path backslash (`https://storage.test/a\b`) also changes
    /// spelling; missing authority slashes instead produce [`NotAbsolute`](Self::NotAbsolute).
    /// Whitespace or controls that make parsing fail produce [`InvalidUrl`](Self::InvalidUrl).
    ChangedUrl,
    /// An otherwise canonical URL has a present query whose components, split
    /// at `&`, are all empty: `https://storage.test/p?`,
    /// `https://storage.test/p?&` or `https://storage.test/p?&&`.
    ///
    /// `at_url` would discard this query. Empty components mixed with nonempty
    /// ones, as in `https://storage.test/p?&key=value&&`, are preserved.
    EmptyQuery,
    /// The original text exceeds 65,534 bytes, regardless of its syntax.
    ///
    /// This check runs first; even relative input of 65,535 bytes produces this
    /// variant. The bound counts UTF-8 bytes, not characters.
    UrlTooLong,
}

impl fmt::Debug for TransferError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::InvalidUrl(_) => "InvalidUrl",
            Self::InvalidUri(_) => "InvalidUri",
            Self::NotAbsolute => "NotAbsolute",
            Self::UnsupportedScheme => "UnsupportedScheme",
            Self::ForbiddenUrlPart => "ForbiddenUrlPart",
            Self::ChangedUrl => "ChangedUrl",
            Self::EmptyQuery => "EmptyQuery",
            Self::UrlTooLong => "UrlTooLong",
        };
        f.debug_tuple(name).finish()
    }
}

impl fmt::Display for TransferError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidUrl(_) => "transfer URL could not be parsed",
            Self::InvalidUri(_) => "transfer URL is not an HTTP URI",
            Self::NotAbsolute => "transfer URL must have an absolute authority",
            Self::UnsupportedScheme => "transfer URL must use HTTP or HTTPS",
            Self::ForbiddenUrlPart => "transfer URL must omit user information and fragments",
            Self::ChangedUrl => "transfer URL spelling would change",
            Self::EmptyQuery => "transfer URL query would be discarded",
            Self::UrlTooLong => "transfer URL exceeds 65,534 bytes",
        })
    }
}

impl StdError for TransferError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::InvalidUrl(source) | Self::InvalidUri(source) => Some(source.as_ref()),
            _ => None,
        }
    }
}

/// Checks the URL text and returns `call.at_url(url)` through the public Call trait.
///
/// No request is built or sent here. The helper cannot inspect the call's base or
/// strip its secrets; origin rejection is performed by the engine before sending.
/// Use a separate credential-free description, and do not change its target later.
///
/// The accepted URL's path and query reach the transport byte for byte for a
/// description with no [`Secret`](concord_core::Secret) query entry. The engine
/// reattaches every such entry when applying `at_url`, replacing returned pairs
/// whose decoded key matches, even when the URL had no matching key. This helper
/// cannot detect those entries in an opaque call. Raw edits, middleware and custom
/// transports that change the target also remain the author's responsibility.
///
/// `at(call, url)?.await?` involves two error types: [`TransferError`] from `at`,
/// then `concord::Error` from sending, including a Build error for another origin.
/// For a typed application error, use the two-line form below to handle each step.
/// Give the upload description a [`Body`](concord_core::Body) input; construct
/// that body before passing it to the call, since description inputs are borrowed.
///
/// ```no_run
/// use concord::{Body, transfer::TransferError};
/// concord::api! {
///     client Uploads {
///         base "https://uploads.example.test";
///         POST send(bytes: Body) { body bytes; }
///     }
/// }
/// #[derive(Debug)]
/// enum UploadError { Url(TransferError), Call(concord::Error) }
/// async fn upload() -> Result<(), UploadError> {
///     let uploads = Uploads::new();
///     let bytes = concord::bytes::Bytes::from_static(b"payload");
///     let body = Body::new(bytes, "application/octet-stream");
///     let url = "https://uploads.example.test/file?sig=%2f+&key=1&key=2";
///     let call = concord::transfer::at(uploads.send(body), url).map_err(UploadError::Url)?;
///     call.await.map_err(UploadError::Call)?;
///     Ok(())
/// }
/// ```
#[must_use = "send the validated call to perform the transfer"]
pub fn at<C: Call>(call: C, url: &str) -> Result<C, TransferError> {
    if url.len() > 65_534 {
        return Err(TransferError::UrlTooLong);
    }
    let (_, rest) = url.split_once("://").ok_or(TransferError::NotAbsolute)?;
    let authority = rest.split(['/', '?', '#', '\\']).next().unwrap_or_default();
    if authority.is_empty() {
        return Err(TransferError::NotAbsolute);
    }
    // Check the written authority too: parsing erases an explicitly empty user.
    if authority.contains('@') || url.contains('#') {
        return Err(TransferError::ForbiddenUrlPart);
    }
    let parsed = concord_core::reqwest::Url::parse(url)
        .map_err(|source| TransferError::InvalidUrl(Arc::new(source)))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(TransferError::UnsupportedScheme);
    }
    if !parsed.has_host() || parsed.cannot_be_a_base() {
        return Err(TransferError::NotAbsolute);
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(TransferError::ForbiddenUrlPart);
    }
    if parsed.as_str() != url {
        return Err(TransferError::ChangedUrl);
    }
    // at_url resolves through Url::join, even for an absolute URL. Check that
    // spelling as well; the literal authority makes it independent of the base.
    let joined = parsed
        .join(url)
        .map_err(|source| TransferError::InvalidUrl(Arc::new(source)))?;
    if joined.as_str() != url {
        return Err(TransferError::ChangedUrl);
    }
    if parsed
        .query()
        .is_some_and(|query| query.split('&').all(str::is_empty))
    {
        return Err(TransferError::EmptyQuery);
    }
    let uri: concord_core::http::Uri = url
        .parse()
        .map_err(|source| TransferError::InvalidUri(Arc::new(source)))?;
    // Uri equality ignores scheme/authority case; compare serialized bytes.
    let serialized_uri = uri.to_string();
    if serialized_uri != url {
        return Err(TransferError::ChangedUrl);
    }
    Ok(call.at_url(url))
}
