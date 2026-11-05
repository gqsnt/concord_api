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
//!   [`GitHubError::ContinuationMismatch`]. A persisted URL restored with [`LinkPages::restore`]
//!   gets its binding only if its path addresses the factory's own collection (the same check,
//!   read from the path: see [`LinkPages::restore`]). The link's own query carries GitHub's
//!   filters, so a resumed traversal continues the original listing.
//! - **Progress.** A `rel="next"` equal to the URL just requested would fetch the same page
//!   forever (only the caller's budget would end the chain): it is a
//!   [`GitHubError::LinkDidNotAdvance`], and the page's items are not returned.
//! - **Continuation before projection.** The `Link` header is read from the full response before
//!   the envelope is projected into items ([`Listing`]). A malformed header is a
//!   [`GitHubError::Link`], never "last page"; an envelope inconsistent with itself is a
//!   [`GitHubError::Envelope`].
//! - **Metadata.** Every page keeps its [`ResponseMeta`] (request id, rate limit, links) in
//!   [`PageInfo`], the page's `meta`.
//!
//! GitHub listings are live: items created, closed or relabelled between two requests can shift
//! between pages, so a traversal is a best-effort enumeration, not a snapshot.
//!
//! [pagination guide]: https://docs.github.com/en/rest/using-the-rest-api/using-pagination-in-the-rest-api

use std::fmt;
use std::future::Future;

use concord::Prepared;
use concord::reqwest::Url;
use concord_pagination::Page;

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
    /// Not a listing: the operation of the binding a restored URL is reported with
    /// ([`GitHubError::ContinuationMismatch`](crate::GitHubError::ContinuationMismatch)'s
    /// `found`) when its path names no collection of the factory's operation.
    pub const UNRECOGNIZED: &str = "unrecognized-collection";
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

/// What a page reports besides its items: the [`Page::meta`] of this SDK's pages.
#[derive(Clone, Debug)]
pub struct PageInfo {
    /// The response metadata (request id, rate limit, links).
    pub response: ResponseMeta,
    /// `total_count` of an envelope page.
    pub total_count: Option<u64>,
}

/// One fetched page: `concord-pagination`'s [`Page`] with a [`NextLink`] key and [`PageInfo`].
pub type LinkPage<T> = Page<T, NextLink, PageInfo>;

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

    /// A continuation from a URL an application persisted ([`NextLink::raw_url`]). Nothing is
    /// sent.
    ///
    /// The URL gets this factory's binding only if its path addresses this factory's collection:
    /// below the base path, a repository part — `repos/{owner}/{name}` naming this factory's
    /// repository (ASCII case-insensitively, as GitHub compares them) or GitHub's
    /// `repositories/{id}` form — followed by exactly the path of the factory's own first
    /// request (for example `issues/11/comments` for issue 11's comments). The numeric id of the
    /// `repositories/{id}` form cannot be checked against the owner and name: such a URL is
    /// accepted for the operation and parent its path names, in whatever repository the id
    /// designates. The query (GitHub's filters, page or cursor) is kept as stored.
    ///
    /// # Errors
    ///
    /// - [`GitHubError::ForeignLink`] if it is not an API URL of this context;
    /// - [`GitHubError::ContinuationMismatch`] if its path addresses another collection.
    ///   `found` is what the path names, as far as it can be read: another parent of the same
    ///   operation, another repository (by name; the `repositories/{id}` form is reported with
    ///   this factory's repository), or [`operation::UNRECOGNIZED`] for any other path;
    /// - the Concord build error if the factory's operation cannot be prepared.
    pub fn restore(&self, url: Url) -> Result<NextLink, GitHubError> {
        if !self.context.is_api_url(&url) {
            return Err(GitHubError::ForeignLink { meta: None });
        }
        let prepared = (self.prepare)(self.operation.clone())?;
        let found = self.binding_of(prepared.request().url(), &url);
        if found != self.binding {
            return Err(GitHubError::ContinuationMismatch {
                expected: Box::new(self.binding.clone()),
                found: Box::new(found),
            });
        }
        Ok(NextLink {
            url,
            binding: self.binding.clone(),
        })
    }

    /// The binding `link`'s path names, read against `own`, the path of this factory's first
    /// request. Equal to `self.binding` exactly when `link` addresses the same collection.
    fn binding_of(&self, own: &Url, link: &Url) -> Binding {
        let unrecognized =
            || Binding::new(operation::UNRECOGNIZED, self.binding.repo.clone(), None);
        let base = self.context.base_url().path().trim_end_matches('/');
        let (Some(own), Some(link)) = (below(base, own), below(base, link)) else {
            return unrecognized();
        };
        let (Some((RepoPart::Name(..), own_rest)), Some((repo, rest))) =
            (repo_part(&own), repo_part(&link))
        else {
            return unrecognized();
        };
        let repo = match repo {
            RepoPart::Name(owner, name)
                if !(owner.eq_ignore_ascii_case(self.binding.repo.owner.as_str())
                    && name.eq_ignore_ascii_case(self.binding.repo.name.as_str())) =>
            {
                RepoRef::new(owner, name)
            }
            RepoPart::Name(..) | RepoPart::Id => self.binding.repo.clone(),
        };
        if rest == own_rest {
            return Binding::new(self.binding.operation, repo, self.binding.parent);
        }
        // The same operation under another parent: every segment equal except the parent's,
        // which is another number in canonical decimal form.
        let parent = self.binding.parent.and_then(|parent| {
            let parent = parent.to_string();
            let at = own_rest.iter().position(|segment| *segment == parent)?;
            let other = rest.get(at)?;
            let number = other
                .parse::<u64>()
                .ok()
                .filter(|n| n.to_string() == *other)?;
            let same_shape = rest.len() == own_rest.len()
                && rest
                    .iter()
                    .zip(own_rest)
                    .enumerate()
                    .all(|(i, (a, b))| i == at || a == b);
            same_shape.then_some(number)
        });
        match parent {
            Some(parent) => Binding::new(self.binding.operation, repo, Some(parent)),
            None => unrecognized(),
        }
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

/// The repository part of an API path.
enum RepoPart<'a> {
    /// `repos/{owner}/{name}`.
    Name(&'a str, &'a str),
    /// `repositories/{id}`.
    Id,
}

/// The path segments of `url` below the base path `base` (without its trailing `/`).
fn below<'a>(base: &str, url: &'a Url) -> Option<Vec<&'a str>> {
    let rest = url.path().strip_prefix(base)?;
    let rest = rest.strip_prefix('/')?;
    Some(rest.split('/').collect())
}

/// Splits `segments` into the repository part and the collection path after it.
fn repo_part<'a, 'b>(segments: &'b [&'a str]) -> Option<(RepoPart<'a>, &'b [&'a str])> {
    match segments {
        ["repos", owner, name, rest @ ..] => Some((RepoPart::Name(owner, name), rest)),
        ["repositories", id, rest @ ..]
            if !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()) =>
        {
            Some((RepoPart::Id, rest))
        }
        _ => None,
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
    // The URL this request goes to: a `next` equal to it does not advance.
    let requested = match &key {
        Some(key) => key.url.clone(),
        None => prepared.request().url().clone(),
    };
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
            Some(url) if *url == requested => {
                return Err(GitHubError::LinkDidNotAdvance {
                    meta: Box::new(meta),
                });
            }
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
    Ok(Page {
        items,
        next,
        meta: PageInfo {
            response: meta,
            total_count,
        },
    })
}
