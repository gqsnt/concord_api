use crate::error::{ApiClientError, ErrorContext};
use crate::pagination::{
    EndpointPagination, PageAdvance, PageApply, PageDecision, PageItems, ProgressKey,
};
use std::num::NonZeroUsize;

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
pub struct CursorPagination<C = String> {
    /// Initial cursor (usually None).
    pub cursor: Option<C>,
    /// Page size (must be > 0).
    pub per_page: u64,

    /// If false, first request omits the cursor param when `cursor` is None.
    pub send_cursor_on_first: bool,

    /// If true, stop when response has no cursor (None) after collecting that page.
    pub stop_when_cursor_missing: bool,
}

impl Default for CursorPagination<String> {
    fn default() -> Self {
        Self {
            cursor: None,
            per_page: 20,
            send_cursor_on_first: false,
            stop_when_cursor_missing: true,
        }
    }
}

impl<Page> EndpointPagination<Page> for CursorPagination<String>
where
    Page: PageItems + HasNextCursor<Cursor = String>,
{
    fn apply(&mut self, ctx: PageApply<'_>) -> Result<(), ApiClientError> {
        validate_per_page(self.per_page, "cursor", ctx.ctx)?;
        if ctx.page_index == 0 && !self.send_cursor_on_first {
            self.cursor = None;
        }
        Ok(())
    }

    fn expected_items_per_page(&self) -> Option<NonZeroUsize> {
        usize::try_from(self.per_page)
            .ok()
            .and_then(NonZeroUsize::new)
    }

    fn advance(
        &mut self,
        page: &Page,
        _ctx: PageAdvance<'_>,
    ) -> Result<PageDecision, ApiClientError> {
        let _ = validate_per_page(
            self.per_page,
            "cursor",
            &ErrorContext {
                endpoint: "pagination",
                method: http::Method::GET,
            },
        )?;
        self.cursor = page.next_cursor();
        if self.cursor.is_none() && self.stop_when_cursor_missing {
            return Ok(PageDecision::Stop);
        }
        Ok(PageDecision::Continue)
    }

    fn progress_key(&self) -> Option<ProgressKey> {
        self.cursor
            .as_ref()
            .map(|cursor| ProgressKey::Str(cursor.to_string()))
    }
}

fn validate_per_page(
    value: u64,
    controller: &'static str,
    ctx: &ErrorContext,
) -> Result<NonZeroUsize, ApiClientError> {
    let value = usize::try_from(value).map_err(|_| ApiClientError::Pagination {
        ctx: ctx.clone(),
        msg: format!("{controller}: per_page does not fit in usize").into(),
    })?;
    NonZeroUsize::new(value).ok_or_else(|| ApiClientError::Pagination {
        ctx: ctx.clone(),
        msg: format!("{controller}: per_page must be greater than zero").into(),
    })
}
