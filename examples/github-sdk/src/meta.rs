//! Response metadata kept with typed values: GitHub's request id, rate-limit headers, the
//! entity tag and the pagination links.
//!
//! Concord's `send().await?` returns the decoded value only. GitHub puts information an application
//! needs in headers (`x-github-request-id` for support requests, `x-ratelimit-*` to pace itself,
//! `ETag` for conditional refreshes, `Link` for pagination), so this module sends through
//! Concord's response boundary: `Prepared::receive` (or `Decoder::execute` for a request edited
//! natively, [`send_edited`]) executes once and returns the response for any status with its body
//! unread; the selected headers are copied into a [`ResponseMeta`]; then `Received::decode`
//! applies the endpoint's own handling (status check, `Json` head check, decoding). A transport
//! failure is Concord's `Transport` error (URL removed) and carries no metadata: no head arrived.
//! A failure after the head keeps the metadata ([`GitHubError::Api`] with `meta: Some`).
//!
//! The headers come from `Received::head` and the base URL from `Received::url`: relative `Link`
//! targets resolve against the URL of the response that carried them, which is not the request's
//! after a followed same-origin redirect (a renamed repository). Nothing is read from the body.
//!
//! `Debug` of [`ResponseMeta`] shows the status, the request id, the rate-limit numbers and which
//! links exist; never a link URL (it carries filters and cursors) or any other header value.

use std::fmt;

use concord::reqwest::header::{ETAG, HeaderMap, HeaderName, LAST_MODIFIED, LINK, RETRY_AFTER};
use concord::reqwest::{self, StatusCode, Url};
use concord::{Prepared, Received, field};

use crate::error::GitHubError;
use crate::link::{self, LinkError, Links};
use crate::params::ETag;

/// `x-github-request-id`.
pub const X_GITHUB_REQUEST_ID: HeaderName = HeaderName::from_static("x-github-request-id");
/// `x-ratelimit-limit`.
pub const X_RATELIMIT_LIMIT: HeaderName = HeaderName::from_static("x-ratelimit-limit");
/// `x-ratelimit-remaining`.
pub const X_RATELIMIT_REMAINING: HeaderName = HeaderName::from_static("x-ratelimit-remaining");
/// `x-ratelimit-used`.
pub const X_RATELIMIT_USED: HeaderName = HeaderName::from_static("x-ratelimit-used");
/// `x-ratelimit-reset` (UTC epoch seconds).
pub const X_RATELIMIT_RESET: HeaderName = HeaderName::from_static("x-ratelimit-reset");
/// `x-ratelimit-resource`.
pub const X_RATELIMIT_RESOURCE: HeaderName = HeaderName::from_static("x-ratelimit-resource");

/// The `x-ratelimit-*` headers of one response. A missing, repeated or unparsable numeric
/// header is `None`; numbers must be unsigned ASCII decimal digits, allowing surrounding OWS.
/// GitHub notes the values may vary between responses because requests are served by several
/// regions; these are the values of this response, not an account-wide state.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RateLimit {
    /// Requests allowed per window.
    pub limit: Option<u64>,
    /// Requests remaining in the current window.
    pub remaining: Option<u64>,
    /// Requests used in the current window.
    pub used: Option<u64>,
    /// When the window resets, UTC epoch seconds.
    pub reset: Option<u64>,
    /// The resource the request counted against (`core`, `search`, ...).
    pub resource: Option<String>,
}

/// Selected metadata of one response head, copied before the body was read.
#[derive(Clone)]
pub struct ResponseMeta {
    status: StatusCode,
    request_id: Option<String>,
    rate: RateLimit,
    retry_after: Option<u64>,
    etag: Option<ETag>,
    last_modified: Option<String>,
    links: Result<Links, LinkError>,
    location: Option<String>,
}

impl ResponseMeta {
    /// Copies the metadata from a response whose body is unread. Relative `Link` targets resolve
    /// against the response's own URL.
    pub fn capture(response: &reqwest::Response) -> Self {
        Self::from_head(response.status(), response.headers(), response.url())
    }

    /// The same, from a status, headers and URL.
    pub fn from_head(status: StatusCode, headers: &HeaderMap, url: &Url) -> Self {
        let text = |name: &HeaderName| {
            headers
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned)
        };
        let number = |name: &HeaderName| field::unsigned(headers, name).ok().flatten();
        Self {
            status,
            request_id: text(&X_GITHUB_REQUEST_ID),
            rate: RateLimit {
                limit: number(&X_RATELIMIT_LIMIT),
                remaining: number(&X_RATELIMIT_REMAINING),
                used: number(&X_RATELIMIT_USED),
                reset: number(&X_RATELIMIT_RESET),
                resource: text(&X_RATELIMIT_RESOURCE),
            },
            retry_after: number(&RETRY_AFTER),
            etag: text(&ETAG).map(ETag::new),
            last_modified: text(&LAST_MODIFIED),
            links: link::parse(headers.get_all(LINK), url),
            location: text(&reqwest::header::LOCATION),
        }
    }

    /// The status.
    pub fn status(&self) -> StatusCode {
        self.status
    }

    /// `x-github-request-id`, the identifier GitHub support asks for.
    pub fn request_id(&self) -> Option<&str> {
        self.request_id.as_deref()
    }

    /// The rate-limit headers.
    pub fn rate_limit(&self) -> &RateLimit {
        &self.rate
    }

    /// `retry-after`, in seconds, when GitHub sent it.
    pub fn retry_after(&self) -> Option<u64> {
        self.retry_after
    }

    /// The entity tag, for a later conditional request.
    pub fn etag(&self) -> Option<&ETag> {
        self.etag.as_ref()
    }

    /// `Last-Modified`, as sent.
    pub fn last_modified(&self) -> Option<&str> {
        self.last_modified.as_deref()
    }

    /// The pagination links, or why the `Link` header could not be read.
    ///
    /// # Errors
    ///
    /// The [`LinkError`] of a malformed header.
    pub fn links(&self) -> Result<&Links, LinkError> {
        self.links.as_ref().map_err(|e| *e)
    }

    /// The raw `Location` header, **unredacted** (a signed download URL is a bearer credential
    /// until it expires).
    pub fn raw_location(&self) -> Option<&str> {
        self.location.as_deref()
    }

    /// Whether this response says the primary or secondary rate limit was hit: `403`/`429` with
    /// `x-ratelimit-remaining: 0`, or with `retry-after` (GitHub's documented signals).
    pub fn is_rate_limited(&self) -> bool {
        matches!(
            self.status,
            StatusCode::FORBIDDEN | StatusCode::TOO_MANY_REQUESTS
        ) && (self.rate.remaining == Some(0) || self.retry_after.is_some())
    }
}

impl fmt::Debug for ResponseMeta {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResponseMeta")
            .field("status", &self.status.as_u16())
            .field("request_id", &self.request_id)
            .field("rate", &self.rate)
            .field("retry_after", &self.retry_after)
            .field("etag", &self.etag.is_some())
            .field("links", &self.links)
            .field("location", &self.location.as_ref().map(|_| "<withheld>"))
            .finish()
    }
}

/// A decoded value and the metadata of its response.
pub struct WithMeta<T> {
    /// The endpoint's typed output.
    pub value: T,
    /// The response metadata.
    pub meta: ResponseMeta,
}

impl<T> fmt::Debug for WithMeta<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WithMeta")
            .field("value", &format_args!("<withheld>"))
            .field("meta", &self.meta)
            .finish()
    }
}

/// The result of a conditional request (`If-None-Match`).
pub enum Conditional<T> {
    /// `304 Not Modified`: the representation the entity tag names is current. GitHub does not
    /// count such a response against the primary rate limit when the request was authorized.
    NotModified {
        /// The `304` response's metadata.
        meta: ResponseMeta,
    },
    /// A new representation (or a first one, without an entity tag).
    Modified(WithMeta<T>),
}

impl<T> fmt::Debug for Conditional<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotModified { meta } => {
                f.debug_struct("NotModified").field("meta", meta).finish()
            }
            Self::Modified(with) => f.debug_tuple("Modified").field(with).finish(),
        }
    }
}

/// Executes `prepared` once (after `edit`), capturing [`ResponseMeta`] from the head, then
/// decodes with the endpoint's decoder. `edit` may change the native request (a `Link` URL, an
/// extra header) or refuse it; a refusal sends nothing.
///
/// # Errors
///
/// [`GitHubError::Api`] with `meta: None` when no response arrived, `Some` once a head did;
/// the error of `edit`.
pub async fn send_edited<T>(
    prepared: Prepared<T>,
    edit: impl FnOnce(&mut reqwest::Request) -> Result<(), GitHubError>,
) -> Result<WithMeta<T>, GitHubError> {
    let (client, mut request, decoder) = prepared.into_parts();
    edit(&mut request)?;
    let received = decoder
        .execute(&client, request)
        .await
        .map_err(no_response)?;
    let (meta, received) = capture(received);
    decode_with_meta(meta, received).await
}

/// The typed value with its metadata: [`send_edited`] without an edit.
///
/// # Errors
///
/// See [`send_edited`].
pub async fn send_with_meta<T>(prepared: Prepared<T>) -> Result<WithMeta<T>, GitHubError> {
    let received = prepared.receive().await.map_err(no_response)?;
    let (meta, received) = capture(received);
    decode_with_meta(meta, received).await
}

/// Sends a request that carries `If-None-Match` (set it on the operation, for example
/// `repo.issue(n).set_if_none_match(etag)`): `304` is [`Conditional::NotModified`], any other
/// response is handled as by [`send_with_meta`].
///
/// # Errors
///
/// See [`send_edited`].
pub async fn send_conditional<T>(prepared: Prepared<T>) -> Result<Conditional<T>, GitHubError> {
    let received = prepared.receive().await.map_err(no_response)?;
    let (meta, received) = capture(received);
    if received.status() == StatusCode::NOT_MODIFIED {
        // A 304 has no body; dropping it returns the connection to the pool.
        return Ok(Conditional::NotModified { meta });
    }
    decode_with_meta(meta, received)
        .await
        .map(Conditional::Modified)
}

/// No response arrived (a transport failure): no metadata.
fn no_response(error: concord::Error) -> GitHubError {
    GitHubError::api(error, None)
}

/// Copies the metadata of a received response, body unread (see the
/// [module documentation](self) for the base URL of relative links).
fn capture<T>(received: Received<T>) -> (ResponseMeta, Received<T>) {
    let meta =
        ResponseMeta::from_head(received.status(), received.head().headers(), received.url());
    (meta, received)
}

/// The endpoint's declared handling, the metadata kept on success and on failure.
async fn decode_with_meta<T>(
    meta: ResponseMeta,
    received: Received<T>,
) -> Result<WithMeta<T>, GitHubError> {
    match received.decode().await {
        Ok(value) => Ok(WithMeta { value, meta }),
        Err(error) => Err(GitHubError::api(error, Some(meta))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_headers_require_one_unsigned_field() {
        for (values, expected) in [
            (&["+42"][..], None),
            (&["42", "43"][..], None),
            (&[" \t42\t "][..], Some(42)),
        ] {
            let mut headers = HeaderMap::new();
            for name in [
                &X_RATELIMIT_LIMIT,
                &X_RATELIMIT_REMAINING,
                &X_RATELIMIT_USED,
                &X_RATELIMIT_RESET,
                &RETRY_AFTER,
            ] {
                for value in values {
                    headers.append(name, value.parse().unwrap());
                }
            }
            let meta = ResponseMeta::from_head(
                StatusCode::OK,
                &headers,
                &Url::parse("https://api.github.com/").unwrap(),
            );
            let rate = meta.rate_limit();
            assert_eq!(
                [
                    rate.limit,
                    rate.remaining,
                    rate.used,
                    rate.reset,
                    meta.retry_after()
                ],
                [expected; 5],
                "{values:?}",
            );
        }
    }
}
