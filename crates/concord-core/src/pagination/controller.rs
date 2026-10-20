use std::fmt;

use http::{HeaderMap, HeaderName, HeaderValue, StatusCode};
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
///
/// Core builds this view only after the page was completely acquired and
/// decoded by its `Buffered` response representation. It retains the declared
/// response-header interests (at most [`MAX_PAGINATION_HEADER_INTERESTS`],
/// at most [`MAX_PAGINATION_HEADER_BYTES`] of retained evidence, never
/// secret/category-denied headers) and, when requested, the
/// credential-sanitized current logical URI. `Debug` never prints the page,
/// header values or URI.
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
        let headers = retain_declared_headers(response.headers(), description)?;
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

    /// Builds a page view outside any traversal, for unit-testing a
    /// controller's [`PaginationController::advance`] without a client.
    ///
    /// The same declared-interest filtering, validation and retained-byte
    /// ceiling as a real traversal apply: only headers named by `description`
    /// are retained and `current_uri` is kept only when the description asks
    /// for it. The correlation carries freshly allocated operation identities
    /// that belong to no traversal. The view grants no request, credential or
    /// transport authority.
    ///
    /// # Errors
    ///
    /// Returns `InvalidControllerDescription` for excessive, invalid,
    /// duplicate or secret header interests, `PageMetadataCeilingExceeded`
    /// when retained evidence exceeds its ceiling, and `Overflow` if operation
    /// identities are exhausted.
    pub fn detached(
        page: Page,
        status: StatusCode,
        headers: &HeaderMap,
        current_uri: &Url,
        description: PaginationDescription,
        page_index: u32,
    ) -> Result<Self, PaginationError> {
        let retained = retain_declared_headers(headers, description)?;
        let allocate = || {
            OperationId::allocate().map_err(|_| PaginationError::new(PaginationErrorKind::Overflow))
        };
        Ok(Self {
            page,
            status,
            headers: retained,
            current_uri: description.include_current_uri.then(|| current_uri.clone()),
            correlation: PageCorrelation {
                aggregate_operation_id: allocate()?,
                page_operation_id: allocate()?,
                page_index,
            },
        })
    }
}

fn retain_declared_headers(
    source: &HeaderMap,
    description: PaginationDescription,
) -> Result<Vec<(HeaderName, HeaderValue)>, PaginationError> {
    if description.response_headers.len() > MAX_PAGINATION_HEADER_INTERESTS {
        return Err(PaginationError::new(
            PaginationErrorKind::InvalidControllerDescription,
        ));
    }

    let mut parsed_names = Vec::with_capacity(description.response_headers.len());
    for raw_name in description.response_headers {
        let name = HeaderName::from_bytes(raw_name.as_bytes())
            .map_err(|_| PaginationError::new(PaginationErrorKind::InvalidControllerDescription))?;
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
        for value in source.get_all(&name) {
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
    Ok(headers)
}

impl<Page> PageResponse<Page> {
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
///
/// Core enforces the item ceiling itself against the complete batch returned
/// by [`PaginationController::advance`]; a controller may use this value to
/// fail early with a precise error instead of building an oversized batch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PageDecisionLimits {
    remaining_items: usize,
}

impl PageDecisionLimits {
    /// Creates decision limits with `remaining_items` of capacity.
    ///
    /// Core supplies the real value during a traversal; this constructor
    /// exists so controllers can be unit-tested with [`PageResponse::detached`].
    #[must_use]
    pub const fn new(remaining_items: usize) -> Self {
        Self { remaining_items }
    }

    /// Returns the maximum number of additional items core can deliver.
    #[must_use]
    pub const fn remaining_items(self) -> usize {
        self.remaining_items
    }
}

/// Atomic result of interpreting one complete page.
pub enum PageStep<C, I> {
    /// The controller's provider-contract proof of exhaustion, with this
    /// page's items. No further page is requested.
    Done {
        /// Finite items extracted from the page.
        items: Vec<I>,
    },
    /// Continue with the next controller state and this page's items.
    ///
    /// Core derives the next request from `next.cursor()` and validates it
    /// (public shaping, fixed-fact invariants and cycle detection) before the
    /// current page is delivered.
    Continue {
        /// Complete next controller state.
        next: C,
        /// Finite items extracted from the page.
        items: Vec<I>,
    },
}

/// Application-defined page interpretation and continuation state.
///
/// A controller is started from a public [`Init`](Self::Init) value (built by
/// the declaration's `init` clause) and owns private state that can evolve
/// without a DSL edit while `Init` and `Cursor` stay compatible. Core calls the
/// methods with fully qualified syntax for the declared page type `P`, so an
/// unrelated inherent method of the same name is never selected.
///
/// # Cursor mapping
///
/// [`cursor`](Self::cursor) returns the **entire** value mapped by the
/// declaration's `cursor -> slot` (or tuple) destination. Core applies it to a
/// fresh page-stable copy of the original endpoint; it never extracts fields,
/// wraps values in `Option` or converts types. Before the first dispatch core
/// checks that the initial cursor reproduces the current request's canonical
/// public identity (`PaginationErrorKind::InvalidInitialCursor` otherwise).
///
/// # Extension contract
///
/// * **Ownership.** One controller value belongs to exactly one traversal; it
///   is moved into [`advance`](Self::advance) and replaced by the returned
///   next state.
/// * **Concurrency.** A controller must be `Send` (a traversal may move
///   between threads between polls) but never needs `Sync` or `Clone`: core
///   never shares or duplicates it.
/// * **Cancellation.** Methods are synchronous, run outside core locks, must
///   be prompt and finite, and must perform no external I/O. They run under
///   the traversal's single aggregate deadline: core checks it before and
///   after every call but cannot preempt arbitrary blocking Rust code.
///   Dropping a traversal drops the controller without further calls.
/// * **Input sensitivity.** The decoded page, declared response headers,
///   current logical URI and cursor values may contain sensitive application
///   data. Controllers own every intentional use of them; returned
///   [`PaginationError`]s carry only a kind and caller-reviewed safe text,
///   and core never prints cursors or pages.
/// * **Errors.** An error from `start` fails the traversal before any request
///   is shaped. An error from `advance` withholds the current page's items;
///   the page is still recorded as received in the
///   [`TraversalSummary`](super::TraversalSummary). Controller errors are
///   terminal, never retried.
/// * **Resource obligations.** The returned batch is checked against the
///   remaining item ceiling as a whole (it is never truncated). Core bounds
///   wire bytes, not the heap growth of a custom decoder or controller;
///   controllers must keep their own state finite.
/// * **Negative capabilities.** A controller cannot send requests, read
///   credentials, choose origins, change rate-limit dimensions, effects,
///   accepted statuses or timeouts, or mutate any endpoint value other than
///   the declared cursor destination. Continuations that would change such
///   fixed facts, repeat an earlier request, or exceed a traversal budget are
///   rejected by core.
#[diagnostic::on_unimplemented(
    message = "[C3-PAGE] `{Self}` is not a pagination controller for page type `{P}`",
    label = "selected by `paginate via` for a `{P}` page",
    note = "the controller must implement `PaginationController<{P}>`, where `{P}` is the endpoint's declared result (one decoded page)",
    note = "select a controller for this page type with `paginate via Controller {{ ... }}`, or declare the page type the controller accepts as the endpoint result"
)]
pub trait PaginationController<P>: Sized + Send + 'static {
    /// One item extracted from a page.
    type Item: Send + 'static;
    /// Public initialization data constructed by the declaration's `init` clause.
    type Init: Send + 'static;
    /// Whole value mapped onto the declared cursor destination slot(s).
    type Cursor: Clone + Send + Sync + 'static;

    /// Bounded response metadata required by this controller.
    const DESCRIPTION: PaginationDescription;

    /// Creates the initial private state from public initialization data.
    fn start(init: Self::Init) -> Result<Self, PaginationError>;

    /// Returns the whole cursor value for the request this state describes.
    fn cursor(&self) -> Self::Cursor;

    /// Atomically extracts items and selects completion or the next state.
    fn advance(
        self,
        page: PageResponse<P>,
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
