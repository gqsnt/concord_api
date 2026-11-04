//! One bounded page walk for both services.
//!
//! Catalog pages with `cursor` / `next_cursor` and Ledger with `page_token` / `paging.next`.
//! The walk itself (the bound, the stopping rule, accumulation, resumption and what happens when
//! a page fails) is written once, in [`traverse`], over a `fetch` closure that returns a [`Page`].
//! Each service only supplies a small adapter: [`catalog_items`] and [`ledger_entries`] set their
//! token on a clone of a request template and map their page type with [`catalog_page`] or
//! [`ledger_page`]. Concord has no pagination; nothing here is a Concord trait.
//!
//! # Policy
//!
//! - **Bound.** At most `max_pages` requests per call. The bound is a [`NonZeroUsize`], so a
//!   walk that may fetch nothing cannot be written. A service that keeps returning a token (even
//!   the same one) cannot make the walk run forever.
//! - **Complete or bounded.** [`Traversal::Complete`] means the last page fetched had no next
//!   token: the listing ended. [`Traversal::Bounded`] means the bound was reached while the service
//!   still had a next page; its [`Continuation`] is that page's token, and nothing is silently
//!   dropped.
//! - **Resumption.** Every call takes `from`: `None` starts at the first page, `Some(c)` starts at
//!   the page `c` designates. Records of one call followed by those of its resumption are the
//!   listing's records in order, without duplicates or gaps, provided the listing did not change
//!   between the calls. A continuation belongs to the request template it came from (same tenant,
//!   tags, account, page size); resuming with another template is the caller's error and is not
//!   detected (the tokens are the services' own opaque text).
//! - **A failed page.** The walk stops at the first error and returns [`Interrupted`]: the error,
//!   the records of the pages that succeeded **in this call**, and the `from` value that refetches
//!   the failed page. The caller decides whether to keep those records and resume from
//!   [`Interrupted::retry_from`] (the same records are not fetched twice), or to discard them and
//!   start over. Nothing is retried here; retrying is a separate policy.

use std::future::Future;
use std::num::NonZeroUsize;

use concord_example_sdk::{
    CatalogTenantListingList, Entry, EntryPage, Item, ItemPage, LedgerEntries,
};

/// One page as the walk sees it: its records and the service's token of the next page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page<T> {
    /// The page's records, in the service's order.
    pub records: Vec<T>,
    /// The next page's token; `None` on the last page.
    pub next: Option<String>,
}

/// Where to resume a walk: the service's token of the first page not yet fetched.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Continuation(String);

impl Continuation {
    /// A continuation from a token the service returned earlier (for example one the
    /// application stored).
    pub fn new(token: impl Into<String>) -> Self {
        Self(token.into())
    }

    /// The service's opaque token.
    pub fn token(&self) -> &str {
        &self.0
    }
}

/// A walk that ended without an error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Traversal<T> {
    /// The listing ended: the last page fetched had no next token.
    Complete {
        /// Records of every page fetched by this call, in order.
        records: Vec<T>,
        /// Pages fetched by this call.
        pages: usize,
    },
    /// The page bound was reached and the service has more.
    Bounded {
        /// Records of every page fetched by this call, in order.
        records: Vec<T>,
        /// Pages fetched by this call: the bound.
        pages: usize,
        /// Pass as `from` to continue with the first page not fetched.
        next: Continuation,
    },
}

impl<T> Traversal<T> {
    /// The records fetched by this call.
    pub fn records(&self) -> &[T] {
        match self {
            Self::Complete { records, .. } | Self::Bounded { records, .. } => records,
        }
    }

    /// The continuation, when the walk was bounded.
    pub fn next(&self) -> Option<&Continuation> {
        match self {
            Self::Complete { .. } => None,
            Self::Bounded { next, .. } => Some(next),
        }
    }
}

/// A walk stopped by a failed page. See the module's policy.
#[derive(Debug)]
pub struct Interrupted<T, E> {
    /// Why the page failed.
    pub error: E,
    /// Records of the pages that succeeded in this call, in order. Kept so the caller can choose:
    /// use them and resume, or drop them and start over.
    pub records: Vec<T>,
    /// Pages that succeeded in this call.
    pub pages: usize,
    /// Pass as `from` to refetch the failed page: `None` when it was the listing's first page.
    pub retry_from: Option<Continuation>,
}

/// Walks pages from `from` (`None`: the first page), at most `max_pages` of them.
///
/// `fetch` receives the token of the page to fetch (`None` for the first page) and returns that
/// page. It is called once per page, in order, and never after an error.
pub async fn traverse<T, E, F, Fut>(
    max_pages: NonZeroUsize,
    from: Option<Continuation>,
    mut fetch: F,
) -> Result<Traversal<T>, Interrupted<T, E>>
where
    F: FnMut(Option<String>) -> Fut,
    Fut: Future<Output = Result<Page<T>, E>>,
{
    let mut records = Vec::new();
    let mut pages = 0;
    let mut position = from;
    loop {
        let token = position.as_ref().map(|c| c.0.clone());
        let page = match fetch(token).await {
            Ok(page) => page,
            Err(error) => {
                return Err(Interrupted {
                    error,
                    records,
                    pages,
                    retry_from: position,
                });
            }
        };
        pages += 1;
        records.extend(page.records);
        let Some(next) = page.next else {
            return Ok(Traversal::Complete { records, pages });
        };
        let next = Continuation(next);
        if pages == max_pages.get() {
            return Ok(Traversal::Bounded {
                records,
                pages,
                next,
            });
        }
        position = Some(next);
    }
}

/// Catalog's page as the walk sees it.
pub fn catalog_page(page: ItemPage) -> Page<Item> {
    Page {
        records: page.items,
        next: page.next_cursor,
    }
}

/// Ledger's page as the walk sees it.
pub fn ledger_page(page: EntryPage) -> Page<Entry> {
    Page {
        records: page.data,
        next: page.paging.next,
    }
}

/// Walks a Catalog listing. Each page is a clone of `template` with its `cursor` set from the
/// position (cleared for the first page); everything else (tags, limit, region) is the
/// template's.
pub async fn catalog_items(
    template: &CatalogTenantListingList,
    max_pages: NonZeroUsize,
    from: Option<Continuation>,
) -> Result<Traversal<Item>, Interrupted<Item, concord::Error>> {
    traverse(max_pages, from, |cursor| {
        let request = template.clone().maybe_cursor(cursor);
        async move { request.send().await.map(catalog_page) }
    })
    .await
}

/// Walks Ledger entries. Each page is a clone of `template` with its `page_token` set from the
/// position (cleared for the first page); everything else (account, `from_ms`, page size) is the
/// template's.
pub async fn ledger_entries(
    template: &LedgerEntries,
    max_pages: NonZeroUsize,
    from: Option<Continuation>,
) -> Result<Traversal<Entry>, Interrupted<Entry, concord::Error>> {
    traverse(max_pages, from, |token| {
        let request = template.clone().maybe_page_token(token);
        async move { request.send().await.map(ledger_page) }
    })
    .await
}
