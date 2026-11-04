//! An external caller of the GitHub SDK slice: the two 4.4 GitHub workflows as library functions.
//!
//! - [`audit::activity_audit`] (GH-A): repository lookup (conditional with an `ETag`), the issue
//!   listing with filters as the parent collection, comments / pull-request details and reviews
//!   as child collections, a request log, and a continuation whenever the run did not complete.
//! - [`artifacts::retrieve_artifact`] (GH-B): select a workflow run, inspect its jobs, find an
//!   artifact by name, follow GitHub's `302` and stream the archive into a bounded
//!   [`sink::ArtifactSink`] without forwarding the token.
//!
//! The crate uses only the SDK's public exports, `concord` and `concord-pagination`. Both
//! workflows are serial (GitHub asks clients not to send concurrent requests) and never retry.
//! The binaries `gh_a` and `gh_b` run the same functions against api.github.com when credentials
//! are in the environment, and print `NOT_RUN` otherwise.

pub mod artifacts;
pub mod audit;
pub mod requests;
pub mod sink;

pub use artifacts::{RetrievalInput, RetrievalOutcome, retrieve_artifact};
pub use audit::{ActivityReport, AuditInput, Completion, activity_audit};
