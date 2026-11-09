//! A bounded Elasticsearch SDK slice built on Concord: NDJSON bulk requests whose per-item results
//! are checked against the submitted operations, and point-in-time `search_after` traversal.
//!
//! - [`Elasticsearch::bulk`] sends a [`BulkRequest`] and returns a [`BulkReport`] only when
//!   **every** operation was applied. A response with a failed item is
//!   [`Error::PartialBulk`] (so `?` never hides it), carrying the full report;
//!   [`BulkReport::resubmit`] builds a request of failed operations only. A request whose
//!   outcome cannot be known is [`Error::is_uncertain`].
//! - [`Elasticsearch::open_point_in_time`], [`Elasticsearch::search_after`] (a bounded
//!   [`concord_pagination::Pages`] chain) and [`Elasticsearch::close_point_in_time`], or the
//!   whole workflow with [`Elasticsearch::scan`], which closes the PIT whether the traversal
//!   succeeded or not.
//! - [`Elasticsearch::get_document`] returns a document and its [`DocumentVersion`], or typed
//!   absence. [`Elasticsearch::conditional_index`] requires that version and reports a stale
//!   write as [`Error::VersionConflict`]. An uncertain write is never retried by this SDK.
//!
//! The wire declaration is [`ElasticsearchApi`]; its generated `send()` returns this crate's
//! [`Error`] on failure through [`SendPolicy`]. See the crate README for the contract, failure semantics
//! and what is excluded.

pub mod bulk;
pub mod document;
pub mod error;
pub mod model;
pub mod pit;

use std::num::NonZeroUsize;

use concord::reqwest::{self, header::AUTHORIZATION, header::HeaderValue};
use concord::{Endpoint, Json, SendWith, jsonl::Ndjson};
use concord_pagination::{Incomplete, Pages, Resume};

pub use bulk::{
    Action, BulkItem, BulkLimits, BulkLine, BulkReport, BulkRequest, BulkResponse, ItemOutcome,
    ItemResult,
};
pub use concord;
pub use concord_pagination;
pub use document::{
    Document, DocumentSource, DocumentVersion, GetDocument, IndexResponse, IndexResult,
};
pub use error::{Error, ErrorCause, ErrorDocument, ErrorField, Invalid, Protocol, ProviderError};
pub use model::{ApiKey, DocumentId, IndexName, KeepAlive, Refresh};
pub use pit::{
    ClosePitBody, ClosePitResponse, Hit, OpenPitResponse, PitId, PitSearch, PointInTime,
    SearchBody, SearchResponse, Sort, SortValues,
};

/// The bound on every successful response body (bulk, search, PIT and document answers).
pub const RESPONSE_LIMIT: usize = 16 * 1024 * 1024;

/// The bound on an error document read from a non-2xx answer.
pub const ERROR_DOCUMENT_LIMIT: usize = 64 * 1024;

concord::api! {
    /// The Elasticsearch REST operations of this slice. Every generated `send()` returns
    /// `Result<T, Error>` through [`SendPolicy`]; [`Elasticsearch`] adds response correlation and
    /// the point-in-time workflow.
    ///
    /// The declared base is the bulk reference's `localhost:9200`; a deployment's URL is given
    /// to [`Elasticsearch::new`] (or `with_base_url`).
    pub client ElasticsearchApi(#[sensitive] header(AUTHORIZATION) authorization: HeaderValue) {
        base "http://localhost:9200";
        send SendPolicy -> Result<_, Error>;
        format Json;
        limit RESPONSE_LIMIT;

        /// `POST /_bulk`: one NDJSON line per action and per source, every line ended by a line
        /// feed (`application/x-ndjson`).
        POST bulk(body lines: Vec<BulkLine>, query refresh?: Refresh) -> BulkResponse {
            path "_bulk";
            request Ndjson;
        }

        /// One index.
        scope index(name: IndexName) {
            path "{name}";

            /// `GET /<index>/_doc/<id>`: a document and its concurrency token, or typed absence.
            GET get_document(id: DocumentId) -> GetDocument { path "_doc/{id}"; }

            /// `PUT /<index>/_doc/<id>` with both required optimistic concurrency parameters.
            PUT conditional_index(id: DocumentId, body document: DocumentSource,
                query(..) version: DocumentVersion) -> IndexResponse { path "_doc/{id}"; }

            /// `POST /<index>/_pit?keep_alive=<n>m`: open a point in time.
            POST open_point_in_time(query keep_alive: KeepAlive) -> OpenPitResponse {
                path "_pit";
            }
        }

        /// `GET /_search` with a `pit` in the body and no index in the path.
        GET search(body request: SearchBody) -> SearchResponse { path "_search"; }

        /// `DELETE /_pit` with `{"id": ..}`: close a point in time.
        DELETE close_point_in_time(body request: ClosePitBody) -> ClosePitResponse {
            path "_pit";
        }
    }
}

/// The send policy of [`ElasticsearchApi`]: ordinarily a non-2xx answer is [`Error::Provider`] with its
/// error document read with a [`ERROR_DOCUMENT_LIMIT`] bound (the status stays authoritative when
/// the document is absent, not JSON, oversized or malformed); a 2xx is decoded as declared.
/// Document reads also accept a `404` with `found: false`, and conditional indexing maps
/// `409` to [`Error::VersionConflict`]. These exceptions never apply to bulk or PIT endpoints.
#[derive(Debug, Default)]
pub struct SendPolicy;

impl<T: ResponsePolicy> SendWith<T> for SendPolicy {
    type Output = Result<T, Error>;

    async fn send<E: Endpoint<T> + Send>(&self, endpoint: E) -> Self::Output {
        let received = endpoint.prepare()?.receive().await?;
        T::read(received).await
    }
}

// Concord's send policy is client-wide. Dispatch by the declared output type so provider
// exceptions are local to the two document endpoints, without matching endpoint-name strings.
/// Internal response dispatch for the client-wide [`SendPolicy`].
#[doc(hidden)]
pub trait ResponsePolicy: Sized + Send {
    /// Consumes one response without replaying its request.
    fn read(
        received: concord::Received<Self>,
    ) -> impl std::future::Future<Output = Result<Self, Error>> + Send {
        standard_response(received)
    }
}

impl ResponsePolicy for BulkResponse {}
impl ResponsePolicy for OpenPitResponse {}
impl ResponsePolicy for SearchResponse {}
impl ResponsePolicy for ClosePitResponse {}

pub(crate) async fn provider_error<T>(received: concord::Received<T>) -> Box<ProviderError> {
    let (meta, document) = received
        .document::<Json, ErrorDocument>(ERROR_DOCUMENT_LIMIT)
        .await;
    Box::new(ProviderError::new(meta, document))
}

pub(crate) async fn standard_response<T>(received: concord::Received<T>) -> Result<T, Error> {
    if !received.status().is_success() {
        return Err(Error::Provider(provider_error(received).await));
    }
    Ok(received.decode().await?)
}

/// The SDK client: the generated [`ElasticsearchApi`] plus document, bulk and point-in-time
/// workflows. Cheap to clone. `Debug` never prints the API key.
///
/// Transport settings belong to the `reqwest::Client` you pass in. For all writes, build it
/// without automatic retries (`reqwest::retry::never()`) and without following redirects: a
/// transport that replays a write on its own can hide an uncertain outcome behind a conflict.
#[derive(Clone, Debug)]
pub struct Elasticsearch {
    api: ElasticsearchApi,
}

impl Elasticsearch {
    /// A client for the deployment at `base_url` (scheme, host, port and any path prefix).
    pub fn new(http: reqwest::Client, base_url: &str, api_key: ApiKey) -> Result<Self, Error> {
        let api = ElasticsearchApi::new(
            http,
            ElasticsearchApiConfig {
                authorization: api_key.into_header(),
            },
        )
        .with_base_url(base_url)?;
        Ok(Self { api })
    }

    /// The generated client, for direct use of the declared operations.
    pub fn api(&self) -> &ElasticsearchApi {
        &self.api
    }

    /// Reads one document, including its sequence number and primary term.
    /// A missing document is [`GetDocument::NotFound`]; a missing index remains a provider error.
    pub async fn get_document(&self, index: &str, id: &str) -> Result<GetDocument, Error> {
        let response = self
            .api
            .index(IndexName::new(index)?)
            .get_document(DocumentId::new(id)?)
            .send()
            .await?;
        let returned_id = match &response {
            GetDocument::Found(document) => document.id(),
            GetDocument::NotFound { id, .. } => id,
        };
        if returned_id != id {
            return Err(Error::Protocol(Protocol::DocumentId));
        }
        Ok(response)
    }

    /// Replaces a document only if `version` still identifies its last modification.
    /// Pass the version returned by [`get_document`](Self::get_document). `source` is the
    /// complete replacement, not a partial update. A stale version is [`Error::VersionConflict`].
    /// No error is automatically retried; if [`Error::is_uncertain`] is true (including a
    /// timeout), reconcile with a fresh read before deciding what to do.
    pub async fn conditional_index<T: serde::Serialize + ?Sized>(
        &self,
        index: &str,
        id: &str,
        version: DocumentVersion,
        source: &T,
    ) -> Result<IndexResponse, Error> {
        let response = self
            .api
            .index(IndexName::new(index)?)
            .conditional_index(DocumentId::new(id)?, DocumentSource::new(source)?, version)
            .send()
            .await?;
        if response.id != id {
            return Err(Error::Protocol(Protocol::DocumentId));
        }
        Ok(response)
    }

    /// Sends a bulk request and checks every item against its operation.
    ///
    /// - `Ok(report)`: every operation was applied.
    /// - [`Error::PartialBulk`]: at least one operation failed; the report has every outcome.
    /// - [`Error::Protocol`]: the response cannot be matched with the request (item count,
    ///   action or id), or its `errors` flag contradicts its items; uncertain.
    /// - Any other error: see [`Error::is_uncertain`] before doing anything with the
    ///   operations.
    pub async fn bulk(&self, request: BulkRequest) -> Result<BulkReport, Error> {
        if request.is_empty() {
            return Err(Error::Invalid(Invalid::EmptyBulk));
        }
        let response = self
            .api
            .bulk(request.lines())
            .maybe_refresh(request.refresh())
            .send()
            .await?;
        bulk::correlate(request, response)
    }

    /// Opens a point in time on `index`.
    pub async fn open_point_in_time(
        &self,
        index: &str,
        keep_alive: KeepAlive,
    ) -> Result<PointInTime, Error> {
        let index = IndexName::new(index)?;
        let opened = self
            .api
            .index(index.clone())
            .open_point_in_time(keep_alive)
            .send()
            .await?;
        if opened.id.as_str().is_empty() {
            return Err(Error::Protocol(Protocol::EmptyPitId));
        }
        Ok(PointInTime::new(index, keep_alive, opened.id))
    }

    /// The `search_after` traversal over `pit`: at most `max_pages` page requests, each naming
    /// the PIT's latest id and the previous page's last sort values. The traversal ends
    /// (`Ok(None)`) at the first page without hits; reaching the cap first is
    /// `PagesError::Bounded`. Items can be consumed as a stream (`.items().into_stream()`).
    ///
    /// It does not close the PIT: call [`close_point_in_time`](Self::close_point_in_time).
    pub fn search_after(
        &self,
        pit: &PointInTime,
        search: &PitSearch,
        max_pages: NonZeroUsize,
    ) -> Pages<Hit, SortValues, (), Error> {
        let api = self.api.clone();
        let pit = pit.share();
        let search = search.clone();
        Pages::new(Resume::Start, move |after: Option<SortValues>| {
            let api = api.clone();
            let pit = pit.share();
            let search = search.clone();
            async move {
                let body = search.body(&pit, after.clone());
                let response = api.search(body).send().await?;
                pit::page(response, &search, &pit, after.as_ref())
            }
        })
        .max_pages(max_pages)
    }

    /// Closes `pit` by its latest id.
    pub async fn close_point_in_time(&self, pit: PointInTime) -> Result<ClosePitResponse, Error> {
        self.api
            .close_point_in_time(ClosePitBody::new(pit.id()))
            .send()
            .await
    }

    /// The whole point-in-time workflow: open a PIT on `index`, read at most `max_pages` pages
    /// with `search_after`, and close the PIT — also when the traversal failed or reached its
    /// cap. At most `max_pages × page_size` hits are held.
    pub async fn scan(
        &self,
        index: &str,
        keep_alive: KeepAlive,
        search: &PitSearch,
        max_pages: NonZeroUsize,
    ) -> Result<Scan, ScanError> {
        let pit = match self.open_point_in_time(index, keep_alive).await {
            Ok(pit) => pit,
            Err(error) => {
                return Err(ScanError {
                    error,
                    collected: Vec::new(),
                    close: None,
                });
            }
        };
        let traversal = self
            .search_after(&pit, search, max_pages)
            .items()
            .collect_complete()
            .await;
        let close = self.close_point_in_time(pit).await;
        match traversal {
            Ok(hits) => Ok(Scan { hits, close }),
            Err(incomplete) => {
                let Incomplete {
                    collected, error, ..
                } = incomplete;
                let error = match error.into_provider_error() {
                    Ok((error, _, _)) => error,
                    Err(other) => Error::PageCap {
                        pages: other.progress().succeeded,
                    },
                };
                Err(ScanError {
                    error,
                    collected,
                    close: Some(close),
                })
            }
        }
    }
}

/// A completed [`Elasticsearch::scan`]: every hit, and how closing the PIT went.
#[derive(Debug)]
#[non_exhaustive]
pub struct Scan {
    /// Every hit, in sort order.
    pub hits: Vec<Hit>,
    /// The close-PIT outcome (a failure here does not make the hits incomplete).
    pub close: Result<ClosePitResponse, Error>,
}

/// A failed [`Elasticsearch::scan`]: why, the hits read before, and how closing the PIT went.
#[derive(Debug)]
#[non_exhaustive]
pub struct ScanError {
    /// The first failure (opening, a page, or [`Error::PageCap`]).
    pub error: Error,
    /// The hits of the pages read before the failure: a prefix, never the whole result.
    pub collected: Vec<Hit>,
    /// The close-PIT outcome; `None` when no PIT was opened.
    pub close: Option<Result<ClosePitResponse, Error>>,
}

impl std::fmt::Display for ScanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "point-in-time scan failed after {} hits: {}",
            self.collected.len(),
            self.error
        )
    }
}

impl std::error::Error for ScanError {}
