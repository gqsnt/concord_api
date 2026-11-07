use crate::{Cursor, Error, EuropePmcSearch, MAX_PAGES, Publication};
use concord_pagination::{Page, Pages, Resume};
use std::{collections::BTreeSet, num::NonZeroUsize};

/// Wire format of a lite search traversal; it never changes during that traversal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchFormat {
    /// Strict JSON media type and Concord's standard JSON codec.
    Json,
    /// Strict XML media type and the SDK's XML decoder.
    Xml,
}

/// Bounded, dependent pages with the provider hit count as page metadata.
pub type SearchPages = Pages<Publication, Cursor, u64, Error>;

impl EuropePmcSearch {
    /// Builds a lazy traversal with a mandatory 1..=20 page cap. Calls generated send per page.
    /// The exact same query, page size and format must be used when resuming a checkpoint.
    /// Every repeated cursor (including a multi-page cycle) fails before its page is emitted.
    /// A short or empty page with a fresh cursor continues. No cursor means exhaustion.
    pub fn pages(
        &self,
        format: SearchFormat,
        from: Resume<Cursor>,
        max: NonZeroUsize,
    ) -> Result<SearchPages, Error> {
        if max.get() > MAX_PAGES {
            return Err(Error::InvalidInput("traversal page cap must be in 1..=20"));
        }
        let scope = self.clone();
        let mut requested = BTreeSet::new();
        Ok(Pages::new(from, move |cursor: Option<Cursor>| {
            let scope = scope.clone();
            let cursor = cursor.unwrap_or_else(Cursor::start);
            requested.insert(cursor.clone());
            let requested = requested.clone();
            async move {
                let response = match format {
                    SearchFormat::Json => scope.json().set_cursor(cursor).send().await?,
                    SearchFormat::Xml => scope.xml().set_cursor(cursor).send().await?,
                };
                if response
                    .next_cursor_mark
                    .as_ref()
                    .is_some_and(|c| requested.contains(c))
                {
                    return Err(Error::RepeatedCursor);
                }
                Ok(Page {
                    items: response.result_list.result,
                    next: response.next_cursor_mark,
                    meta: response.hit_count,
                })
            }
        })
        .max_pages(max))
    }
}
