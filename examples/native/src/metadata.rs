//! W2 (M01–M05): the typed value together with selected response-head metadata, after an
//! optional native edit, with Concord's error conversion and redaction kept.
//!
//! The ownership stages, each a value you hold:
//!
//! 1. [`concord::Prepared`] → `into_parts()`: the client, the native request, the endpoint's
//!    [`concord::Decoder`];
//! 2. your edit of the native `reqwest::Request` (a header, a timeout, a query pair);
//! 3. exactly **one** `client.execute(request)`. A failure before any response head becomes a
//!    Concord [`ErrorKind::Transport`] error through `decoder.finish(Err(e))`, the documented
//!    conversion that removes the URL — never a raw `reqwest::Error`;
//! 4. [`Received`]: the head arrived, the body is unread. The headers you selected, the status,
//!    the version and the final URL are copied into a [`Meta`] by borrowing the response;
//! 5. [`Received::decode`] hands the response to the endpoint's decoder (status check, the
//!    format's head checks, body read, decoding), unchanged.
//!
//! What survives, by failure stage ([`MetaError`] holds the `concord::Error` and an
//! `Option<Meta>`):
//!
//! | Stage | `ErrorKind` | Response in the `concord::Error` | [`MetaError::meta`] |
//! |---|---|---|---|
//! | `prepare()` (when converted with `?`) | `Build` | — | `None` |
//! | execute fails (connect, timeout before the head) | `Transport` | — | `None` |
//! | non-2xx | `Status` | unread (`into_response()`) | `Some` |
//! | the format refuses the 2xx head | `Metadata` | unread (`into_response()`) | `Some` |
//! | reading the body fails (truncated, timeout) | `Transport` | — (body gone) | `Some` |
//! | the body does not decode | `Decode` | — (body gone) | `Some` |
//! | success | — | — | [`WithMeta::meta`] |
//!
//! `Some` metadata therefore means "a response head was received". After a body-read or decode
//! failure only the captured head survives; the body bytes are not kept. A body-read failure is
//! Concord's `Transport` even where reqwest's own `transport().is_decode()` is true (a truncated
//! body read with `bytes()`); Concord's `Decode` is the format refusing a complete body.
//!
//! Which metadata to keep is the application's choice (`keep`, per call): only what must outlive
//! the response. Everything else in the head is dropped with it.
//!
//! **Diagnostics.** [`Meta`], [`WithMeta`], [`Received`] and [`MetaError`] implement `Debug` (and
//! `MetaError` `Display`) by hand: status, version and the **names** of the kept headers; never a
//! header value, the URL or the decoded value, so no `T: Debug` bound is needed. `MetaError`'s
//! `Display` is the Concord error's, and it has no `source()` chain, like `concord::Error`.
//! Raw access is explicit and named as such: [`Meta::raw_header`], [`Meta::raw_headers`],
//! [`Meta::raw_url`], the public [`WithMeta::value`], and the `concord::Error` accessors
//! (`response()`, `into_response()`, `transport()`), whose boundaries Concord documents.

use std::fmt;

use concord::reqwest::header::{AsHeaderName, ETAG, HeaderMap, HeaderName, HeaderValue};
use concord::reqwest::{self, StatusCode, Url, Version};
use concord::{Decoder, ErrorKind, Prepared};

use crate::api::{Inventory, Item};

/// The response-head metadata an application chose to keep: status, version, the final URL and
/// the values of selected headers, copied before the body is read.
///
/// `Debug` shows the status, the version and the kept header names only. Values and the URL are
/// reachable through the `raw_*` accessors, unredacted.
#[derive(Clone)]
pub struct Meta {
    status: StatusCode,
    version: Version,
    url: Url,
    headers: HeaderMap,
}

impl Meta {
    /// Copies the status, version, final URL and every value of the `keep` headers (repeated
    /// lines included, in order) from a response whose body is not read.
    pub fn capture(response: &reqwest::Response, keep: &[HeaderName]) -> Self {
        let mut headers = HeaderMap::new();
        for name in keep {
            for value in response.headers().get_all(name) {
                headers.append(name.clone(), value.clone());
            }
        }
        Self {
            status: response.status(),
            version: response.version(),
            url: response.url().clone(),
            headers,
        }
    }

    /// The response status.
    pub fn status(&self) -> StatusCode {
        self.status
    }

    /// The HTTP version.
    pub fn version(&self) -> Version {
        self.version
    }

    /// The first value of a kept header, **unredacted** (`None` when the header was absent or
    /// not selected).
    pub fn raw_header(&self, name: impl AsHeaderName) -> Option<&HeaderValue> {
        self.headers.get(name)
    }

    /// Every kept header value, **unredacted**.
    pub fn raw_headers(&self) -> &HeaderMap {
        &self.headers
    }

    /// The final URL of the response (after redirects), **unredacted**: query values and path
    /// segments included, sensitive or not.
    pub fn raw_url(&self) -> &Url {
        &self.url
    }
}

impl fmt::Debug for Meta {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // `keys()` yields each name once, however many values it has.
        let names: Vec<&str> = self.headers.keys().map(HeaderName::as_str).collect();
        f.debug_struct("Meta")
            .field("status", &self.status.as_u16())
            .field("version", &self.version)
            .field("header_names", &names)
            .field("url", &format_args!("<withheld>"))
            .finish()
    }
}

/// A decoded value with the metadata of the response it came from.
pub struct WithMeta<T> {
    /// The endpoint's typed output, decoded by its declared format.
    pub value: T,
    /// The selected response-head metadata.
    pub meta: Meta,
}

impl<T> fmt::Debug for WithMeta<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WithMeta")
            .field("value", &format_args!("<withheld>"))
            .field("meta", &self.meta)
            .finish()
    }
}

/// A response whose head was received and captured, its body still unread, with the endpoint's
/// decoder.
pub struct Received<T> {
    meta: Meta,
    response: reqwest::Response,
    decoder: Decoder<T>,
}

impl<T> Received<T> {
    /// The metadata captured from the head.
    pub fn meta(&self) -> &Meta {
        &self.meta
    }

    /// Reads and decodes the body with the endpoint's decoder (`Decoder::decode`: status check,
    /// the format's head checks, body read, decoding), keeping the metadata on both outcomes.
    ///
    /// # Errors
    ///
    /// A [`MetaError`] with `Some` metadata: `Status` or `Metadata` (the response is kept unread
    /// in the `concord::Error`), `Transport` (the body read failed) or `Decode`.
    pub async fn decode(self) -> Result<WithMeta<T>, MetaError> {
        let Self {
            meta,
            response,
            decoder,
        } = self;
        match decoder.decode(response).await {
            Ok(value) => Ok(WithMeta { value, meta }),
            Err(error) => Err(MetaError {
                error,
                meta: Some(Box::new(meta)),
            }),
        }
    }
}

impl<T> fmt::Debug for Received<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Received")
            .field("meta", &self.meta)
            .field("body", &format_args!("<unread>"))
            .finish()
    }
}

/// A failed call: the Concord error, and the metadata of the response head if one arrived.
pub struct MetaError {
    error: concord::Error,
    // Boxed, like `concord::Error`'s own state: the `Err` variant stays two pointers wide.
    meta: Option<Box<Meta>>,
}

impl MetaError {
    /// The Concord error (its kind, and for `Status`/`Metadata` the unread response).
    pub fn error(&self) -> &concord::Error {
        &self.error
    }

    /// The stage at which the call failed.
    pub fn kind(&self) -> ErrorKind {
        self.error.kind()
    }

    /// The metadata of the response head: `None` when no head arrived (build or transport
    /// failure before a response), `Some` from the moment a head was received.
    pub fn meta(&self) -> Option<&Meta> {
        self.meta.as_deref()
    }

    /// The Concord error and the metadata, owned (for `concord::Error::into_response`).
    pub fn into_parts(self) -> (concord::Error, Option<Meta>) {
        (self.error, self.meta.map(|meta| *meta))
    }
}

/// A Concord error raised before any response: `prepare()?` in a function returning
/// `MetaError`.
impl From<concord::Error> for MetaError {
    fn from(error: concord::Error) -> Self {
        Self { error, meta: None }
    }
}

impl fmt::Display for MetaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.error, f)
    }
}

impl fmt::Debug for MetaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MetaError")
            .field("error", &self.error)
            .field("meta", &self.meta)
            .finish()
    }
}

/// No `source()`: `Display` already is the Concord error's, and `concord::Error` has no chain.
impl std::error::Error for MetaError {}

/// Stages 1–4: takes the prepared request apart, applies `edit` to the native request, executes
/// it **once** and captures the `keep` headers from the head, leaving the body unread.
///
/// # Errors
///
/// A [`MetaError`] without metadata when no response arrived: Concord's `Transport` error,
/// converted by the endpoint's decoder (the URL removed).
pub async fn execute<T>(
    prepared: Prepared<T>,
    keep: &[HeaderName],
    edit: impl FnOnce(&mut reqwest::Request),
) -> Result<Received<T>, MetaError> {
    let (client, mut request, decoder) = prepared.into_parts();
    edit(&mut request);
    match client.execute(request).await {
        Ok(response) => Ok(Received {
            meta: Meta::capture(&response, keep),
            response,
            decoder,
        }),
        Err(e) => {
            // `Decoder::finish` is the public conversion of a native transport error into
            // Concord's (`ErrorKind::Transport`, URL removed); it never succeeds on `Err`.
            let Err(error) = decoder.finish(Err(e)).await else {
                unreachable!("`Decoder::finish(Err(_))` is always `Err`")
            };
            Err(MetaError { error, meta: None })
        }
    }
}

/// [`execute`], then [`Received::decode`]: the typed value with the selected metadata.
///
/// ```no_run
/// # use concord::reqwest::header::{ETAG, HeaderValue};
/// # use concord_example_native::{api::Inventory, metadata::{MetaError, send_with_meta}};
/// # async fn run(api: Inventory) -> Result<(), MetaError> {
/// let got = send_with_meta(api.item(7).prepare()?, &[ETAG], |request| {
///     let trace = HeaderValue::from_static("trace-1");
///     request.headers_mut().insert("x-trace", trace);
/// })
/// .await?;
/// let etag = got.meta.raw_header(ETAG); // deliberate, unredacted access
/// # let _ = (got.value, etag); Ok(()) }
/// ```
///
/// # Errors
///
/// A [`MetaError`]: see the module documentation for what each stage keeps.
pub async fn send_with_meta<T>(
    prepared: Prepared<T>,
    keep: &[HeaderName],
    edit: impl FnOnce(&mut reqwest::Request),
) -> Result<WithMeta<T>, MetaError> {
    execute(prepared, keep, edit).await?.decode().await
}

/// The headers [`item_with_meta`] keeps: the entity tag and the server's request id.
pub static ITEM_META: [HeaderName; 2] = [ETAG, HeaderName::from_static("x-request-id")];

/// One item with its `ETag` and the server's `x-request-id`, sending the caller's request id.
///
/// Use it when a caller needs a response header (a validator for a later conditional request, a
/// correlation id) together with the typed value, and possibly a native edit. Mark `request_id`
/// sensitive yourself (`HeaderValue::set_sensitive`) if it is a secret: after `into_parts` only
/// flagged values are masked in native `Debug`.
///
/// # Errors
///
/// A [`MetaError`] (see the module documentation).
pub async fn item_with_meta(
    api: &Inventory,
    id: u64,
    request_id: HeaderValue,
) -> Result<WithMeta<Item>, MetaError> {
    send_with_meta(api.item(id).prepare()?, &ITEM_META, |request| {
        request.headers_mut().insert("x-request-id", request_id);
    })
    .await
}
