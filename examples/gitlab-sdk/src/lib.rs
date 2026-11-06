//! A GitLab REST API v4 slice written with `concord::api!`: page through a project's issues, and
//! retrieve one issue with its project.
//!
//! **Scope.** `GET /projects/:id/issues` (with `Link` pagination), `GET
//! /projects/:id/issues/:issue_iid` and `GET /projects/:id`, where `:id` is a project ID or its
//! full path sent as one URL-encoded segment. Not covered: Git operations, merge requests, other
//! resources, other API versions, GraphQL, writes, keyset pagination, OAuth flows. `README.md`
//! lists every limit.
//!
//! **Use.**
//!
//! ```no_run
//! use concord_sdk_gitlab::{Call, GitLab, IssueIid, IssueState, PrivateToken, ProjectRef};
//!
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! let http = concord::reqwest::Client::builder()
//!     .redirect(concord_sdk_gitlab::redirect_policy())
//!     .build()?;
//! let gitlab = GitLab::with_token(http, PrivateToken::new(std::env::var("GITLAB_TOKEN")?)?);
//! let project = gitlab.project(ProjectRef::path("acme/widgets")?);
//!
//! // Workflow A: every open issue, following GitLab's `Link` header.
//! let issues = project.issues().set_state(IssueState::Opened).items().collect_complete().await?;
//!
//! // One request with metadata.
//! let first = project.issue(IssueIid::new(1)?).call().await?;
//!
//! // Workflow B: an issue and its project.
//! let context = gitlab.issue_context(ProjectRef::path("acme/widgets")?, IssueIid::new(1)?).await?;
//! # let _ = (issues, first, context); Ok(()) }
//! ```
//!
//! **Layers.**
//!
//! - [`api`]: the declaration ([`GitLab`], [`GitLab::with_token`]). Every operation is a generated
//!   endpoint with setters, `prepare()` and `send()`.
//! - [`Call`]: one adapter that sends any operation with [`GitLabError`] and [`ResponseMeta`].
//! - [`pages`]: the issue traversal (`project.issues().pages()`), pinned to offset pagination and
//!   driven by `Link: rel="next"` exactly as sent.
//! - [`context`]: workflow B ([`GitLab::issue_context`]).
//! - [`params`], [`models`], [`meta`], [`link`], [`error`]: inputs, responses, metadata, the
//!   `Link` parser and errors.
//!
//! The SDK performs no retry, no rate-limit waiting and no concurrency.

pub mod api;
pub mod call;
pub mod context;
pub mod error;
pub mod link;
pub mod meta;
pub mod models;
pub mod pages;
pub mod params;

use concord::reqwest::{Url, redirect};

pub use api::{GITLAB_COM, GitLab, GitLabConfig, PRIVATE_TOKEN};
pub use call::Call;
pub use context::{ContextError, IssueContext};
pub use error::{ApiError, ErrorBody, ErrorMessage, GitLabError, PaginationProblem};
pub use meta::{Pagination, Response, ResponseMeta};
pub use models::{Issue, IssueStatus, Project};
pub use pages::{IssueItems, IssuePage, IssuePages, NextPage};
pub use params::{
    InputError, IssueIid, IssueOrder, IssueState, Labels, PerPage, PrivateToken, ProjectId,
    ProjectRef, SortOrder,
};

/// The README's examples, compiled as doctests.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
pub struct ReadmeDoctests;

/// A redirect policy for the `reqwest::Client` you give to [`GitLab`]: follow at most 10
/// redirects that stay on the origin (scheme, host, port) of the original request, and stop at
/// the first one that leaves it, returning that `3xx` response (a [`GitLabError::Api`]).
///
/// GitLab redirects `GET` requests that use the old path of a moved project. reqwest's default
/// policy follows redirects to any host and removes only `Authorization`-like headers on the
/// way; it would forward `PRIVATE-TOKEN`. With this policy the token never follows a redirect to
/// another origin. `redirect::Policy::none()` is also safe (every redirect is then an error).
pub fn redirect_policy() -> redirect::Policy {
    redirect::Policy::custom(|attempt| {
        let leaves = attempt
            .previous()
            .first()
            .is_some_and(|origin| !same_origin(origin, attempt.url()));
        if leaves {
            attempt.stop()
        } else if attempt.previous().len() > 10 {
            attempt.error("too many redirects")
        } else {
            attempt.follow()
        }
    })
}

/// Same scheme, host and port (default ports made explicit).
pub(crate) fn same_origin(a: &Url, b: &Url) -> bool {
    a.scheme() == b.scheme()
        && a.host_str() == b.host_str()
        && a.port_or_known_default() == b.port_or_known_default()
}
