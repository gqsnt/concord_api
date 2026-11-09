//! Public MediaWiki queries through generated Concord operations.
//!
//! Supply the identifying User-Agent and checked complete script URL in `MediaWikiConfig`.
//! Generated `send()` applies the SDK's envelope policy; `prepare()?.send()` applies only
//! Concord's HTTP/JSON handling. All tests use loopback fixtures. See the package README.

#![doc = include_str!("../README.md")]

mod batch;
mod error;
mod models;
mod pagination;
mod policy;

pub use batch::{PageAnswer, PageIdLimit, PageIds, TooManyPageIds, Unavailable};
pub use concord;
pub use concord_pagination as pagination_engine;
pub use error::{ApiProblem, Error, ErrorDocument, HttpFailure};
pub use models::*;
pub use pagination::CategoryPages;
pub use policy::QueryPolicy;

use concord::{Json, reqwest::header::USER_AGENT};
use models::Row;

/// Maximum bytes in each successful query body (1 MiB).
pub const BODY_LIMIT: usize = 1024 * 1024;
/// Maximum bytes inspected in a non-success HTTP body (64 KiB).
pub const ERROR_BODY_LIMIT: usize = 64 * 1024;
/// Maximum bytes in each successful batch-content body (16 MiB): up to 500 pages of content
/// arrive in one response until the wiki's own result-size limit makes it continue.
pub const BATCH_BODY_LIMIT: usize = 16 * 1024 * 1024;

concord::api! {
    /// A single wiki's public Action API. The caller supplies its identifying User-Agent.
    pub client MediaWiki(origin: concord::BaseUrl, #[sensitive] header(USER_AGENT) user_agent: String) {
        base origin;
        response Json;
        limit BODY_LIMIT;
        send QueryPolicy;

        /// Public JSON queries using the modern formatversion=2 response shape.
        scope query {
            query { "action" = "query"; "format" = "json"; "formatversion" = "2"; }

            /// Search one page of titles; choose a returned page ID for `fetch_page`.
            GET search(
                query srsearch: String,
                query srlimit: u16 = 10,
                query(..) continuation?: Continuation,
            ) -> Envelope<SearchResults> {
                query { "list" = "search"; "srprop" = ""; }
            }

            /// Fetch the latest revision's raw main-slot content for one page ID.
            GET content(query pageids: u64) -> Envelope<ContentResults> {
                query {
                    "prop" = "revisions"; "rvprop" = "ids|content";
                    "rvslots" = "main"; "rvlimit" = "1";
                }
            }

            /// One response of the latest revisions' main-slot content for several page IDs.
            /// Prefer `fetch_pages`, which follows continuation and answers every ID.
            /// `rvlimit` is single-page only (API:Revisions), so it is not sent here.
            GET contents(
                query pageids: PageIds,
                query(..) continuation?: Continuation,
            ) -> Envelope<ContentResults> {
                query { "prop" = "revisions"; "rvprop" = "ids|content"; "rvslots" = "main"; }
                limit BATCH_BODY_LIMIT;
            }

            /// Read one category page. Carry the entire returned continuation forward.
            GET category_members(
                query cmtitle: String,
                query cmlimit: u16 = 50,
                query(..) continuation?: Continuation,
            ) -> Envelope<CategoryResults> {
                query { "list" = "categorymembers"; }
            }
        }
    }
}

impl MediaWikiQuery {
    /// Fetch the chosen search hit's content, refusing missing or suppressed content.
    pub async fn fetch_page(&self, pageid: u64) -> Result<PageContent, Error> {
        let response = self.content(pageid).send().await?;
        if response.continuation.is_some() || response.data.pages.len() != 1 {
            return Err(Error::InvalidResponse(Box::new(response.meta)));
        }
        let page = response
            .data
            .pages
            .into_iter()
            .next()
            .expect("one page checked");
        if page.pageid != Some(pageid) {
            return Err(Error::ContentUnavailable);
        }
        match page.project(pageid) {
            Row::Content(content) => Ok(content),
            Row::Unavailable(_) | Row::NoRevision => Err(Error::ContentUnavailable),
            Row::Inconsistent => Err(Error::InvalidResponse(Box::new(response.meta))),
        }
    }
}
