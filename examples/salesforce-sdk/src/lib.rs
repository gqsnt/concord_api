//! A bounded Salesforce REST + Bulk API 2.0 SDK slice built on Concord.
//!
//! Not a complete Salesforce SDK: `PROVIDER.md` lists the operations it implements, the pinned
//! API version (68.0) and its sources, the documented discrepancies and how they are resolved,
//! and what it deliberately leaves out.
//!
//! - [`OrgContext`]: one org (instance URL), one API version, one access token; the generated
//!   [`SalesforceApi`] handle ([`OrgContext::api`]) and the validation of links Salesforce returns
//!   ([`context`]).
//! - [`ids`], [`soql`], [`model`], [`bulk`]: typed identifiers, SOQL construction kept apart from
//!   transport encoding, typed REST documents (a small Account/Contact schema with optional and
//!   custom fields, dynamic records, describe), Bulk API 2.0 documents.
//! - [`query`]: the SOQL page factory for `concord_pagination::sequential`.
//! - [`bulk_query`]: query jobs, the sequential `Sforce-Locator` result chain and the parallel
//!   result-pages discovery factory for `concord_pagination::manifest_jobs`.
//! - [`ingest`]: ingest jobs (create, known-length upload, close, poll, result files).
//! - [`csv`]: the provider's CSV dialect on `concord::csv`.
//!
//! The declared operations stay ordinary generated operations: callers keep their setters,
//! `prepare()`, `send()`, `send_raw()` and `into_parts()`.
//!
//! ```no_run
//! use concord_example_salesforce_sdk::ids::{AccessToken, ApiVersion, InstanceUrl};
//! use concord_example_salesforce_sdk::soql::{FieldPath, Soql};
//! use concord_example_salesforce_sdk::ids::SObjectName;
//! use concord_example_salesforce_sdk::{OrgContext, SfError};
//!
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! let ctx = OrgContext::new(
//!     concord::reqwest::Client::new(),
//!     InstanceUrl::parse("https://acme.my.salesforce.com")?,
//!     ApiVersion::PINNED,
//!     AccessToken::new(std::env::var("SF_ACCESS_TOKEN")?)?,
//! )?;
//! let soql = Soql::select(
//!     SObjectName::new("Account")?,
//!     [FieldPath::parse("Id")?, FieldPath::parse("Name")?],
//! )
//! .build()?;
//! // An ordinary generated operation: setters, prepare, send.
//! let page = match ctx.api().query().accounts(soql).send().await {
//!     Ok(page) => page,
//!     Err(e) => return Err(SfError::from_concord(e).await.into()),
//! };
//! println!("{} of {} accounts", page.records.len(), page.total_size);
//! # Ok(()) }
//! ```

mod api;
pub mod bulk;
pub mod bulk_query;
pub mod context;
pub mod csv;
mod error;
pub mod ids;
pub mod ingest;
pub mod model;
pub mod query;
pub mod soql;
mod transport;

pub use api::*;
pub use context::{ContextError, OrgContext, ResultLink};
pub use error::{
    ApiFailure, ErrorBodyState, JobFailure, LinkError, LinkErrorKind, LinkRole, MAX_ERROR_BODY,
    ProtocolError, ProtocolErrorKind, SfError,
};
pub use transport::ApiUsage;
