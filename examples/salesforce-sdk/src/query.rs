//! SOQL query traversal: an ordinary listing ([`QueryPages::into_pages`]) and the page factory
//! for `concord_pagination::sequential`.
//!
//! [`QueryPages::fetch`] turns a locator key into one owned request future: `None` runs the SOQL
//! statement (`GET query?q=`), `Some(locator)` fetches the next batch (`GET query/{locator}`).
//! The projection reads the **whole** envelope before moving the records out: the next key comes
//! from `nextRecordsUrl` (validated against the org, see [`crate::OrgContext::query_link`]), only
//! when `done` is false; `totalSize` is kept as metadata and never used to construct a locator.
//! An envelope that contradicts itself (`done: false` without a link, `done: true` with one) or
//! whose next locator is the one just requested (which would repeat the batch forever) is a typed
//! [`ProtocolError`], not an empty page. Every record's `attributes` must name the queried
//! object and a record URL on the org and version.
//!
//! To read a statement, iterate its pages; the end is explicit (`Ok(None)` only after the batch
//! with `done: true`; a failed batch is a [`PagesError`] carrying the locator to resume from):
//!
//! ```no_run
//! # use concord_example_salesforce_sdk::{OrgContext, model::Account, query::QueryPages, soql::Soql};
//! # use concord_pagination::Resume;
//! # async fn run(ctx: &OrgContext, soql: Soql) -> Result<(), Box<dyn std::error::Error>> {
//! let mut pages = QueryPages::<Account>::new(ctx, soql).into_pages(Resume::Start);
//! while let Some(page) = pages.next_page().await? {
//!     println!("{} records, totalSize {}", page.items.len(), page.meta.total_size);
//! }
//! # Ok(()) }
//! ```
//!
//! For a page budget, a resume report or a custom traversal, give the factory to the engine:
//!
//! ```no_run
//! # use std::num::NonZeroUsize;
//! # use concord_example_salesforce_sdk::{OrgContext, model::Account, query::QueryPages, soql::Soql};
//! # use concord_pagination::{sequential, Budget, Resume};
//! # async fn run(ctx: &OrgContext, soql: Soql) {
//! let pages = QueryPages::<Account>::new(ctx, soql);
//! let budget = Budget::new(NonZeroUsize::new(20).unwrap());
//! let mut chain = sequential(Resume::Start, budget, |key| pages.fetch(key));
//! while let Some(page) = chain.next().await {
//!     let page = page.unwrap();
//!     println!("{} records, totalSize {}", page.items.len(), page.meta.total_size);
//! }
//! let report = chain.finish(); // outcome, counters, where to resume
//! # let _ = report; }
//! ```
//!
//! [`PagesError`]: concord_pagination::PagesError

use std::future::Future;
use std::marker::PhantomData;

use concord::Prepared;
use concord_pagination::{Page, Pages, Resume};
use serde::de::DeserializeOwned;

use crate::SalesforceApiQuery;
use crate::context::OrgContext;
use crate::error::{ProtocolError, ProtocolErrorKind, SfError};
use crate::ids::{BatchSize, QueryLocator, RecordId};
use crate::model::{Account, Attributes, Contact, DynamicRecord, QueryResponse};
use crate::soql::Soql;
use crate::transport::{ApiUsage, prepared, send_with_usage};

/// A record type with declared query endpoints.
pub trait QueryRecord: DeserializeOwned + 'static {
    /// The first batch of `soql` (`GET query?q=`).
    ///
    /// # Errors
    ///
    /// A Concord build error.
    fn first(
        query: &SalesforceApiQuery,
        soql: Soql,
    ) -> Result<Prepared<QueryResponse<Self>>, concord::Error>;

    /// The batch at `locator` (`GET query/{locator}`).
    ///
    /// # Errors
    ///
    /// A Concord build error.
    fn more(
        query: &SalesforceApiQuery,
        locator: QueryLocator,
    ) -> Result<Prepared<QueryResponse<Self>>, concord::Error>;

    /// The record's `attributes`.
    fn attributes(&self) -> &Attributes;

    /// The record's id, when the type models it.
    fn id(&self) -> Option<&RecordId>;
}

impl QueryRecord for Account {
    fn first(
        q: &SalesforceApiQuery,
        soql: Soql,
    ) -> Result<Prepared<QueryResponse<Self>>, concord::Error> {
        q.accounts(soql).prepare()
    }
    fn more(
        q: &SalesforceApiQuery,
        l: QueryLocator,
    ) -> Result<Prepared<QueryResponse<Self>>, concord::Error> {
        q.more_accounts(l).prepare()
    }
    fn attributes(&self) -> &Attributes {
        &self.attributes
    }
    fn id(&self) -> Option<&RecordId> {
        Some(&self.id)
    }
}

impl QueryRecord for Contact {
    fn first(
        q: &SalesforceApiQuery,
        soql: Soql,
    ) -> Result<Prepared<QueryResponse<Self>>, concord::Error> {
        q.contacts(soql).prepare()
    }
    fn more(
        q: &SalesforceApiQuery,
        l: QueryLocator,
    ) -> Result<Prepared<QueryResponse<Self>>, concord::Error> {
        q.more_contacts(l).prepare()
    }
    fn attributes(&self) -> &Attributes {
        &self.attributes
    }
    fn id(&self) -> Option<&RecordId> {
        Some(&self.id)
    }
}

impl QueryRecord for DynamicRecord {
    fn first(
        q: &SalesforceApiQuery,
        soql: Soql,
    ) -> Result<Prepared<QueryResponse<Self>>, concord::Error> {
        q.dynamic(soql).prepare()
    }
    fn more(
        q: &SalesforceApiQuery,
        l: QueryLocator,
    ) -> Result<Prepared<QueryResponse<Self>>, concord::Error> {
        q.more_dynamic(l).prepare()
    }
    fn attributes(&self) -> &Attributes {
        &self.attributes
    }
    fn id(&self) -> Option<&RecordId> {
        None
    }
}

/// Response metadata kept with each page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QueryPageMeta {
    /// `totalSize` (metadata only).
    pub total_size: u64,
    /// `done`.
    pub done: bool,
    /// `Sforce-Limit-Info` `api-usage`, when returned.
    pub api_usage: Option<ApiUsage>,
}

/// One query batch as a pagination page.
pub type QueryPage<R> = Page<R, QueryLocator, QueryPageMeta>;

/// The page factory of one SOQL statement in one org. Each [`fetch`](Self::fetch) builds a
/// fresh declared operation from the frozen context and statement.
pub struct QueryPages<R> {
    ctx: OrgContext,
    soql: Soql,
    batch: Option<BatchSize>,
    _records: PhantomData<fn() -> R>,
}

impl<R: QueryRecord> QueryPages<R> {
    /// The pages of `soql` in `ctx`'s org.
    pub fn new(ctx: &OrgContext, soql: Soql) -> Self {
        Self {
            ctx: ctx.clone(),
            soql,
            batch: None,
            _records: PhantomData,
        }
    }

    /// Requests `batchSize` (`Sforce-Query-Options`); the server may return fewer.
    #[must_use]
    pub fn batch_size(mut self, batch: BatchSize) -> Self {
        self.batch = Some(batch);
        self
    }

    /// The statement.
    pub fn soql(&self) -> &Soql {
        &self.soql
    }

    /// The context.
    pub fn context(&self) -> &OrgContext {
        &self.ctx
    }

    /// The page at `key`: the first batch for `None`, the batch at the locator otherwise. The
    /// future owns everything it needs (no borrow of `self`: it captures no lifetime).
    pub fn fetch(
        &self,
        key: Option<QueryLocator>,
    ) -> impl Future<Output = Result<QueryPage<R>, SfError>> + 'static + use<R> {
        let ctx = self.ctx.clone();
        let soql = self.soql.clone();
        let batch = self.batch;
        async move {
            let scope = ctx.api().query().maybe_batch_size(batch);
            let prepared = prepared(match &key {
                None => R::first(&scope, soql.clone()),
                Some(locator) => R::more(&scope, locator.clone()),
            })?;
            let (response, api_usage) = send_with_usage(prepared).await?;
            project(&ctx, &soql, key.as_ref(), response, api_usage)
        }
    }
}

impl<R: QueryRecord + Send> QueryPages<R> {
    /// The statement's pages from `from` ([`Resume::Start`], or a resume point a previous
    /// traversal returned) as an ordinary listing: [`Pages::next_page`] is `Ok(None)` only after
    /// the batch with `done: true`; a failed batch is a [`concord_pagination::PagesError`]
    /// carrying the [`SfError`] and the locator to retry. No page cap unless you set one
    /// ([`Pages::max_pages`]). Each page is one [`fetch`](Self::fetch). Add `concord-pagination` as a direct dependency and use `concord_pagination::Resume`. `Resume::Start` repeats the SOQL statement's first batch with its configured batch size; `Resume::At(key)` takes the `QueryLocator` validated from `nextRecordsUrl` in a returned page's `next`.
    pub fn into_pages(
        self,
        from: Resume<QueryLocator>,
    ) -> Pages<R, QueryLocator, QueryPageMeta, SfError> {
        Pages::new(from, move |key| self.fetch(key))
    }
}

const OPERATION: &str = "query";

fn project<R: QueryRecord>(
    ctx: &OrgContext,
    soql: &Soql,
    requested: Option<&QueryLocator>,
    response: QueryResponse<R>,
    api_usage: Option<ApiUsage>,
) -> Result<QueryPage<R>, SfError> {
    let next = match (response.done, response.next_records_url.as_deref()) {
        (true, None) => None,
        (true, Some(_)) => {
            return Err(ProtocolError::new(OPERATION, ProtocolErrorKind::NextLinkWhenDone).into());
        }
        (false, None) => {
            return Err(ProtocolError::new(OPERATION, ProtocolErrorKind::MissingNextLink).into());
        }
        (false, Some(raw)) => Some(ctx.query_link(raw)?),
    };
    if next.is_some() && next.as_ref() == requested {
        return Err(ProtocolError::new(OPERATION, ProtocolErrorKind::LocatorDidNotAdvance).into());
    }
    for record in &response.records {
        check_record(ctx, soql, record)?;
    }
    Ok(Page {
        items: response.records,
        next,
        meta: QueryPageMeta {
            total_size: response.total_size,
            done: response.done,
            api_usage,
        },
    })
}

/// A record must be of the queried object, and its URL (when present) must designate it on this
/// org and version.
fn check_record<R: QueryRecord>(ctx: &OrgContext, soql: &Soql, record: &R) -> Result<(), SfError> {
    let attributes = record.attributes();
    if !attributes
        .sobject
        .as_str()
        .eq_ignore_ascii_case(soql.object().as_str())
    {
        return Err(ProtocolError::new(OPERATION, ProtocolErrorKind::UnexpectedRecordType).into());
    }
    if let Some(url) = &attributes.url {
        let (object, id) = ctx.record_link(url)?;
        let same_object = object
            .as_str()
            .eq_ignore_ascii_case(attributes.sobject.as_str());
        let same_id = record.id().is_none_or(|own| *own == id);
        if !same_object || !same_id {
            return Err(
                ProtocolError::new(OPERATION, ProtocolErrorKind::RecordLinkMismatch).into(),
            );
        }
    }
    Ok(())
}
