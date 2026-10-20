//! Application-controlled search without lying about exhaustion (§23.4).
//!
//! A page bound is not "the item does not exist". Processing time between
//! loop iterations counts against the traversal's one aggregate deadline, and
//! the caller never reconstructs rate, auth or retry state to stop early.

// ANCHOR: find_summary
use concord::pagination::{StopReason, TraversalSummary};
use concord::{ApiError, CallOptions};

use crate::contracts::catalog_v3::{ItemSummary, catalog_api};

/// The truthful outcome of a bounded search.
#[derive(Debug)]
pub enum SearchResult<T> {
    /// The item was found; the traversal was stopped by the application.
    Found {
        /// The matching item.
        item: T,
        /// The traversal summary at the application stop.
        traversal: TraversalSummary,
    },
    /// The provider proved exhaustion without a match.
    Exhausted {
        /// The exhausted traversal summary.
        traversal: TraversalSummary,
    },
    /// The traversal stopped for another reason (for example the page bound);
    /// the item may still exist.
    Stopped {
        /// The traversal summary, including its stop reason.
        traversal: TraversalSummary,
    },
}

/// Searches the feed for `wanted` over at most `maximum_pages` pages.
///
/// # Errors
///
/// A page fetch failure (returned once by the traversal).
pub async fn find_summary_within(
    scope: &catalog_api::regional::Regional,
    wanted: &str,
    limits: &CallOptions,
    maximum_pages: u32,
) -> Result<SearchResult<ItemSummary>, ApiError> {
    let mut pages = scope.feed().with_options(limits).pages(maximum_pages);
    while let Some(page) = pages.next_page().await? {
        for item in page.into_items() {
            if item.id == wanted {
                return Ok(SearchResult::Found {
                    item,
                    traversal: pages.finish(),
                });
            }
        }
    }
    let summary = pages.finish();
    if summary.stop_reason() == StopReason::Exhausted {
        Ok(SearchResult::Exhausted { traversal: summary })
    } else {
        Ok(SearchResult::Stopped { traversal: summary })
    }
}

/// The §23.4 search with its canonical 100-page bound.
///
/// # Errors
///
/// A page fetch failure.
pub async fn find_summary(
    scope: &catalog_api::regional::Regional,
    wanted: &str,
    limits: &CallOptions,
) -> Result<SearchResult<ItemSummary>, ApiError> {
    find_summary_within(scope, wanted, limits, 100).await
}
// ANCHOR_END: find_summary
