use crate::{
    Cursor, Error, EuropePmcPublicationReferences, EuropePmcSearch, MAX_PAGES, PageNumber,
    PageSize, Publication, Reference, ReferencesPage, requested_page_size,
};
use concord_pagination::{OffsetError, Page, Pages, Resume, next_offset};
use std::{
    collections::BTreeSet,
    num::NonZeroUsize,
    sync::{Arc, OnceLock},
};

/// Wire format of a search or references traversal; it never changes during that traversal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchFormat {
    /// Strict JSON media type and Concord's standard JSON codec.
    Json,
    /// Strict XML media type and the SDK's XML decoder.
    Xml,
}

/// Bounded, dependent pages with the provider hit count as page metadata.
pub type SearchPages = Pages<Publication, Cursor, u64, Error>;

/// Bounded reference-list pages keyed by page number, with the hit count as page metadata.
pub type ReferencePages = Pages<Reference, PageNumber, u64, Error>;

fn check_cap(max: NonZeroUsize) -> Result<(), Error> {
    if max.get() > MAX_PAGES {
        return Err(Error::InvalidInput("traversal page cap must be in 1..=20"));
    }
    Ok(())
}

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
        check_cap(max)?;
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

impl EuropePmcPublicationReferences {
    /// Builds a lazy page-number traversal of this reference list, with a mandatory 1..=20 page
    /// cap. Calls generated send per page, always at this scope's page size.
    ///
    /// Pages are requested in order until the hit count is reached: every page before the last
    /// must be full and the last must hold exactly the remainder. A page that disagrees, that
    /// reports a hit count different from the first returned page, or that lies beyond the hit
    /// count fails with [`Error::InvalidPage`] before it is emitted; the pages already returned
    /// stay complete and `resume()` names the failed page. A cap reached before the last page
    /// is [`concord_pagination::PagesError::Bounded`] with the next page number.
    ///
    /// Resume with the same publication, page size and format: a page number addresses
    /// `(n - 1) * page size` records, which the SDK cannot check against an earlier run.
    /// A resumed traversal's first page re-establishes the hit count.
    pub fn pages(
        &self,
        format: SearchFormat,
        from: Resume<PageNumber>,
        max: NonZeroUsize,
    ) -> Result<ReferencePages, Error> {
        check_cap(max)?;
        // Generated handles have no getters; the declared value is read back without I/O.
        let size = requested_page_size(self.json().prepare()?.request())
            .and_then(|n| u16::try_from(n).ok())
            .ok_or(Error::InvalidInput("references page size is unreadable"))
            .and_then(PageSize::new)?;
        let scope = self.clone();
        let first_hits = Arc::new(OnceLock::new());
        Ok(Pages::new(from, move |page: Option<PageNumber>| {
            let scope = scope.clone();
            let first_hits = Arc::clone(&first_hits);
            let page = page.unwrap_or(PageNumber::FIRST);
            async move {
                let response = match format {
                    SearchFormat::Json => scope.json().set_page(page).send().await?,
                    SearchFormat::Xml => scope.xml().set_page(page).send().await?,
                };
                let next = next_reference_page(page, size, &response, &first_hits)?;
                Ok(Page {
                    items: response.reference_list.reference,
                    next,
                    meta: response.hit_count,
                })
            }
        })
        .max_pages(max))
    }
}

/// Page-number continuation: the shared offset check plus "a continuing page is full", since
/// page `n + 1` starts at `n * size` whatever page `n` returned.
fn next_reference_page(
    page: PageNumber,
    size: PageSize,
    response: &ReferencesPage,
    first_hits: &OnceLock<u64>,
) -> Result<Option<PageNumber>, Error> {
    let hits = response.hit_count;
    if *first_hits.get_or_init(|| hits) != hits {
        return Err(Error::InvalidPage(
            "hit count changed during the references traversal",
        ));
    }
    let limit = size.get() as u64;
    let len = response.reference_list.reference.len() as u64;
    let end = next_offset(page.offset(size), limit, len, Some(hits)).map_err(|e| {
        Error::InvalidPage(match e {
            OffsetError::BeyondCount => "references page lies beyond the hit count",
            OffsetError::DidNotAdvance => "empty references page before the hit count",
            OffsetError::TooManyItems => "references page exceeds its page size",
            OffsetError::ZeroLimit | OffsetError::Overflow => "references page arithmetic failed",
        })
    })?;
    match end {
        None => Ok(None),
        Some(_) if len == limit => page
            .next()
            .map(Some)
            .ok_or(Error::InvalidPage("references page number overflow")),
        Some(_) => Err(Error::InvalidPage(
            "short references page before the hit count",
        )),
    }
}
