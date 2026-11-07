#![doc = include_str!("../README.md")]

mod atom;
mod error;
mod input;
mod model;
mod policy;
mod query;

pub use atom::Atom;
pub use error::{ApiFailure, Error, HttpFailure};
pub use input::{IdList, PageSize, SearchTerm};
pub use model::{Author, Category, Feed, Link, Paper, Problem};
pub use policy::Policy;
pub use query::{Field, Query, SortBy, SortOrder};
pub use {concord, concord_pagination};

use concord::{Sends, reqwest};
use concord_pagination::{Page, Pages, Resume, next_offset};
use std::{num::NonZeroUsize, time::Duration};

/// Maximum success document size: 2 MiB, before XML parsing.
pub const BODY_LIMIT: usize = 2 * 1024 * 1024;
/// Maximum provider error document size: 64 KiB.
pub const ERROR_BODY_LIMIT: usize = 64 * 1024;

concord::api! {
    /// Public arXiv metadata. Reuse the client or its policy to share pacing.
    pub client Arxiv(policy: Policy) {
        base "https://export.arxiv.org/api";
        response Atom;
        limit BODY_LIMIT;
        send policy -> Result<_, Error>;

        /// The `start`/`max_results` result window of `search` and `query` (manual §3.1.1.2).
        /// The SDK's offset traversal is written once over this trait.
        params Paging(
            query start: u32 = 0,
            query("max_results") size: PageSize = PageSize::default(),
        );

        /// Term search, typed queries and direct ID lookup share the query route,
        /// with distinct inputs.
        scope papers {
            /// A literal term, searched across all indexed fields.
            GET search(
                #[sensitive] query("search_query") term: SearchTerm,
                ..Paging,
            ) -> Feed { path "query"; }

            /// A typed field/Boolean query, optionally sorted; paged like `search`.
            /// Without `sort_by`/`sort_order` neither key is sent: the provider's order applies.
            GET query(
                #[sensitive] query("search_query") expr: Query,
                ..Paging,
                query("sortBy") sort_by?: SortBy,
                query("sortOrder") sort_order?: SortOrder,
            ) -> Feed { path "query"; }

            /// Fetch metadata for up to 50 supplied identifiers, including version suffixes.
            GET fetch(#[sensitive] query("id_list") ids: IdList) -> Feed {
                path "query";
                query { "start" = "0"; "max_results" = "50"; }
            }
        }
    }
}

impl Arxiv {
    /// Construct with a supplied transport and a fresh three-second pacing policy.
    pub fn with_http(http: reqwest::Client) -> Self {
        Self::new(
            http,
            ArxivConfig {
                policy: Policy::default(),
            },
        )
    }
}

/// A transport with a 30-second timeout, no redirects, no native retries, and no proxy.
/// Supplying another transport makes its redirect/retry behavior the caller's responsibility.
pub fn http_client() -> reqwest::Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .no_proxy()
        .user_agent("concord-sdk-arxiv/0.1.0")
        .build()
}

impl ArxivPapersSearch {
    /// Traverse with a mandatory page budget, retaining failures and resumable offsets.
    /// `Resume::Start` begins at this request's own `start` (0 unless set); `Resume::At(n)`
    /// replaces it. `from` belongs to this term and page size. The provider's 30,000-result
    /// boundary produces an explicit error if further results remain. Listings are not
    /// snapshots.
    pub fn pages(self, from: Resume<u32>, max_pages: NonZeroUsize) -> Pages<Paper, u32, (), Error> {
        offset_pages(self, from, max_pages)
    }
}

impl ArxivPapersQuery {
    /// Traverse exactly as [`ArxivPapersSearch::pages`] does. `from` belongs to this query,
    /// sort and page size; every page repeats them.
    pub fn pages(self, from: Resume<u32>, max_pages: NonZeroUsize) -> Pages<Paper, u32, (), Error> {
        offset_pages(self, from, max_pages)
    }
}

/// The one offset traversal: each page re-sends the request through the client's policy
/// (pacing, window and paging checks) with `start` replaced, or unchanged for `Resume::Start`.
fn offset_pages<E>(
    request: E,
    from: Resume<u32>,
    max_pages: NonZeroUsize,
) -> Pages<Paper, u32, (), Error>
where
    E: Paging + Sends<Result<Feed, Error>> + Clone + Send + 'static,
{
    Pages::new(from, move |offset| {
        let request = match offset {
            Some(offset) => request.clone().set_start(offset),
            None => request.clone(),
        };
        async move {
            let feed = request.send().await?;
            let next = next_offset(
                feed.start,
                feed.items_per_page,
                feed.entries.len() as u64,
                Some(feed.total_results),
            )
            .map_err(|_| Error::Pagination("inconsistent offset page"))?;
            let next = next
                .map(|n| {
                    u32::try_from(n).map_err(|_| Error::Pagination("offset exceeds SDK range"))
                })
                .transpose()?;
            Ok(Page::new(feed.entries, next))
        }
    })
    .max_pages(max_pages)
}
