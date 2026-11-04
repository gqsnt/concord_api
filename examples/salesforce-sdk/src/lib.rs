//! A bounded Salesforce REST + Bulk API 2.0 SDK slice built on Concord.
//!
//! Not a complete Salesforce SDK: `PROVIDER.md` lists the operations it implements, the pinned
//! API version and its sources, and what it deliberately leaves out.

mod api;
pub mod bulk;
pub mod bulk_query;
mod context;
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
