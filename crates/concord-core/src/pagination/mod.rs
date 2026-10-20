//! Application-controlled pagination: the controller contract, lazy and
//! collecting terminals, and truthful traversal summaries.
//!
//! A pageable generated request offers three terminals, all inert until
//! polled:
//!
//! * `request.pages(maximum_pages)` → [`Pages`], a caller-driven lazy
//!   traversal whose [`Pages::next_page`] fetches one page per await;
//! * `request.collect_prefix(maximum_pages)` → [`CollectPrefix`], awaiting to
//!   an explicit [`Prefix`] with its [`TraversalSummary`];
//! * `request.collect_all(maximum_pages)` → [`CollectAll`], awaiting to every
//!   item only after proven exhaustion, or a completion-limit error.
//!
//! Every traversal has one aggregate operation identity and one absolute
//! deadline started at its first polled fetch; the deadline covers controller
//! calls, every page and attempt, and time the application spends between
//! polls. Continuation is not a retry: each page is a new child operation with
//! its own identity and rate admission, sharing the aggregate budgets:
//!
//! * pages: the requested bound intersected with the client page ceiling
//!   (both disclosed in the summary);
//! * delivered items: the client item ceiling, checked against whole batches;
//! * acquired response bytes: every nonempty body byte read for any attempt
//!   (accepted pages, rejected statuses, failed partial acquisitions),
//!   intersected with the caller's response-byte cap;
//! * handoffs: the client aggregate send ceiling (`aggregate_max_sends`,
//!   [`DEFAULT_TRAVERSAL_MAX_HANDOFFS`] by default) intersected with the
//!   caller's send cap, counting first submissions of later pages; replays: the caller's
//!   replay cap, counting policy retries and authentication recovery; neither
//!   resets per page, and each page has at most [`MAX_PAGE_ATTEMPTS`] attempts;
//! * identity storage: the client identity ceiling bounds the complete
//!   canonical identity vectors retained for cycle detection (a hash only
//!   indexes them; equality is never decided by the hash alone).
//!
//! Custom page shapes need no iterable marker: any response representation
//! with the complete `Buffered` driver is pageable by a controller that knows
//! how to extract its items.

mod bridge;
mod controller;
mod engine;
mod error;
mod identity;
mod limits;
mod meter;
mod summary;
mod terminals;

pub use crate::error::PaginationErrorKind;
pub use controller::{
    MAX_PAGINATION_HEADER_BYTES, MAX_PAGINATION_HEADER_INTERESTS, PageCorrelation,
    PageDecisionLimits, PageResponse, PageStep, PaginationController, PaginationDescription,
};
pub use error::PaginationError;
pub use limits::{DEFAULT_TRAVERSAL_MAX_HANDOFFS, MAX_PAGE_ATTEMPTS};
pub use summary::{ContinuationKnowledge, StopReason, TraversalSummary};
pub use terminals::{CollectAll, CollectPrefix, Page, Pages, Prefix};

#[doc(hidden)]
pub use bridge::{
    ControllerCursor, ControllerInit, PageItem, PageableEndpoint, collect_all_request,
    collect_prefix_request, pages_request,
};

pub(crate) use meter::TraversalMeter;

#[cfg(test)]
mod runtime_tests;
#[cfg(test)]
mod tests;
