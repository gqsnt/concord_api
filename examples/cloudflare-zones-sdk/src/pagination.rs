//! Page walkers over the two page-numbered listings, built on `concord-pagination`.
//!
//! Both listings document the same `result_info` (`page`, `per_page`, `count`, `total_count`,
//! `total_pages`) and different page sizes (List Zones 5..=50, List DNS Records 1..=5,000,000:
//! [`ZonesPerPage`](crate::ZonesPerPage), [`RecordsPerPage`](crate::RecordsPerPage)). The
//! continuation rule, stated once:
//!
//! - page `n` is requested with `page=n` (the template's `page` is replaced; the first is 1);
//! - the response must say `result_info.page == n` and carry `result_info.total_pages`, or the
//!   walk stops with [`Error::Envelope`] (never a silent end);
//! - the next page is `n + 1` while `n < total_pages`; otherwise the listing is complete.
//!   `total_count` (documented as the count *without* search parameters) and the number of
//!   items are never used to decide completeness.
//!
//! A failed page — `success: false` included — ends the walk with
//! `PagesError::Provider { error, failed, .. }`: the pages already returned stay with the caller
//! and `failed` is the page to resume from with `pages_from`. A listing that changes between
//! requests can repeat or skip items; Cloudflare offers no snapshot on these endpoints.

use std::num::NonZeroU32;

use concord::Sends;
use concord_pagination::{Page, Pages, Resume};

use crate::error::{EnvelopeError, EnvelopeProblem, Error, ResponseMeta};
use crate::models::{DnsRecord, Zone};
use crate::policy::PageResponse;
use crate::wire::{PageInfo, ResponseInfo};
use crate::{CloudflareZoneDnsRecordsList, CloudflareZonesList, Paging};

/// What a walker keeps of each page besides its items.
#[derive(Debug)]
#[non_exhaustive]
pub struct PageMeta {
    /// `result_info.page`, as checked against the request.
    pub page: NonZeroU32,
    /// `result_info.total_pages` of this response.
    pub total_pages: u32,
    /// The whole `result_info`.
    pub result_info: PageInfo,
    /// `messages`.
    pub messages: Vec<ResponseInfo>,
    /// `errors` sent beside `success: true`.
    pub errors: Vec<ResponseInfo>,
    /// The response head.
    pub meta: ResponseMeta,
}

/// A List Zones walk: `next_page()`, `items()`, `collect_complete()`, `max_pages(..)`.
pub type ZonePages = Pages<Zone, NonZeroU32, PageMeta, Error>;

/// A List DNS Records walk.
pub type RecordPages = Pages<DnsRecord, NonZeroU32, PageMeta, Error>;

impl CloudflareZonesList {
    /// Every page of this listing, from page 1, with its filters and `per_page`.
    pub fn pages(self) -> ZonePages {
        walk(self, Resume::Start)
    }

    /// The pages from `from` on (the `failed` or `resume` position of a `PagesError`). Use `concord_sdk_cloudflare_zones::concord_pagination::Resume` for the position. `Resume::Start` repeats page 1 of the zones listing, replacing the template's page; `Resume::At(key)` takes the `NonZeroU32` page number in a returned page's `next`.
    pub fn pages_from(self, from: Resume<NonZeroU32>) -> ZonePages {
        walk(self, from)
    }
}

impl CloudflareZoneDnsRecordsList {
    /// Every page of this listing, from page 1, with its filters and `per_page`.
    pub fn pages(self) -> RecordPages {
        walk(self, Resume::Start)
    }

    /// The pages from `from` on (the `failed` or `resume` position of a `PagesError`). Use `concord_sdk_cloudflare_zones::concord_pagination::Resume` for the position. `Resume::Start` repeats page 1 of the DNS records listing, replacing the template's page; `Resume::At(key)` takes the `NonZeroU32` page number in a returned page's `next`.
    pub fn pages_from(self, from: Resume<NonZeroU32>) -> RecordPages {
        walk(self, from)
    }
}

fn walk<E, T>(template: E, from: Resume<NonZeroU32>) -> Pages<T, NonZeroU32, PageMeta, Error>
where
    E: Sends<Result<PageResponse<T>, Error>> + Paging + Clone + Send + 'static,
    T: Send + 'static,
{
    Pages::new(from, move |page: Option<NonZeroU32>| {
        let requested = page.unwrap_or(NonZeroU32::MIN);
        let request = template.clone().set_page(requested);
        async move { next(requested, request.send().await?) }
    })
}

fn next<T>(
    requested: NonZeroU32,
    response: PageResponse<T>,
) -> Result<Page<T, NonZeroU32, PageMeta>, Error> {
    let PageResponse {
        result,
        result_info,
        messages,
        errors,
        meta,
    } = response;
    let unusable =
        |meta, problem| Err(Error::Envelope(Box::new(EnvelopeError::new(meta, problem))));
    let Some(
        info @ PageInfo {
            page: Some(page),
            total_pages: Some(total_pages),
            ..
        },
    ) = result_info
    else {
        return unusable(meta, EnvelopeProblem::MissingPageInfo);
    };
    if page != requested.get() {
        return unusable(
            meta,
            EnvelopeProblem::PageMismatch {
                requested,
                returned: page,
            },
        );
    }
    let next = if page < total_pages {
        // `page < total_pages <= u32::MAX`, so `page + 1` fits.
        requested.checked_add(1)
    } else {
        None
    };
    Ok(Page {
        items: result,
        next,
        meta: PageMeta {
            page: requested,
            total_pages,
            result_info: info,
            messages,
            errors,
            meta,
        },
    })
}
