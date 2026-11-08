//! Link-header pagination: ordinary traversals ([`LinkPages::pages`], [`LinkPages::items`]) and
//! the page factory underneath them, also usable with `concord-pagination`'s engine.
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
//!   [`GitHubError::ContinuationMismatch`]. It is persisted with its binding
//!   ([`NextLink::to_stored`]), because the URL alone cannot say which repository it belongs to:
//!   GitHub may address the collection by repository id, or by a new name after a rename.
//!   [`LinkPages::restore`] accepts a stored continuation only for the factory's own binding, and
//!   only if the URL's path addresses that operation and parent. The link's own query carries
//!   GitHub's filters, so a resumed traversal continues the original listing.
//! - **Progress.** A `rel="next"` that requests the URL just requested would fetch the same page
//!   forever (only the caller's budget would end the chain): it is a
//!   [`GitHubError::LinkDidNotAdvance`], and the page's items are not returned. A fragment is
//!   never sent, so it is removed from a link before the comparison and is not kept.
//! - **Continuation before projection.** The `Link` header is read from the full response before
//!   the envelope is projected into items ([`Listing`]). A malformed header is a
//!   [`GitHubError::Link`], never "last page"; an envelope inconsistent with itself is a
//!   [`GitHubError::Envelope`].
//! - **Metadata.** Every page keeps its [`ResponseMeta`] (request id, rate limit, links) in
//!   [`PageInfo`], the page's `meta`.
//!
//! [`LinkPages::pages`] and [`LinkPages::items`] run [`LinkPages::fetch`] as
//! `concord_pagination`'s owned [`Pages`] / [`PageItems`] ([`GitHubPages`], [`GitHubItems`]):
//! `Ok(None)` only after a page without `rel="next"`; a cap ([`Pages::max_pages`], none by
//! default) reached while GitHub links on is `Err(PagesError::Bounded { resume, .. })`, and every
//! rule above that refuses a page (a foreign or non-advancing link, a malformed `Link`, a status)
//! is `Err(PagesError::Provider { error, failed, .. })` with the [`GitHubError`]: neither is a
//! silent end. The engine (`sequential`, `resume_items`) takes `|key| factory.fetch(key)` for
//! item-level checkpoints and engine reports.
//!
//! GitHub listings are live: items created, closed or relabelled between two requests can shift
//! between pages, so a traversal is a best-effort enumeration, not a snapshot.
//!
//! [pagination guide]: https://docs.github.com/en/rest/using-the-rest-api/using-pagination-in-the-rest-api

use std::fmt;
use std::future::Future;

use concord::Prepared;
use concord::reqwest::Url;
use concord_pagination::{Page, PageItems, Pages, Resume};

use crate::context::{GitHubContext, RepoContext};
use crate::error::{EnvelopeProblem, GitHubError};
use crate::ids::RepoRef;
use crate::meta::{ResponseMeta, WithMeta, send_edited};
use crate::models::{
    Artifact, ArtifactList, Job, JobList, Workflow, WorkflowList, WorkflowRun, WorkflowRunList,
};

/// GitHub operation ids, used as the operation part of a [`Binding`]. Which declared operation is
/// a listing, and with which id, is decided in one place: an exhaustive `match` over
/// [`GitHubOperation`](crate::api::GitHubOperation), so a new endpoint does not compile until it
/// is classified.
pub mod operation {
    use crate::api::GitHubOperation;

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
    /// Not a listing: the operation of the binding a restored continuation is reported with
    /// ([`GitHubError::ContinuationMismatch`](crate::GitHubError::ContinuationMismatch)'s
    /// `found`) when its path names no collection of the factory's operation, or when its stored
    /// operation is none of these ids.
    pub const UNRECOGNIZED: &str = "unrecognized-collection";

    /// The GitHub operation id of a declared Link-paged listing (`Some`), `None` for any other
    /// operation: the one table of listing ids ([`listing`] is derived from it). No wildcard arm:
    /// an endpoint added to the declaration without a decision here does not compile (E0004,
    /// naming its `GitHubOperation` variant).
    ///
    /// `workflow_runs` ([`WORKFLOW_RUNS`]) is a listing and keeps its id (a continuation that
    /// names it is recognized), but has no ready-made `RepoContext` factory: it is addressed by a
    /// workflow file name or id, while a [`Binding`](super::Binding)'s parent is a number.
    pub(crate) fn listing_id(operation: GitHubOperation) -> Option<&'static str> {
        use GitHubOperation as Op;
        match operation {
            Op::RepoLabels => Some(LABELS),
            Op::RepoIssues => Some(ISSUES),
            Op::RepoIssueComments => Some(ISSUE_COMMENTS),
            Op::RepoRepoComments => Some(REPO_COMMENTS),
            Op::RepoPulls => Some(PULLS),
            Op::RepoPullReviews => Some(PULL_REVIEWS),
            Op::RepoPullFiles => Some(PULL_FILES),
            Op::RepoActionsWorkflows => Some(WORKFLOWS),
            Op::RepoActionsRuns => Some(RUNS),
            Op::RepoActionsWorkflowRuns => Some(WORKFLOW_RUNS),
            Op::RepoActionsRunJobs => Some(RUN_JOBS),
            Op::RepoActionsArtifacts => Some(ARTIFACTS),
            Op::RepoActionsRunArtifacts => Some(RUN_ARTIFACTS),
            Op::RepoGet
            | Op::RepoIssue
            | Op::RepoComment
            | Op::RepoPull
            | Op::RepoPullReview
            | Op::RepoActionsRun
            | Op::RepoActionsJob
            | Op::RepoActionsArtifact
            | Op::RepoActionsDownloadArtifact => None,
        }
    }

    /// The listing operation id equal to `id`.
    pub(super) fn listing(id: &str) -> Option<&'static str> {
        GitHubOperation::ALL
            .iter()
            .filter_map(|&op| listing_id(op))
            .find(|listing| *listing == id)
    }
}

/// The version tag of [`NextLink::to_stored`]'s text form.
const STORED: &str = "github-link/1";

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

    /// The URL, **unredacted**. To persist the continuation, store [`NextLink::to_stored`]: the
    /// URL alone does not identify its repository.
    pub fn raw_url(&self) -> &Url {
        &self.url
    }

    /// The continuation as one line of text for an application to persist, with the URL
    /// **unredacted**: `github-link/1 {operation} {owner}/{name} {parent} {url}`, where `parent` is
    /// the parent's number or id, or `-` for a top-level listing. The binding is kept with the
    /// URL because a link may address the collection by repository id or, after a rename, by the
    /// new name; neither can be checked against the repository from the URL. Read it back with
    /// [`LinkPages::restore`].
    pub fn to_stored(&self) -> String {
        let parent = self
            .binding
            .parent
            .map_or_else(|| "-".to_owned(), |parent| parent.to_string());
        format!(
            "{STORED} {} {} {parent} {}",
            self.binding.operation, self.binding.repo, self.url
        )
    }
}

/// The parts of a stored continuation: operation id, repository, parent and URL.
fn parse_stored(stored: &str) -> Option<(&str, RepoRef, Option<u64>, Url)> {
    let mut parts = stored.split(' ');
    let (Some(STORED), Some(operation), Some(repo), Some(parent), Some(url), None) = (
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
    ) else {
        return None;
    };
    let (owner, name) = repo.split_once('/')?;
    if owner.is_empty() || name.is_empty() || name.contains('/') {
        return None;
    }
    let parent = match parent {
        "-" => None,
        number => Some(
            number
                .parse::<u64>()
                .ok()
                .filter(|n| n.to_string() == number)?,
        ),
    };
    Some((
        operation,
        RepoRef::new(owner, name),
        parent,
        Url::parse(url).ok()?,
    ))
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

/// The ordinary page traversal of a listing ([`LinkPages::pages`]): owned, nameable, its end
/// explicit (see the [module documentation](self)).
pub type GitHubPages<T> = Pages<T, NextLink, PageInfo, GitHubError>;

/// The ordinary item traversal of a listing ([`LinkPages::items`]).
pub type GitHubItems<T> = PageItems<T, NextLink, GitHubError>;

/// A page factory for one list operation of one repository. See the
/// [module documentation](self).
pub struct LinkPages<Op, R> {
    context: GitHubContext,
    binding: Binding,
    operation: Op,
    prepare: fn(Op) -> Result<Prepared<R>, concord::Error>,
}

impl<Op: Clone, R> Clone for LinkPages<Op, R> {
    fn clone(&self) -> Self {
        Self {
            context: self.context.clone(),
            binding: self.binding.clone(),
            operation: self.operation.clone(),
            prepare: self.prepare,
        }
    }
}

impl<Op, R> LinkPages<Op, R>
where
    Op: Clone + Send + 'static,
    R: Listing + Send + 'static,
    R::Item: Send + 'static,
{
    /// The pages of this listing from its first page: a [`Pages`] traversal over
    /// [`fetch`](Self::fetch), one page at a time, without a cap unless you set one
    /// ([`Pages::max_pages`]). `Ok(None)` only after a page without `rel="next"`; a cap reached
    /// while GitHub links on, or a refused or failed page, is an error that names the page to
    /// continue from. Nothing is sent until the first `next_page()`.
    ///
    /// ```no_run
    /// use std::num::NonZeroUsize;
    /// use concord_example_github_sdk::RepoContext;
    /// use concord_example_github_sdk::ids::IssueNumber;
    ///
    /// # async fn run(repo: RepoContext) -> Result<(), Box<dyn std::error::Error>> {
    /// let comments = repo.issue_comment_pages(IssueNumber(7), |op| op).items();
    /// let all = comments.collect_complete().await?; // `Err` keeps the prefix and the reason
    ///
    /// let cap = NonZeroUsize::new(10).unwrap();
    /// let mut pages = repo.label_pages(repo.handle().labels()).pages().max_pages(cap);
    /// while let Some(page) = pages.next_page().await? {
    ///     println!("{} labels, request {:?}", page.items.len(), page.meta.response.request_id());
    /// }
    /// # let _ = all; Ok(()) }
    /// ```
    pub fn pages(&self) -> GitHubPages<R::Item> {
        self.pages_from(Resume::Start)
    }

    /// [`pages`](Self::pages) from `from`: a link of this listing (a bounded or failed
    /// traversal's `PagesError::resume`, a [`restore`](Self::restore)d continuation), or
    /// `Resume::Start`. A link of another listing or origin is refused by the first page's
    /// request, before anything is sent.
    pub fn pages_from(&self, from: Resume<NextLink>) -> GitHubPages<R::Item> {
        let factory = self.clone();
        Pages::new(from, move |key| factory.fetch(key))
    }

    /// The items of this listing from its first page, one at a time ([`PageItems`] over
    /// [`pages`](Self::pages)); the same end rules.
    pub fn items(&self) -> GitHubItems<R::Item> {
        self.pages().items()
    }
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

    /// A continuation an application persisted ([`NextLink::to_stored`]). Nothing is sent.
    ///
    /// The stored binding must be this factory's: the same operation and parent, and the same
    /// repository (owner and name ASCII case-insensitively, as GitHub compares them). It says
    /// which repository the link belongs to, so a link that addresses the collection by
    /// repository id (`repositories/{id}/…`) or, after a rename, by another name is restored
    /// correctly, and a continuation of another repository is refused whatever its URL form. The
    /// URL must then address this factory's operation and parent: below the base path, a
    /// repository part (`repos/{owner}/{name}` or `repositories/{id}`) followed by exactly the
    /// path of the factory's own first request (for example `issues/11/comments` for issue 11's
    /// comments). The query (GitHub's filters, page or cursor) is kept as stored; a fragment is
    /// removed.
    ///
    /// # Errors
    ///
    /// - [`GitHubError::MalformedStoredLink`] if `stored` is not [`NextLink::to_stored`]'s form;
    /// - [`GitHubError::ContinuationMismatch`] if the stored binding is another one (`found` is
    ///   the stored binding; an unknown operation id is [`operation::UNRECOGNIZED`]), or if the
    ///   URL's path addresses another collection (`found` is what the path names, as far as it
    ///   can be read: another parent of the same operation, or [`operation::UNRECOGNIZED`]);
    /// - [`GitHubError::ForeignLink`] if the URL is not an API URL of this context;
    /// - the Concord build error if the factory's operation cannot be prepared.
    pub fn restore(&self, stored: &str) -> Result<NextLink, GitHubError> {
        let Some((operation_id, repo, parent, mut url)) = parse_stored(stored) else {
            return Err(GitHubError::MalformedStoredLink);
        };
        let mismatch = |found: Binding| GitHubError::ContinuationMismatch {
            expected: Box::new(self.binding.clone()),
            found: Box::new(found),
        };
        let same_repo = repo
            .owner
            .as_str()
            .eq_ignore_ascii_case(self.binding.repo.owner.as_str())
            && repo
                .name
                .as_str()
                .eq_ignore_ascii_case(self.binding.repo.name.as_str());
        if operation_id != self.binding.operation || !same_repo || parent != self.binding.parent {
            let operation = operation::listing(operation_id).unwrap_or(operation::UNRECOGNIZED);
            return Err(mismatch(Binding::new(operation, repo, parent)));
        }
        if !self.context.is_api_url(&url) {
            return Err(GitHubError::ForeignLink { meta: None });
        }
        url.set_fragment(None);
        let prepared = (self.prepare)(self.operation.clone())?;
        // The repository comes from the stored binding; the path must name the same collection.
        let found = self.binding_of(prepared.request().url(), &url);
        if found.operation != self.binding.operation || found.parent != self.binding.parent {
            return Err(mismatch(found));
        }
        Ok(NextLink {
            url,
            binding: self.binding.clone(),
        })
    }

    /// The binding `link`'s path names, read against `own`, the path of this factory's first
    /// request: its operation and parent equal this factory's exactly when `link` addresses the
    /// same collection (the repository is another name's, or this factory's for the id form).
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
    // The URL this request goes to, without a fragment (never sent): a `next` equal to it, also
    // without its fragment, does not advance.
    let mut requested = match &key {
        Some(key) => key.url.clone(),
        None => prepared.request().url().clone(),
    };
    requested.set_fragment(None);
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
        Ok(links) => match links.next().cloned().map(|mut url| {
            url.set_fragment(None);
            url
        }) {
            None => None,
            Some(url) if url == requested => {
                return Err(GitHubError::LinkDidNotAdvance {
                    meta: Box::new(meta),
                });
            }
            Some(url) if context.is_api_url(&url) => Some(NextLink { url, binding }),
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
