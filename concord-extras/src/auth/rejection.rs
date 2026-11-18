//! A response that rejects a credential without a `401`: the function that recognises
//! it, and the pass-through that shows it the body as the body is read.

use concord_core::bytes::Bytes;
use concord_core::http::{self, HeaderMap, StatusCode};
use concord_core::reqwest::{self, Response, ResponseBuilderExt};
use http_body::{Body, Frame, SizeHint};
use std::fmt;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

/// The longest body that a `rejected_when` function is shown, and the most that is
/// copied of any body, in bytes: 64 KiB.
const LIMIT: usize = 65_536;

pub(super) type Rejects = dyn Fn(&Seen<'_>) -> bool + Send + Sync;

/// What a `rejected_when` function receives: the status, the headers and the whole body
/// of one response.
///
/// It is shown a response whose body was read to its end, holds at most 64 KiB and has
/// the length that the response announced. Only the methods return the values: `Debug`
/// prints none of them. Match the errors that reject the token, not every error:
///
/// ```no_run
/// #[derive(serde::Deserialize)]
/// struct Envelope { ok: bool, error: Option<String> }
/// fn rejects_the_token(seen: &concord::auth::Seen<'_>) -> bool {
///     seen.status() == 200
///         && serde_json::from_slice::<Envelope>(seen.body()).is_ok_and(|reply| {
///             !reply.ok && matches!(reply.error.as_deref(), Some("token_expired" | "token_revoked"))
///         })
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

/// Hands the response back with a body that forwards every frame, its size hint and its
/// end unchanged, and shows the whole body to `rejects` once it has been read to its end.
/// `reject` runs once, outside every lock of the credential, when `rejects` returns `true`.
/// A response whose final URL is not at `origin` is returned untouched.
pub(super) fn watch(
    response: Response,
    origin: Option<&reqwest::Url>,
    rejects: Arc<Rejects>,
    reject: impl FnOnce() + Send + Sync + 'static,
) -> Response {
    if !origin.is_some_and(|origin| origin.origin() == response.url().origin()) {
        return response;
    }
    // The length that the reader checks the body against: the header, as the reader
    // parses it, or else the exact size hint of the body.
    let announced = response
        .headers()
        .get(http::header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .filter(|text| !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit()))
        .and_then(|text| text.parse::<u64>().ok())
        .or_else(|| response.content_length());
    // A body announced as longer than the limit is never shown: leave it untouched.
    if announced.is_some_and(|length| usize::try_from(length).map_or(true, |length| length > LIMIT))
    {
        return response;
    }
    let url = response.url().clone();
    let mut raw: http::Response<reqwest::Body> = response.into();
    // Only an extension is set, so this builder has no fallible inputs.
    let holder = http::Response::builder()
        .url(url)
        .body(())
        .unwrap_or_else(|_| http::Response::new(()));
    raw.extensions_mut()
        .extend(holder.into_parts().0.extensions);
    let pending = Pending {
        status: raw.status(),
        headers: raw.headers().clone(),
        announced,
        body: Vec::new(),
        rejects,
        reject: Box::new(reject),
    };
    raw.map(|body| {
        reqwest::Body::wrap(Watched {
            body,
            pending: Some(pending),
        })
    })
    .into()
}

struct Watched {
    body: reqwest::Body,
    // None once the body has been shown, has failed, or has passed the limit.
    pending: Option<Pending>,
}

struct Pending {
    status: StatusCode,
    headers: HeaderMap,
    announced: Option<u64>,
    // One buffer, never larger than the limit.
    body: Vec<u8>,
    rejects: Arc<Rejects>,
    reject: Box<dyn FnOnce() + Send + Sync>,
}

impl Pending {
    // Copies a piece of the body; false when the body would pass the limit.
    fn copy(&mut self, data: &[u8]) -> bool {
        if data.is_empty() {
            return true;
        }
        let Some(needed) = self.body.len().checked_add(data.len()) else {
            return false;
        };
        if needed > LIMIT {
            return false;
        }
        if needed > self.body.capacity() {
            let wanted = self
                .announced
                .and_then(|length| usize::try_from(length).ok())
                .filter(|length| *length >= needed)
                .unwrap_or_else(|| self.body.capacity().saturating_mul(2))
                .clamp(needed, LIMIT);
            let more = wanted.saturating_sub(self.body.len());
            if self.body.try_reserve_exact(more).is_err() {
                return false;
            }
        }
        self.body.extend_from_slice(data);
        true
    }

    fn show(self) {
        let Pending {
            status,
            headers,
            announced,
            body,
            rejects,
            reject,
        } = self;
        // An incomplete or over-long body, by the reader's own count, is not shown.
        let length = u64::try_from(body.len()).ok();
        if announced.is_some_and(|announced| length != Some(announced)) {
            return;
        }
        let seen = Seen {
            status,
            headers: &headers,
            body: &body,
        };
        // A panic rejects nothing; its payload is dropped.
        if catch_unwind(AssertUnwindSafe(|| rejects(&seen))).unwrap_or(false) {
            reject();
        }
    }
}

impl Body for Watched {
    type Data = Bytes;
    type Error = <reqwest::Body as Body>::Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let frame = Pin::new(&mut self.body).poll_frame(cx);
        let this = &mut *self;
        match &frame {
            Poll::Ready(Some(Ok(frame))) => {
                if let Some(data) = frame.data_ref() {
                    if !this
                        .pending
                        .as_mut()
                        .is_some_and(|pending| pending.copy(data))
                    {
                        drop(this.pending.take());
                    }
                }
                if this.body.is_end_stream() {
                    if let Some(pending) = this.pending.take() {
                        pending.show();
                    }
                }
            }
            Poll::Ready(Some(Err(_))) => drop(this.pending.take()),
            Poll::Ready(None) => {
                if let Some(pending) = this.pending.take() {
                    pending.show();
                }
            }
            Poll::Pending => {}
        }
        frame
    }

    fn is_end_stream(&self) -> bool {
        self.body.is_end_stream()
    }

    fn size_hint(&self) -> SizeHint {
        self.body.size_hint()
    }
}
