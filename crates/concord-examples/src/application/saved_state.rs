//! Saved state versus CLI opinions (§23.3).
//!
//! Saved `None` means exact absence (`replace_category(None)`); CLI `None`
//! means "no new opinion" (`set_category_if_some(None)` keeps the current
//! value). Adding a declaration default later does not make those intents
//! synonyms. The returned `Vec` is one response, never an undisclosed full
//! traversal.

// ANCHOR: saved_state
use std::num::NonZeroU32;

use concord::{ApiError, CallOptions};

use super::execute::execute;
use crate::contracts::catalog_v3::{ItemSummary, catalog_api};

/// A list configuration the application saved earlier.
pub struct SavedList {
    /// Saved category; `None` is an explicit "no category" choice.
    pub category: Option<String>,
    /// Saved page size.
    pub count: NonZeroU32,
}

/// Command-line overrides; each `None` means "no new opinion".
#[derive(Default)]
pub struct CliListPatch {
    /// Category override.
    pub category: Option<String>,
    /// Page-size override.
    pub count: Option<NonZeroU32>,
}

/// Builds the inert list request: saved state first, then CLI opinions.
pub fn list_request(
    scope: &catalog_api::regional::Regional,
    saved: SavedList,
    cli: CliListPatch,
) -> catalog_api::regional::ItemSummariesRequest {
    scope
        .item_summaries(Vec::new())
        .replace_category(saved.category)
        .set_count(saved.count)
        .set_category_if_some(cli.category)
        .set_count_if_some(cli.count)
}

/// Executes one configured list page under the caller's limits.
///
/// # Errors
///
/// The single request's failure.
pub async fn configured_list(
    scope: &catalog_api::regional::Regional,
    saved: SavedList,
    cli: CliListPatch,
    limits: &CallOptions,
) -> Result<Vec<ItemSummary>, ApiError> {
    let request = list_request(scope, saved, cli);
    execute(request, limits).await
}
// ANCHOR_END: saved_state
