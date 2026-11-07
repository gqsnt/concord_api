//! Public MediaWiki queries through generated Concord operations.
//!
//! Set an identifying User-Agent and the complete script URL with `with_base_url`.
//! Generated `send()` applies the SDK's envelope policy; `prepare()?.send()` applies only
//! Concord's HTTP/JSON handling. All tests use loopback fixtures. See the package README.

#![doc = include_str!("../README.md")]

mod error;
mod models;
mod pagination;
mod policy;

pub use concord;
pub use concord_pagination as pagination_engine;
pub use error::{ApiProblem, Error, ErrorDocument, HttpFailure};
pub use models::*;
pub use pagination::CategoryPages;
pub use policy::QueryPolicy;

use concord::{Json, reqwest::header::USER_AGENT};

/// Maximum bytes in each successful query body (1 MiB).
pub const BODY_LIMIT: usize = 1024 * 1024;
/// Maximum bytes inspected in a non-success HTTP body (64 KiB).
pub const ERROR_BODY_LIMIT: usize = 64 * 1024;

concord::api! {
    /// A single wiki's public Action API. The caller supplies its identifying User-Agent.
    pub client MediaWiki(#[sensitive] header(USER_AGENT) user_agent: String) {
        base "https://en.wikipedia.org/w/api.php";
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
        if page.missing || page.invalid || page.pageid != Some(pageid) {
            return Err(Error::ContentUnavailable);
        }
        if page.revisions.len() > 1 {
            return Err(Error::InvalidResponse(Box::new(response.meta)));
        }
        let revision = page
            .revisions
            .into_iter()
            .next()
            .ok_or(Error::ContentUnavailable)?;
        let slot = revision
            .slots
            .and_then(|slots| slots.main)
            .ok_or(Error::ContentUnavailable)?;
        Ok(PageContent {
            pageid,
            title: page.title,
            revision: revision.revid.ok_or(Error::ContentUnavailable)?,
            content: slot.content.ok_or(Error::ContentUnavailable)?,
        })
    }
}
