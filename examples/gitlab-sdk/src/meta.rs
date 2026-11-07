//! Response metadata kept with typed values and with errors: the request id, GitLab's offset
//! pagination headers, `Link` and `Retry-After`.
//!
//! [`ResponseMeta::capture`] reads them from the response head before the body is consumed, so
//! they survive a body that fails to read or decode. Nothing else of the response is kept.

use std::fmt;

use concord::reqwest::header::{HeaderMap, LINK, RETRY_AFTER};
use concord::reqwest::{StatusCode, Url};
use concord::{Received, ResponseHead};

use crate::link::{self, LinkError, Links};

/// A typed value with the metadata of the response it came from.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Response<T> {
    /// The decoded value.
    pub value: T,
    /// The response metadata.
    pub meta: ResponseMeta,
}

impl<T> Response<T> {
    /// The value, without its metadata.
    pub fn into_value(self) -> T {
        self.value
    }
}

/// GitLab's offset-pagination headers, each `None` when absent, repeated, empty or not unsigned
/// ASCII decimal digits (surrounding spaces and tabs are allowed).
///
/// GitLab documents that `x-total` and `x-total-pages` (and `rel="last"`) are not sent when a
/// query returns more than 10,000 records, so a missing total is not "zero" and not "last page".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Pagination {
    /// `x-page`: this page's index, from 1.
    pub page: Option<u64>,
    /// `x-per-page`: items per page.
    pub per_page: Option<u64>,
    /// `x-prev-page`: the previous page's index.
    pub prev_page: Option<u64>,
    /// `x-next-page`: the next page's index. Informational only: the traversal follows `Link`.
    pub next_page: Option<u64>,
    /// `x-total`: total items.
    pub total: Option<u64>,
    /// `x-total-pages`: total pages.
    pub total_pages: Option<u64>,
}

/// What the SDK keeps from a response head. `Debug` shows the status, the request id, the
/// pagination numbers and which `Link` relations were present — never a URL or another header
/// value.
#[derive(Clone)]
pub struct ResponseMeta {
    status: StatusCode,
    request_id: Option<String>,
    pagination: Pagination,
    next_page_announced: bool,
    retry_after: Option<u64>,
    links: Result<Links, LinkError>,
}

impl ResponseMeta {
    /// Reads the metadata of `received` without consuming it. Relative `Link` targets resolve
    /// against the final response URL (after any redirect the client followed).
    pub fn capture<T>(received: &Received<T>) -> Self {
        Self::from_head(received.status(), received.head().headers(), received.url())
    }

    pub(crate) fn from_head(status: StatusCode, headers: &HeaderMap, url: &Url) -> Self {
        let text = |name: &str| {
            headers
                .get(name)
                .and_then(|value| value.to_str().ok())
                .map(str::trim)
        };
        let head = ResponseHead::new(status, concord::reqwest::Version::HTTP_11, headers);
        let number = |name: &str| head.unsigned(name).ok().flatten();
        Self {
            status,
            request_id: text("x-request-id")
                .filter(|id| !id.is_empty())
                .map(str::to_owned),
            pagination: Pagination {
                page: number("x-page"),
                per_page: number("x-per-page"),
                prev_page: number("x-prev-page"),
                next_page: number("x-next-page"),
                total: number("x-total"),
                total_pages: number("x-total-pages"),
            },
            // Present and non-empty, number or not: GitLab sends it empty on the last page.
            next_page_announced: headers
                .get("x-next-page")
                .is_some_and(|value| !value.as_bytes().trim_ascii().is_empty()),
            retry_after: number(RETRY_AFTER.as_str()),
            links: link::parse(headers.get_all(LINK), url),
        }
    }

    /// The response status.
    pub fn status(&self) -> StatusCode {
        self.status
    }

    /// `x-request-id`, GitLab's correlation id for support requests.
    pub fn request_id(&self) -> Option<&str> {
        self.request_id.as_deref()
    }

    /// The offset-pagination headers.
    pub fn pagination(&self) -> Pagination {
        self.pagination
    }

    /// `Retry-After` in seconds (sent with `429 Too Many Requests`); an HTTP-date is not read.
    pub fn retry_after(&self) -> Option<u64> {
        self.retry_after
    }

    /// The `Link` relations, or why the header could not be read. URLs are **unredacted** (they
    /// carry the listing's filters); do not log them.
    ///
    /// # Errors
    ///
    /// The [`LinkError`] of a malformed header.
    pub fn links(&self) -> Result<&Links, LinkError> {
        self.links.as_ref().map_err(|e| *e)
    }

    /// Whether `x-next-page` names a next page (present and non-empty).
    pub(crate) fn next_page_announced(&self) -> bool {
        self.next_page_announced
    }
}

impl fmt::Debug for ResponseMeta {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResponseMeta")
            .field("status", &self.status.as_u16())
            .field("request_id", &self.request_id)
            .field("pagination", &self.pagination)
            .field("retry_after", &self.retry_after)
            .field("links", &self.links)
            .finish()
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
                "x-page",
                "x-per-page",
                "x-prev-page",
                "x-next-page",
                "x-total",
                "x-total-pages",
                "retry-after",
            ] {
                for value in values {
                    headers.append(name, value.parse().unwrap());
                }
            }
            let meta = ResponseMeta::from_head(
                StatusCode::OK,
                &headers,
                &Url::parse("https://gitlab.example.com/api/v4/projects/8/issues").unwrap(),
            );
            let pagination = meta.pagination();
            assert_eq!(
                [
                    pagination.page,
                    pagination.per_page,
                    pagination.prev_page,
                    pagination.next_page,
                    pagination.total,
                    pagination.total_pages,
                    meta.retry_after(),
                ],
                [expected; 7],
                "{values:?}",
            );
        }
    }
}
