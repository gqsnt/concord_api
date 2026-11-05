//! The frozen initial consumer (stage 1 of the maintenance exercise). **Not edited after stage 1.**
//!
//! A downstream triage tool asks: "which open bugs changed recently, and how much discussion do
//! they have?" This SDK version can only express "bug" as a label, so bugs are the open issues
//! carrying the repository's bug label. The Issues API lists pull requests too; they are counted
//! apart, never reported as bugs. The listing follows GitHub's `Link` pages under a page budget.
//!
//! Written from the SDK's public documentation only (`context`, `filters`, `pages`, `paging`
//! rustdoc and `PROVIDER.md`), the way an application would use it:
//!
//! - [`open_bug_digest`]: the filter record ([`IssueFilter`]) through the typed wrapper
//!   ([`RepoContext::issues`]), the ready-made page factory ([`RepoContext::issue_pages`]) and
//!   `concord-pagination`'s sequential traversal;
//! - [`newest_bug`]: the generated operation's own setters
//!   (`repo.handle().issues().set_state(..)`), one request.

use std::fmt;
use std::num::NonZeroUsize;

use concord_example_github_sdk::filters::IssueFilter;
use concord_example_github_sdk::ids::{IssueNumber, Login};
use concord_example_github_sdk::models::{Issue, IssueItem};
use concord_example_github_sdk::params::{
    Direction, IssueSort, Labels, ParamError, PerPage, StateFilter, Timestamp,
};
use concord_example_github_sdk::{GitHubError, RepoContext};
use concord_pagination::{Budget, Outcome, Resume, sequential};

/// What the digest lists.
#[derive(Clone, Debug)]
pub struct BugQuery {
    /// The label that marks a bug in this repository (for example `bug`).
    pub label: String,
    /// Only bugs updated at or after this time.
    pub updated_since: Option<Timestamp>,
    /// Page size.
    pub per_page: PerPage,
    /// At most this many issue pages are requested.
    pub max_pages: NonZeroUsize,
}

/// One open bug.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BugSummary {
    /// The issue number.
    pub number: IssueNumber,
    /// The title.
    pub title: String,
    /// Number of comments.
    pub comments: u64,
    /// Assignees' logins.
    pub assignees: Vec<Login>,
    /// Last update.
    pub updated_at: Timestamp,
}

impl From<Issue> for BugSummary {
    fn from(issue: Issue) -> Self {
        Self {
            number: issue.number,
            title: issue.title,
            comments: issue.comments,
            assignees: issue.assignees.into_iter().map(|user| user.login).collect(),
            updated_at: issue.updated_at,
        }
    }
}

/// The digest of one repository.
#[derive(Debug)]
pub struct BugDigest {
    /// Open bugs, most recently updated first (GitHub's order).
    pub bugs: Vec<BugSummary>,
    /// Pull requests carrying the label (the Issues API lists them too); not bugs.
    pub pull_requests_skipped: usize,
    /// Issue pages fetched.
    pub pages: usize,
    /// `x-github-request-id` of each page, for a support request.
    pub request_ids: Vec<String>,
    /// `x-ratelimit-remaining` after the last page.
    pub rate_remaining: Option<u64>,
    /// Whether GitHub's listing was exhausted or the page budget stopped it.
    pub outcome: Outcome,
}

/// Why a digest could not be produced.
#[derive(Debug)]
pub enum DigestError {
    /// The query cannot be expressed (an empty label or one containing a comma).
    Query(ParamError),
    /// GitHub, the transport or a page failed.
    GitHub(GitHubError),
}

impl fmt::Display for DigestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Query(error) => write!(f, "bug digest: {error}"),
            Self::GitHub(error) => write!(f, "bug digest: {error}"),
        }
    }
}

impl std::error::Error for DigestError {}

impl From<ParamError> for DigestError {
    fn from(error: ParamError) -> Self {
        Self::Query(error)
    }
}

impl From<GitHubError> for DigestError {
    fn from(error: GitHubError) -> Self {
        Self::GitHub(error)
    }
}

/// Lists the open bugs of `repo` updated since `query.updated_since`, most recently updated first,
/// following GitHub's pages until the listing ends or `query.max_pages` pages were fetched.
///
/// # Errors
///
/// [`DigestError::Query`] before anything is sent; [`DigestError::GitHub`] for the first page that
/// failed (pages fetched before it are discarded: a digest is all or nothing).
pub async fn open_bug_digest(
    repo: &RepoContext,
    query: &BugQuery,
) -> Result<BugDigest, DigestError> {
    let filter = IssueFilter {
        state: Some(StateFilter::Open),
        labels: Some(Labels::new([query.label.as_str()])?),
        since: query.updated_since.clone(),
        sort: Some(IssueSort::Updated),
        direction: Some(Direction::Desc),
        per_page: Some(query.per_page),
        ..IssueFilter::default()
    };
    let pages = repo.issue_pages(repo.issues(&filter));
    let mut chain = sequential(Resume::Start, Budget::new(query.max_pages), |key| {
        pages.fetch(key)
    });
    let mut digest = BugDigest {
        bugs: Vec::new(),
        pull_requests_skipped: 0,
        pages: 0,
        request_ids: Vec::new(),
        rate_remaining: None,
        outcome: Outcome::Exhausted,
    };
    while let Some(page) = chain.next().await {
        let page = page?;
        digest.pages += 1;
        if let Some(id) = page.meta.response.request_id() {
            digest.request_ids.push(id.to_owned());
        }
        digest.rate_remaining = page.meta.response.rate_limit().remaining;
        for item in page.items {
            match item {
                IssueItem::Issue(issue) => digest.bugs.push(issue.into()),
                IssueItem::PullRequest { .. } => digest.pull_requests_skipped += 1,
            }
        }
    }
    digest.outcome = chain.finish().outcome;
    Ok(digest)
}

/// The most recently opened bug among the ten newest open items carrying `label` (one request;
/// pull requests among them are skipped), for a status badge.
///
/// # Errors
///
/// [`DigestError::Query`] for an unusable label; [`DigestError::GitHub`] if the request failed.
pub async fn newest_bug(
    repo: &RepoContext,
    label: &str,
) -> Result<Option<BugSummary>, DigestError> {
    let items = repo
        .handle()
        .issues()
        .set_state(StateFilter::Open)
        .set_labels(Labels::new([label])?)
        .set_sort(IssueSort::Created)
        .set_direction(Direction::Desc)
        .set_per_page(PerPage::new(10)?)
        .send()
        .await
        .map_err(GitHubError::from)?;
    Ok(items.into_iter().find_map(|item| match item {
        IssueItem::Issue(issue) => Some(issue.into()),
        IssueItem::PullRequest { .. } => None,
    }))
}
