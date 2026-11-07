//! A bounded, public-data Europe PMC SDK built on Concord's generated send path.
//!
//! Local fixtures exercise JSON search → selected publication → citations → core citation,
//! the same search in XML with opaque cursor continuation, and a selected publication's
//! reference list paged by page number in either format. No live calls are made in tests.
//! See the package README and PROVENANCE for provider sources and limits.

mod error;
mod input;
mod model;
mod pagination;
mod xml;

/// Concord types used in the public API and by generated operations.
pub use concord;
/// Shared traversal errors, resume positions and reports.
pub use concord_pagination;
/// Shared Retry-After error type and parser used by HTTP failures.
pub use concord_retry;
pub use error::{Error, HttpError};
pub use input::{Cursor, PageNumber, PageSize, PublicationId, Query, RecordKey, Source};
pub use model::{
    CitationList, CitationsPage, Document, Publication, Reference, ReferenceList, ReferencesPage,
    ResultList, SearchPage,
};
pub use pagination::{ReferencePages, SearchFormat, SearchPages};
pub use xml::{ReferencesXml, SearchXml};

use concord::{Endpoint, Json, SendWith, reqwest};
use std::time::Duration;

/// Maximum buffered success document size (XML and JSON).
pub const BODY_LIMIT: usize = 256 * 1024;
/// Maximum buffered non-2xx diagnostic body size.
pub const ERROR_BODY_LIMIT: usize = 16 * 1024;
/// Maximum pages in an SDK traversal. Resumption requires the same query and page size.
pub const MAX_PAGES: usize = 20;

/// One execution, bounded diagnostics and decoded-envelope validation; no retry or signing.
#[derive(Default)]
pub struct SendPolicy;

impl<T: Document + Send> SendWith<T> for SendPolicy {
    type Output = Result<T, Error>;

    async fn send<E: Endpoint<T> + Send>(&self, endpoint: E) -> Self::Output {
        let prepared = endpoint.prepare()?;
        let requested = requested_page_size(prepared.request()).unwrap_or(100);
        let received = prepared.receive().await?;
        if !received.status().is_success() {
            let meta = received.meta();
            let body = received.bytes_limited(ERROR_BODY_LIMIT).await;
            return Err(Error::Http(Box::new(HttpError { meta, body })));
        }
        let value = received.decode().await?;
        value.validate(requested)?;
        Ok(value)
    }
}

/// The `pageSize` a prepared request carries. Generated handles expose no value getters, so
/// the send policy and the references traversal read the declared value back from the URL.
pub(crate) fn requested_page_size(request: &reqwest::Request) -> Option<usize> {
    request
        .url()
        .query_pairs()
        .find(|(key, _)| key == "pageSize")
        .and_then(|(_, value)| value.parse::<usize>().ok())
}

concord::api! {
    /// Public Europe PMC REST client. Queries and opaque cursors are withheld in generated Debug.
    pub client EuropePmc {
        base "https://www.ebi.ac.uk/europepmc/webservices/rest";
        limit BODY_LIMIT;
        send SendPolicy -> Result<_, Error>;

        /// One immutable bounded search plan, shared by JSON, XML and core metadata lookups.
        scope search(#[sensitive] query("query") query: Query, query("pageSize") page_size: PageSize) {
            path "search";

            /// Lite JSON search; explicitly requests JSON rather than accepting the XML default.
            GET json(#[sensitive] query("cursorMark") cursor: Cursor = Cursor::start()) -> SearchPage {
                query { "format" = "json"; "resultType" = "lite"; }
                response Json;
            }
            /// The same lite search in XML, through an SDK Decode implementation.
            GET xml(#[sensitive] query("cursorMark") cursor: Cursor = Cursor::start()) -> SearchPage {
                query { "format" = "xml"; "resultType" = "lite"; }
                response SearchXml;
            }
            /// Core JSON metadata. Prefer fetch_citation for an exact selected identity.
            GET core() -> SearchPage {
                query { "format" = "json"; "resultType" = "core"; }
                response Json;
            }
        }

        /// A publication identified by both provider source and external identifier.
        scope publication(source: Source, id: PublicationId) {
            path "{source}/{id}";

            /// First page of publications citing this record; pagination beyond it is excluded.
            GET citations(query("pageSize") page_size: PageSize = PageSize::default()) -> CitationsPage {
                path "citations";
                query { "format" = "json"; "page" = "1"; }
                response Json;
            }

            /// This publication's reference list (`references`) at one page size, shared by every
            /// page. Read it whole with `pages`; a matched [`Reference`] has a `key()`.
            scope references(query("pageSize") page_size: PageSize) {
                path "references";

                /// One page with `format=json` pinned, never the provider's XML default.
                GET json(query("page") page: PageNumber = PageNumber::FIRST) -> ReferencesPage {
                    query { "format" = "json"; }
                    response Json;
                }
                /// The same page with `format=xml` pinned, decoded by the SDK XML format.
                GET xml(query("page") page: PageNumber = PageNumber::FIRST) -> ReferencesPage {
                    query { "format" = "xml"; }
                    response ReferencesXml;
                }
            }
        }
    }
}

/// A transport with a ten-second total request timeout, no redirects, Referer or native retries.
/// Callers may supply their own client, in which case its retry/redirect policy also applies.
pub fn http_client() -> reqwest::Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .referer(false)
        .retry(reqwest::retry::never())
        .no_proxy()
        .build()
}

impl EuropePmc {
    /// Fetches core citation metadata for exactly one selected identity via the search module.
    /// The provider has no separate single-record route in this declared slice.
    /// An empty response is NotFound; ambiguity or an identity mismatch is UnexpectedRecord.
    pub async fn fetch_citation(&self, key: &RecordKey) -> Result<Publication, Error> {
        let response = self
            .search(key.query(), PageSize::new(1)?)
            .core()
            .send()
            .await?;
        let mut records = response.result_list.result.into_iter();
        let Some(record) = records.next() else {
            return Err(Error::NotFound);
        };
        if record.key() != *key || records.next().is_some() || response.hit_count != 1 {
            return Err(Error::UnexpectedRecord);
        }
        Ok(record)
    }
}

/// Compiles the package README's public calling example.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
pub struct ReadmeDoctests;
