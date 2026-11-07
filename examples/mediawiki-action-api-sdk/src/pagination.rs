use std::num::NonZeroUsize;

use concord_pagination::{Page, Pages, Resume};

use crate::{Continuation, Error, MediaWikiQuery, PageIdentity};

/// Bounded category traversal, preserving incomplete prefixes and every continuation field.
pub type CategoryPages = Pages<PageIdentity, Continuation, (), Error>;

impl MediaWikiQuery {
    /// Traverse a category from a whole continuation, with a required request-count bound.
    ///
    /// An empty page with continuation still advances. `batchcomplete` does not end traversal.
    /// Only exhaustion yields `Ok(None)`; bounds/failures are typed pagination errors.
    /// Reuse the identical wiki, category and page size when resuming.
    pub fn category_pages(
        &self,
        category: impl Into<String>,
        page_size: u16,
        from: Option<Continuation>,
        max_pages: NonZeroUsize,
    ) -> CategoryPages {
        let template = self
            .category_members(category.into())
            .set_cmlimit(page_size);
        let resume = from.map_or(Resume::Start, Resume::At);
        Pages::new(resume, move |continuation: Option<Continuation>| {
            let request = template.clone().maybe_continuation(continuation);
            async move {
                let response = request.send().await?;
                Ok(Page::new(
                    response.data.categorymembers,
                    response.continuation,
                ))
            }
        })
        .max_pages(max_pages)
    }
}
