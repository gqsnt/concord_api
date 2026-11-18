//! A response that rejects a credential without a `401`: the function that recognises
//! it, and the pass-through that shows it the body as the body is read.

use concord_core::bytes::Bytes;
use concord_core::http::{self, HeaderMap, StatusCode};
use concord_core::reqwest::{self, Response, ResponseBuilderExt};
use futures_core::Stream;
use std::fmt;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::pin::Pin;
use std::task::{Context, Poll};

/// The longest body that a `rejected_when` function is shown, in bytes: 64 KiB.
const LIMIT: usize = 65_536;

pub(super) type Rejects = dyn Fn(&Seen<'_>) -> bool + Send + Sync;

/// What a `rejected_when` function receives: the status, the headers and the whole body
/// of one response.
///
/// It is shown a response whose body was read to its end and holds at most 64 KiB.
/// Only the methods return the values: `Debug` prints none of them.
///
/// ```no_run
/// fn expired(seen: &concord::auth::Seen<'_>) -> bool {
///     seen.status() == 200 && seen.body().starts_with(br#"{"ok":false"#)
/// }
/// ```
pub struct Seen<'a> {
    status: StatusCode,
    headers: &'a HeaderMap,
    body: &'a [u8],
}

impl Seen<'_> {
    /// Returns the response status.
    ///
    /// ```no_run
    /// fn ok(seen: &concord::auth::Seen<'_>) -> bool { seen.status().is_success() }
    /// ```
    pub fn status(&self) -> StatusCode {
        self.status
    }

    /// Returns the response headers.
    ///
    /// ```no_run
    /// fn json(seen: &concord::auth::Seen<'_>) -> bool {
    ///     seen.headers().get("content-type").is_some_and(|value| value.as_bytes().starts_with(b"application/json"))
    /// }
    /// ```
    pub fn headers(&self) -> &HeaderMap {
        self.headers
    }

    /// Returns the whole body, as it was received.
    ///
    /// ```no_run
    /// fn empty(seen: &concord::auth::Seen<'_>) -> bool { seen.body().is_empty() }
    /// ```
    pub fn body(&self) -> &[u8] {
        self.body
    }
}

impl fmt::Debug for Seen<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Seen(..)")
    }
}

/// Hands the response back with a body that passes every frame through unchanged and
/// shows the whole body to `rejects` once it has been read to its end. `reject` runs
/// once, outside every lock of the credential, when `rejects` returns `true`.
pub(super) fn watch(
    response: Response,
    rejects: std::sync::Arc<Rejects>,
    reject: impl FnOnce() + Send + 'static,
) -> Response {
    // A body known to be longer than the limit is never shown: leave it untouched.
    if response
        .content_length()
        .is_some_and(|length| usize::try_from(length).map_or(true, |length| length > LIMIT))
    {
        return response;
    }
    let url = response.url().clone();
    let raw: http::Response<reqwest::Body> = response.into();
    let (mut parts, body) = raw.into_parts();
    // Only an extension is set, so this builder has no fallible inputs.
    let holder = http::Response::builder()
        .url(url)
        .body(())
        .unwrap_or_else(|_| http::Response::new(()));
    parts.extensions.extend(holder.into_parts().0.extensions);
    let pending = Pending {
        status: parts.status,
        headers: parts.headers.clone(),
        chunks: Vec::new(),
        length: 0,
        rejects,
        reject: Box::new(reject),
    };
    let watched = Watched {
        inner: Box::pin(Response::from(http::Response::new(body)).bytes_stream()),
        pending: Some(pending),
    };
    Response::from(http::Response::from_parts(
        parts,
        reqwest::Body::wrap_stream(watched),
    ))
}

type Chunks = dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send;

struct Watched {
    inner: Pin<Box<Chunks>>,
    // None once the body has been shown, has failed, or has passed the limit.
    pending: Option<Pending>,
}

struct Pending {
    status: StatusCode,
    headers: HeaderMap,
    chunks: Vec<Bytes>,
    length: usize,
    rejects: std::sync::Arc<Rejects>,
    reject: Box<dyn FnOnce() + Send>,
}

impl Pending {
    fn show(self) {
        let Pending {
            status,
            headers,
            chunks,
            rejects,
            reject,
            ..
        } = self;
        let joined;
        let body: &[u8] = match chunks.as_slice() {
            [] => &[],
            [one] => one,
            many => {
                joined = many.concat();
                &joined
            }
        };
        let seen = Seen {
            status,
            headers: &headers,
            body,
        };
        // A panic rejects nothing; its payload is dropped.
        if catch_unwind(AssertUnwindSafe(|| rejects(&seen))).unwrap_or(false) {
            reject();
        }
    }
}

impl Stream for Watched {
    type Item = Result<Bytes, reqwest::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let polled = self.inner.as_mut().poll_next(cx);
        match &polled {
            Poll::Ready(Some(Ok(chunk))) => {
                let within = self.pending.as_mut().is_some_and(|pending| {
                    match pending.length.checked_add(chunk.len()) {
                        Some(length) if length <= LIMIT => {
                            pending.length = length;
                            pending.chunks.push(chunk.clone());
                            true
                        }
                        _ => false,
                    }
                });
                if !within {
                    drop(self.pending.take());
                }
            }
            Poll::Ready(Some(Err(_))) => drop(self.pending.take()),
            Poll::Ready(None) => {
                if let Some(pending) = self.pending.take() {
                    pending.show();
                }
            }
            Poll::Pending => {}
        }
        polled
    }
}
