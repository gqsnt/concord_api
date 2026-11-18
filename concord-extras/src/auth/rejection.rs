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
/// It is shown a response whose body was read to its end, holds at most 64 KiB and is
/// complete, by the rule that [`Renewed::rejected_when`](super::Renewed::rejected_when)
/// states. Only the methods return the values: `Debug`
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

/// Whether the response's final URL, as the transport reports it, is at the origin of
/// the credential's base. Neither rejection path applies to a response from elsewhere.
pub(super) fn same_origin(origin: Option<&reqwest::Url>, response: &Response) -> bool {
    origin.is_some_and(|origin| origin.origin() == response.url().origin())
}

/// Hands the response back with a body that forwards every frame, its size hint and its
/// end unchanged, and shows the whole body to `rejects` once it has been read to its end.
/// `reject` runs once, outside every lock of the credential, when `rejects` returns `true`.
pub(super) fn watch(
    response: Response,
    method: &http::Method,
    rejects: Arc<Rejects>,
    reject: impl FnOnce() + Send + Sync + 'static,
) -> Response {
    // The length that the reader checks the body against: the header, as the reader
    // parses it, or else the exact size hint of the body.
    let announced = response
        .headers()
        .get(http::header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .filter(|text| !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit()))
        .and_then(|text| text.parse::<u64>().ok())
        .or_else(|| response.content_length());
    // No response is left untouched for its announced length: a `HEAD` or a `304` can
    // announce the representation's length, even as the exact size hint, and give no
    // byte. The limit applies to the bytes that come.
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
        sent: Sent::of(method),
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
    // The method as this layer sent it; a lower layer may have changed it.
    sent: Sent,
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
            sent,
            announced,
            body,
            rejects,
            reject,
        } = self;
        if !complete(status, sent, announced, body.len()) {
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

/// The method of the request as this layer sent it, as far as it decides the body.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Sent {
    Head,
    Connect,
    Other,
}

impl Sent {
    fn of(method: &http::Method) -> Self {
        if *method == http::Method::HEAD {
            Self::Head
        } else if *method == http::Method::CONNECT {
            Self::Connect
        } else {
            Self::Other
        }
    }
}

/// Whether a body that ended cleanly after `length` bytes is the whole body of its
/// response, so that the function may see it. A body that ended with an error never
/// reaches this question.
fn complete(status: StatusCode, sent: Sent, announced: Option<u64>, length: usize) -> bool {
    // These responses never carry a body: a length there describes the representation
    // (`HEAD`, `304`) or frames nothing (a tunnel), so they are complete with no byte,
    // and a byte means a malformed response. A lower layer that changes the method
    // defeats this: the method that counts is the one this layer sent.
    let bodiless = status.is_informational()
        || status == StatusCode::NO_CONTENT
        || status == StatusCode::RESET_CONTENT
        || status == StatusCode::NOT_MODIFIED
        || sent == Sent::Head
        || (sent == Sent::Connect && status.is_success());
    if bodiless {
        return length == 0;
    }
    // Any other response follows the reader's rule: exactly the length announced, by the
    // header or the exact size hint. A clean end before it, as an HTTP/2 stream reset
    // with no error, is a cut body. With nothing announced, a clean end is complete.
    announced.is_none_or(|announced| u64::try_from(length).is_ok_and(|length| length == announced))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c11_2_the_rule_for_a_complete_body() {
        let ok = StatusCode::OK;
        for (status, sent, announced, length, complete_body) in [
            // An HTTP/2 reset with no error before any data: eight announced, none came.
            (ok, Sent::Other, Some(8), 0, false),
            (ok, Sent::Other, Some(8), 8, true),
            (ok, Sent::Other, Some(8), 9, false),
            (ok, Sent::Other, None, 0, true),
            (ok, Sent::Other, None, 5, true),
            (ok, Sent::Head, Some(80_000), 0, true),
            (ok, Sent::Head, Some(1), 1, false),
            (StatusCode::NOT_MODIFIED, Sent::Other, Some(80_000), 0, true),
            (StatusCode::NO_CONTENT, Sent::Other, None, 0, true),
            (StatusCode::RESET_CONTENT, Sent::Other, Some(1), 1, false),
            (StatusCode::RESET_CONTENT, Sent::Other, Some(8), 0, true),
            (StatusCode::CONTINUE, Sent::Other, None, 1, false),
            (StatusCode::CREATED, Sent::Connect, Some(8), 0, true),
            (StatusCode::CREATED, Sent::Connect, Some(8), 1, false),
            (StatusCode::FORBIDDEN, Sent::Connect, Some(1), 1, true),
            (StatusCode::FORBIDDEN, Sent::Connect, Some(1), 0, false),
        ] {
            assert_eq!(
                complete(status, sent, announced, length),
                complete_body,
                "{status} {sent:?} {announced:?} {length}"
            );
        }
    }
}
