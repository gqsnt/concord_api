//! A bounded GitHub REST SDK slice written with `concord::api!` (4.4 provider integrations).
//!
//! **Scope.** Repository lookup and labels; issues (list with filters, get, comments); pull
//! requests (list, get, reviews, files); GitHub Actions workflows, runs (list with filters, get),
//! jobs, artifacts (repository and per run, get) and the artifact download. It is a coherent
//! slice for two workflows (a repository activity audit and a workflow-artifact retrieval), not a
//! complete GitHub SDK; `PROVIDER.md` lists what is supported and what is deliberately not.
//!
//! **Layers.**
//!
//! - [`api`]: the declaration. Every operation is a generated endpoint with its setters,
//!   `prepare()`, `send()`, `send_raw()` and `into_parts()`.
//! - [`ids`], [`params`], [`models`], [`filters`]: typed identifiers (an issue id is not a pull
//!   request id), validated parameters, typed responses (issue versus pull-request shapes of the
//!   Issues API), filter records.
//! - [`context`]: one configured [`GitHubContext`] (transport, base, token, pinned API version,
//!   `User-Agent`) and per-repository [`RepoContext`]s that hand out the generated handles and,
//!   for undeclared operations, native requests with the same headers.
//! - [`meta`]: request id, rate limit, `ETag` and `Link` kept with typed values; conditional
//!   requests (`304`).
//! - [`link`], [`paging`], [`pages`]: the shared `Link` relation projection and every listing's ordinary
//!   traversal (`repo.issue_pages(op).items()`, `.pages()`: `Ok(None)` only at the end, a cap or
//!   a failed page is an error), whose page factory also serves `concord-pagination`'s engine.
//! - [`download`]: the artifact download (`302` at the native boundary, streamed to a sink, no
//!   token forwarding).
//!
//! The SDK performs no retry, no waiting on rate limits and no concurrency: GitHub asks for serial
//! requests, and those policies belong to the application.

pub mod api;
pub mod context;
pub mod download;
pub mod error;
pub mod filters;
pub mod ids;
pub mod link;
pub mod meta;
pub mod models;
pub mod pages;
pub mod paging;
pub mod params;

pub use api::{GitHub, GitHubConfig, X_GITHUB_API_VERSION};
pub use context::{ContextError, GitHubContext, RepoContext, redirect_policy};
pub use error::GitHubError;
pub use ids::RepoRef;
