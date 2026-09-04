use std::fmt;

use http::{HeaderName, HeaderValue, StatusCode};
use url::Url;

use crate::error::PaginationErrorKind;
use crate::operation::OperationId;
use crate::redaction::is_sensitive_name;
use crate::response::ApiResponse;

use super::PaginationError;

/// Maximum number of distinct response-header interests a controller may declare.
pub const MAX_PAGINATION_HEADER_INTERESTS: usize = 16;
/// Maximum retained bytes across declared response-header values for one page.
pub const MAX_PAGINATION_HEADER_BYTES: usize = 64 * 1024;

/// Static bounded metadata interests for a pagination controller.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PaginationDescription {
    response_headers: &'static [&'static str],
    include_current_uri: bool,
}

impl PaginationDescription {
    /// No response metadata interests.
    pub const NONE: Self = Self::new(&[], false);

    /// Declares exact response headers and whether the current logical URI is required.
    #[must_use]
    pub const fn new(response_headers: &'static [&'static str], include_current_uri: bool) -> Self {
        Self {
            response_headers,
            include_current_uri,
        }
    }

    /// Returns the finite static header-name interests.
    #[must_use]
    pub const fn response_headers(self) -> &'static [&'static str] {
        self.response_headers
    }

    /// Returns whether relative-link resolution needs the current logical page URI.
    #[must_use]
    pub const fn includes_current_uri(self) -> bool {
        self.include_current_uri
    }
}

/// Safe correlation attached to one controller page decision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PageCorrelation {
    aggregate_operation_id: OperationId,
    page_operation_id: OperationId,
    page_index: u32,
}

impl PageCorrelation {
    /// Returns the aggregate pagination operation identity.
    #[must_use]
    pub const fn aggregate_operation_id(self) -> OperationId {
        self.aggregate_operation_id
    }

    /// Returns the managed page operation identity.
    #[must_use]
    pub const fn page_operation_id(self) -> OperationId {
        self.page_operation_id
    }

    /// Returns the zero-based page index.
    #[must_use]
    pub const fn page_index(self) -> u32 {
        self.page_index
    }
}

/// Exact decoded page plus only the bounded response metadata declared by its controller.
pub struct PageResponse<Page> {
    page: Page,
    status: StatusCode,
    headers: Vec<(HeaderName, HeaderValue)>,
    current_uri: Option<Url>,
    correlation: PageCorrelation,
}

impl<Page> PageResponse<Page> {
    pub(crate) fn from_accepted(
        response: ApiResponse<Page>,
        description: PaginationDescription,
        aggregate_operation_id: OperationId,
        page_operation_id: OperationId,
        page_index: u32,
    ) -> Result<Self, PaginationError> {
        if description.response_headers.len() > MAX_PAGINATION_HEADER_INTERESTS {
            return Err(PaginationError::new(
                PaginationErrorKind::InvalidControllerDescription,
            ));
        }

        let mut parsed_names = Vec::with_capacity(description.response_headers.len());
        for raw_name in description.response_headers {
            let name = HeaderName::from_bytes(raw_name.as_bytes()).map_err(|_| {
                PaginationError::new(PaginationErrorKind::InvalidControllerDescription)
            })?;
            if is_sensitive_name(name.as_str()) || parsed_names.contains(&name) {
                return Err(PaginationError::new(
                    PaginationErrorKind::InvalidControllerDescription,
                ));
            }
            parsed_names.push(name);
        }

        let mut retained_bytes = 0_usize;
        let mut headers = Vec::new();
        for name in parsed_names {
            for value in response.headers().get_all(&name) {
                retained_bytes = retained_bytes
                    .checked_add(name.as_str().len())
                    .and_then(|bytes| bytes.checked_add(value.as_bytes().len()))
                    .ok_or_else(|| {
                        PaginationError::new(PaginationErrorKind::PageMetadataCeilingExceeded)
                    })?;
                if retained_bytes > MAX_PAGINATION_HEADER_BYTES {
                    return Err(PaginationError::new(
                        PaginationErrorKind::PageMetadataCeilingExceeded,
                    ));
                }
                headers.push((name.clone(), value.clone()));
            }
        }

        let status = response.status();
        let current_uri = description
            .include_current_uri
            .then(|| response.url().clone());
        let page = response.into_value();
        Ok(Self {
            page,
            status,
            headers,
            current_uri,
            correlation: PageCorrelation {
                aggregate_operation_id,
                page_operation_id,
                page_index,
            },
        })
    }

    /// Returns the accepted status for this page.
    #[must_use]
    pub const fn status(&self) -> StatusCode {
        self.status
    }

    /// Returns values retained for one declared response-header interest.
    pub fn header_values<'a>(
        &'a self,
        name: &'a HeaderName,
    ) -> impl Iterator<Item = &'a HeaderValue> + 'a {
        self.headers
            .iter()
            .filter(move |(candidate, _)| candidate == name)
            .map(|(_, value)| value)
    }

    /// Returns the logical current page URI when the description requested it.
    #[must_use]
    pub const fn current_uri(&self) -> Option<&Url> {
        self.current_uri.as_ref()
    }

    /// Returns safe operation/page correlation.
    #[must_use]
    pub const fn correlation(&self) -> PageCorrelation {
        self.correlation
    }

    /// Borrows the exact decoded page.
    #[must_use]
    pub const fn page(&self) -> &Page {
        &self.page
    }

    /// Extracts the exact decoded page.
    #[must_use]
    pub fn into_page(self) -> Page {
        self.page
    }
}

impl<Page> fmt::Debug for PageResponse<Page> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PageResponse")
            .field("status", &self.status)
            .field("header_value_count", &self.headers.len())
            .field("has_current_uri", &self.current_uri.is_some())
            .field("correlation", &self.correlation)
            .field("page", &"<application page omitted>")
            .finish()
    }
}

/// Remaining core aggregate capacity supplied to one synchronous page decision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PageDecisionLimits {
    remaining_items: usize,
}

impl PageDecisionLimits {
    pub(crate) const fn new(remaining_items: usize) -> Self {
        Self { remaining_items }
    }

    /// Returns the maximum number of additional items core can retain.
    #[must_use]
    pub const fn remaining_items(self) -> usize {
        self.remaining_items
    }
}

/// Atomic result of interpreting one complete page.
pub enum PageStep<Controller, Item> {
    /// Natural pagination completion with this page's items.
    Done {
        /// Eager finite items extracted from the page.
        items: Vec<Item>,
    },
    /// Continue with this page's items and the next controller state.
    Continue {
        /// Eager finite items extracted from the page.
        items: Vec<Item>,
        /// Complete next controller state.
        next: Controller,
    },
}

/// Deterministic application-defined page interpretation and continuation state.
///
/// Methods are synchronous, run outside core locks, must be prompt and finite,
/// and must perform no external I/O. Core checks the absolute deadline before
/// and after calls but cannot preempt arbitrary blocking Rust code.
pub trait PaginationController<Page>: Sized + Send + 'static {
    /// One eagerly extracted aggregate item.
    type Item: Send + 'static;

    /// Bounded response metadata required by this controller.
    const DESCRIPTION: PaginationDescription;

    /// Validates the current state before one page request is prepared.
    fn validate(&self) -> Result<(), PaginationError> {
        Ok(())
    }

    /// Atomically extracts items and selects completion or the next state.
    fn advance(
        self,
        page: PageResponse<Page>,
        limits: PageDecisionLimits,
    ) -> Result<PageStep<Self, Self::Item>, PaginationError>;
}

#[cfg(test)]
mod tests {
    use http::{HeaderMap, HeaderValue, Method, StatusCode, Version};
    use url::Url;

    use super::*;
    use crate::endpoint::EndpointId;
    use crate::response::ExecutionMeta;

    fn accepted(page: &'static str, headers: HeaderMap) -> ApiResponse<&'static str> {
        ApiResponse::new(
            Url::parse("https://example.test/items?cursor=public-uri-sentinel").expect("test URL"),
            StatusCode::PARTIAL_CONTENT,
            Version::HTTP_2,
            headers,
            ExecutionMeta::new(
                EndpointId::new("pagination.controller.test").expect("endpoint ID"),
                Method::GET,
                3,
                1,
                0,
                false,
            ),
            page,
        )
    }

    #[test]
    fn page_response_retains_only_declared_bounded_metadata_and_redacts_debug() {
        let mut headers = HeaderMap::new();
        headers.append(
            "link",
            HeaderValue::from_static("</items?cursor=next>; rel=next"),
        );
        headers.insert(
            "x-not-declared",
            HeaderValue::from_static("private-header-sentinel"),
        );
        let aggregate = OperationId::allocate().expect("aggregate ID");
        let page_id = OperationId::allocate().expect("page ID");
        let response = PageResponse::from_accepted(
            accepted("private-page-sentinel", headers),
            PaginationDescription::new(&["link"], true),
            aggregate,
            page_id,
            3,
        )
        .expect("bounded page response");

        assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(response.header_values(&http::header::LINK).count(), 1);
        assert_eq!(
            response
                .header_values(&HeaderName::from_static("x-not-declared"))
                .count(),
            0
        );
        assert!(response.current_uri().is_some());
        assert_eq!(response.correlation().aggregate_operation_id(), aggregate);
        assert_eq!(response.correlation().page_operation_id(), page_id);
        assert_eq!(response.correlation().page_index(), 3);

        let debug = format!("{response:?}");
        for forbidden in [
            "private-page-sentinel",
            "private-header-sentinel",
            "public-uri-sentinel",
            "cursor=next",
        ] {
            assert!(!debug.contains(forbidden));
        }
    }

    #[test]
    fn invalid_interests_and_first_excess_metadata_byte_fail_before_controller() {
        let operation = OperationId::allocate().expect("operation ID");
        let duplicate = PageResponse::from_accepted(
            accepted("page", HeaderMap::new()),
            PaginationDescription::new(&["link", "Link"], false),
            operation,
            operation,
            0,
        )
        .expect_err("duplicate normalized interests");
        assert_eq!(
            duplicate.kind(),
            PaginationErrorKind::InvalidControllerDescription
        );

        let private = PageResponse::from_accepted(
            accepted("page", HeaderMap::new()),
            PaginationDescription::new(&["set-cookie"], false),
            operation,
            operation,
            0,
        )
        .expect_err("private header interest");
        assert_eq!(
            private.kind(),
            PaginationErrorKind::InvalidControllerDescription
        );

        let mut headers = HeaderMap::new();
        headers.insert(
            "x-large",
            HeaderValue::from_bytes(&vec![b'a'; MAX_PAGINATION_HEADER_BYTES])
                .expect("valid large header"),
        );
        let excess = PageResponse::from_accepted(
            accepted("page", headers),
            PaginationDescription::new(&["x-large"], false),
            operation,
            operation,
            0,
        )
        .expect_err("name bytes put retained metadata above ceiling");
        assert_eq!(
            excess.kind(),
            PaginationErrorKind::PageMetadataCeilingExceeded
        );
    }
}
