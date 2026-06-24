use std::borrow::Cow;

/// Output helper trait for cursor pagination.
/// The output type must both:
/// - expose items (PageItems)
/// - expose a "next cursor" value (HasNextCursor)
pub trait HasNextCursor {
    type Cursor: Clone + Eq + std::hash::Hash + ToString + Send + 'static;
    fn next_cursor(&self) -> Option<Self::Cursor>;
}

impl<T: Send + 'static> HasNextCursor for Vec<T> {
    type Cursor = String;

    fn next_cursor(&self) -> Option<Self::Cursor> {
        None
    }
}

/// Cursor pagination:
/// - request: cursor + per_page
/// - response: provides a "next cursor"
#[derive(Clone, Debug)]
pub struct CursorPagination {
    /// Query key for cursor (ex: "cursor", "pageCursor", "starting_after").
    pub cursor_key: Cow<'static, str>,
    /// Query key for per-page (ex: "per_page", "pageSize", "limit").
    pub per_page_key: Cow<'static, str>,

    /// Initial cursor (usually None).
    pub cursor: Option<String>,
    /// Page size (must be > 0).
    pub per_page: u64,

    /// If false, first request omits the cursor param when `cursor` is None.
    pub send_cursor_on_first: bool,

    /// If true, stop when response has no cursor (None/empty) after collecting that page.
    pub stop_when_cursor_missing: bool,
}

impl Default for CursorPagination {
    fn default() -> Self {
        Self {
            cursor_key: Cow::from("cursor"),
            per_page_key: Cow::from("per_page"),
            cursor: None,
            per_page: 20,
            send_cursor_on_first: false,
            stop_when_cursor_missing: true,
        }
    }
}
