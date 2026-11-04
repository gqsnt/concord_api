//! Link-header pagination: a page factory for `concord-pagination`'s sequential traversal.
//!
//! GitHub's contract ([pagination guide]): a paginated response carries `Link` with
//! `rel="next"` until the last page; clients follow those URLs rather than building page
//! queries; `rel="last"` is omitted when it cannot be calculated. The URL is opaque here: some
//! endpoints page with `page`, others with `before`/`after` cursors, and GitHub may address the
//! collection by repository id (`/repositories/1300192/issues?page=2`) rather than by name.
//!
//! [`LinkPages`] turns one generated list operation (filters already set) into a fetch function
//! `Option<NextLink> -> Page`:
//!
//! - **Key.** `None` fetches the first page: the operation as configured. `Some(link)` fetches the
//!   page a previous response named. The next request cannot be built before its key exists,
//!   and no page number is ever computed (not from `last`, not from a count).
//! - **Same endpoint, provider URL.** A linked page is the same endpoint (same headers, same
//!   decoder, same Concord endpoint name in errors) sent to the URL GitHub gave: the operation is
//!   prepared, then its native URL is replaced by the link (`Prepared::into_parts`). The link must
//!   be on the configured API origin and under its base path ([`GitHubContext::is_api_url`]);
//!   otherwise nothing is sent ([`GitHubError::ForeignLink`]), so the token never leaves the API.
//! - **Binding.** A [`NextLink`] remembers the operation, repository and parent resource it came
//!   from ([`Binding`]); using it with another [`LinkPages`] is a
//!   [`GitHubError::ContinuationMismatch`]. The link's own query carries GitHub's filters, so a
//!   resumed traversal continues the original listing.
//! - **Continuation before projection.** The `Link` header is read from the full response before
//!   the envelope is projected into items ([`Listing`]). A malformed header is a
//!   [`GitHubError::Link`], never "last page"; an envelope inconsistent with itself is a
//!   [`GitHubError::Envelope`].
//! - **Metadata.** Every page keeps its [`ResponseMeta`] (request id, rate limit, links) in
//!   [`PageInfo`].
//!
//! GitHub listings are live: items created, closed or relabelled between two requests can shift
//! between pages, so a traversal is a best-effort enumeration, not a snapshot.
//!
//! [pagination guide]: https://docs.github.com/en/rest/using-the-rest-api/using-pagination-in-the-rest-api

use std::fmt;
use std::future::Future;

use concord::Prepared;
use concord::reqwest::Url;

use crate::context::{GitHubContext, RepoContext};
use crate::error::{EnvelopeProblem, GitHubError};
use crate::ids::RepoRef;
use crate::meta::{ResponseMeta, WithMeta, send_edited};
use crate::models::{
    Artifact, ArtifactList, Job, JobList, Workflow, WorkflowList, WorkflowRun, WorkflowRunList,
};

/// GitHub operation ids, used as the operation part of a [`Binding`].
pub mod operation {
    /// `issues/list-for-repo`
    pub const ISSUES: &str = "issues/list-for-repo";
    /// `issues/list-comments`
    pub const ISSUE_COMMENTS: &str = "issues/list-comments";
    /// `issues/list-comments-for-repo`
    pub const REPO_COMMENTS: &str = "issues/list-comments-for-repo";
    /// `issues/list-labels-for-repo`
    pub const LABELS: &str = "issues/list-labels-for-repo";
    /// `pulls/list`
    pub const PULLS: &str = "pulls/list";
    /// `pulls/list-reviews`
    pub const PULL_REVIEWS: &str = "pulls/list-reviews";
    /// `pulls/list-files`
    pub const PULL_FILES: &str = "pulls/list-files";
    /// `actions/list-repo-workflows`
    pub const WORKFLOWS: &str = "actions/list-repo-workflows";
    /// `actions/list-workflow-runs-for-repo`
    pub const RUNS: &str = "actions/list-workflow-runs-for-repo";
    /// `actions/list-workflow-runs`
    pub const WORKFLOW_RUNS: &str = "actions/list-workflow-runs";
    /// `actions/list-jobs-for-workflow-run`
    pub const RUN_JOBS: &str = "actions/list-jobs-for-workflow-run";
    /// `actions/list-artifacts-for-repo`
    pub const ARTIFACTS: &str = "actions/list-artifacts-for-repo";
    /// `actions/list-workflow-run-artifacts`
    pub const RUN_ARTIFACTS: &str = "actions/list-workflow-run-artifacts";
}

/// What a continuation belongs to: the listing operation, the repository and, for a child
/// collection, the parent's number or id (issue number, pull number, run id).
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Binding {
    operation: &'static str,
    repo: RepoRef,
    parent: Option<u64>,
}

impl Binding {
    /// A binding for `operation` (see [`operation`]) of `repo`, under `parent`.
    pub fn new(operation: &'static str, repo: RepoRef, parent: Option<u64>) -> Self {
        Self {
            operation,
            repo,
            parent,
        }
    }

    /// The operation id.
    pub fn operation(&self) -> &'static str {
        self.operation
    }

    /// The repository.
    pub fn repo(&self) -> &RepoRef {
        &self.repo
    }

    /// The parent resource.
    pub fn parent(&self) -> Option<u64> {
        self.parent
    }
}

impl fmt::Debug for Binding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Binding")
            .field("operation", &self.operation)
            .field("repo", &format_args!("{}", self.repo))
            .field("parent", &self.parent)
            .finish()
    }
}

/// A page GitHub linked to (`rel="next"`): the key of the next fetch and the restart point of an
/// interrupted traversal. `Debug` withholds the URL.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct NextLink {
    url: Url,
    binding: Binding,
}

impl NextLink {
    /// What it belongs to.
    pub fn binding(&self) -> &Binding {
        &self.binding
    }

    /// The URL, **unredacted** (for an application that persists a continuation; restore it with
    /// [`LinkPages::restore`]).
    pub fn raw_url(&self) -> &Url {
        &self.url
    }
}

impl fmt::Debug for NextLink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NextLink")
            .field("binding", &self.binding)
            .field("url", &format_args!("<withheld>"))
            .finish()
    }
}

/// A list response: either a bare JSON array or an envelope `{ total_count, <items> }`. The
/// projection moves the items out of the envelope (no `Clone` needed).
pub trait Listing {
    /// The item type.
    type Item;
    /// `total_count` of an envelope (`None` for a bare array).
    fn total_count(&self) -> Option<u64>;
    /// The page's items, moved out.
    fn into_items(self) -> Vec<Self::Item>;
}

impl<T> Listing for Vec<T> {
    type Item = T;
    fn total_count(&self) -> Option<u64> {
        None
    }
    fn into_items(self) -> Vec<T> {
        self
    }
}

macro_rules! envelope {
    ($list:ty, $item:ty, $field:ident) => {
        impl Listing for $list {
            type Item = $item;
            fn total_count(&self) -> Option<u64> {
                Some(self.total_count)
            }
            fn into_items(self) -> Vec<$item> {
                self.$field
            }
        }
    };
}

envelope!(WorkflowRunList, WorkflowRun, workflow_runs);
envelope!(JobList, Job, jobs);
envelope!(ArtifactList, Artifact, artifacts);
envelope!(WorkflowList, Workflow, workflows);

/// What a page reports besides its items.
#[derive(Clone, Debug)]
pub struct PageInfo {
    /// The response metadata (request id, rate limit, links).
    pub meta: ResponseMeta,
    /// `total_count` of an envelope page.
    pub total_count: Option<u64>,
}

/// One fetched page.
pub struct LinkPage<T> {
    /// The items, in GitHub's order.
    pub items: Vec<T>,
    /// The next page, `None` on the last page.
    pub next: Option<NextLink>,
    /// The page's metadata.
    pub info: PageInfo,
}

impl<T> fmt::Debug for LinkPage<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LinkPage")
            .field("items", &self.items.len())
            .field("next", &self.next)
            .field("info", &self.info)
            .finish()
    }
}

/// A page factory for one list operation of one repository. See the
/// [module documentation](self).
pub struct LinkPages<Op, R> {
    context: GitHubContext,
    binding: Binding,
    operation: Op,
    prepare: fn(Op) -> Result<Prepared<R>, concord::Error>,
}

impl<Op: Clone, R: Listing> LinkPages<Op, R> {
    /// Pages of `operation` (a generated list operation with its filters set), prepared with
    /// `prepare` (its generated `prepare` method, for example `GitHubRepoIssues::prepare`).
    pub fn new(
        repo: &RepoContext,
        operation_id: &'static str,
        parent: Option<u64>,
        operation: Op,
        prepare: fn(Op) -> Result<Prepared<R>, concord::Error>,
    ) -> Self {
        Self {
            context: repo.context().clone(),
            binding: Binding::new(operation_id, repo.repo().clone(), parent),
            operation,
            prepare,
        }
    }

    /// What this factory's continuations belong to.
    pub fn binding(&self) -> &Binding {
        &self.binding
    }

    /// A continuation from a URL an application persisted ([`NextLink::raw_url`]).
    ///
    /// # Errors
    ///
    /// [`GitHubError::ForeignLink`] if it is not an API URL of this context.
    pub fn restore(&self, url: Url) -> Result<NextLink, GitHubError> {
        if !self.context.is_api_url(&url) {
            return Err(GitHubError::ForeignLink { meta: None });
        }
        Ok(NextLink {
            url,
            binding: self.binding.clone(),
        })
    }

    /// Fetches the first page (`None`) or a linked page. The returned future owns everything it
    /// needs (the operation is prepared now, without I/O), so the factory can be used from a
    /// `FnMut(Option<NextLink>) -> Future` closure.
    pub fn fetch(
        &self,
        key: Option<NextLink>,
    ) -> impl Future<Output = Result<LinkPage<R::Item>, GitHubError>> + use<Op, R> {
        let prepared = (self.prepare)(self.operation.clone());
        fetch_page(self.context.clone(), self.binding.clone(), prepared, key)
    }
}

impl<Op, R> fmt::Debug for LinkPages<Op, R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LinkPages")
            .field("binding", &self.binding)
            .finish_non_exhaustive()
    }
}

async fn fetch_page<R: Listing>(
    context: GitHubContext,
    binding: Binding,
    prepared: Result<Prepared<R>, concord::Error>,
    key: Option<NextLink>,
) -> Result<LinkPage<R::Item>, GitHubError> {
    let prepared = prepared?;
    if let Some(key) = &key {
        if key.binding != binding {
            return Err(GitHubError::ContinuationMismatch {
                expected: Box::new(binding),
                found: Box::new(key.binding.clone()),
            });
        }
        if !context.is_api_url(&key.url) {
            return Err(GitHubError::ForeignLink { meta: None });
        }
    }
    let WithMeta { value, meta } = send_edited(prepared, |request| {
        if let Some(key) = key {
            *request.url_mut() = key.url;
        }
        Ok(())
    })
    .await?;
    // The continuation comes from the full response, before the items are moved out.
    let next = match meta.links() {
        Err(error) => {
            return Err(GitHubError::Link {
                error,
                meta: Box::new(meta),
            });
        }
        Ok(links) => match links.next() {
            None => None,
            Some(url) if context.is_api_url(url) => Some(NextLink {
                url: url.clone(),
                binding,
            }),
            Some(_) => {
                return Err(GitHubError::ForeignLink {
                    meta: Some(Box::new(meta)),
                });
            }
        },
    };
    let total_count = value.total_count();
    let items = value.into_items();
    if let Some(total) = total_count
        && total < items.len() as u64
    {
        return Err(GitHubError::Envelope {
            problem: EnvelopeProblem::TotalBelowPage {
                total_count: total,
                items: items.len(),
            },
            meta: Box::new(meta),
        });
    }
    Ok(LinkPage {
        items,
        next,
        info: PageInfo { meta, total_count },
    })
}
