//! The updated consumer (stage 3 of the maintenance exercise), beside the frozen one.
//!
//! The same triage question as [`crate::before`], now that the repository marks bugs with GitHub's
//! issue **type** rather than a label: the stage 2 `type` filter selects them on the server
//! (through the filter record, [`IssueFilter::issue_type`], and through the generated setter
//! `set_issue_type`), and each bug's event history comes from `issues/list-events`, an operation
//! the SDK does not declare, added through its public context in [`crate::issue_events`].

use std::fmt;
use std::num::NonZeroUsize;

use concord_example_github_sdk::filters::IssueFilter;
use concord_example_github_sdk::ids::{IssueNumber, Login};
use concord_example_github_sdk::models::IssueItem;
use concord_example_github_sdk::params::{Direction, IssueSort, PerPage, StateFilter, Timestamp};
use concord_example_github_sdk::{GitHubError, RepoContext};
use concord_pagination::{Budget, Outcome, Resume, sequential};

use crate::before::BugSummary;
use crate::issue_events::{IssueEvent, IssueEventPages};

/// What the digest lists.
#[derive(Clone, Debug)]
pub struct TypedBugQuery {
    /// The issue type that marks a bug (for example `Bug`), `*` or `none`.
    pub issue_type: String,
    /// Only bugs updated at or after this time.
    pub updated_since: Option<Timestamp>,
    /// Page size of the issue listing.
    pub per_page: PerPage,
    /// At most this many issue pages.
    pub max_pages: NonZeroUsize,
    /// At most this many event pages per bug.
    pub event_pages: NonZeroUsize,
}

/// One bug with its event history.
#[derive(Debug)]
pub struct BugTrail {
    /// The bug, as the frozen consumer summarizes it.
    pub bug: BugSummary,
    /// Its events, oldest first (GitHub's order).
    pub events: Vec<IssueEvent>,
    /// Whether the event listing was exhausted or bounded by `event_pages`.
    pub events_outcome: Outcome,
    /// `x-github-request-id` of each event page.
    pub event_request_ids: Vec<String>,
}

impl BugTrail {
    /// Who last applied a label, if an event says so.
    pub fn last_labeled_by(&self) -> Option<&Login> {
        self.events
            .iter()
            .rev()
            .find(|event| event.event == "labeled")
            .and_then(|event| event.actor.as_ref())
            .map(|actor| &actor.login)
    }

    /// How often the bug was reopened.
    pub fn times_reopened(&self) -> usize {
        self.events
            .iter()
            .filter(|event| event.event == "reopened")
            .count()
    }
}

/// The digest of one repository.
#[derive(Debug)]
pub struct TypedBugDigest {
    /// Open bugs of the type, most recently updated first, with their events.
    pub bugs: Vec<BugTrail>,
    /// Pull requests the listing returned (the Issues API lists them too); not bugs.
    pub pull_requests_skipped: usize,
    /// Issue pages fetched.
    pub issue_pages: usize,
    /// `x-github-request-id` of each issue page.
    pub request_ids: Vec<String>,
    /// Whether the issue listing was exhausted or bounded by `max_pages`.
    pub outcome: Outcome,
}

/// Why the digest could not be produced.
#[derive(Debug)]
pub enum TrailError {
    /// An issue page failed.
    GitHub(GitHubError),
    /// An event page of one bug failed.
    Events {
        /// The bug.
        number: IssueNumber,
        /// The failure (the SDK's own error type, also for this extension operation).
        error: GitHubError,
    },
}

impl fmt::Display for TrailError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GitHub(error) => write!(f, "typed bug digest: {error}"),
            Self::Events { number, error } => {
                write!(f, "typed bug digest: events of #{number}: {error}")
            }
        }
    }
}

impl std::error::Error for TrailError {}

impl From<GitHubError> for TrailError {
    fn from(error: GitHubError) -> Self {
        Self::GitHub(error)
    }
}

/// Lists the open issues of type `query.issue_type` updated since `query.updated_since`, most
/// recently updated first, each with up to `query.event_pages` pages of events.
///
/// # Errors
///
/// [`TrailError::GitHub`] for the first issue page that failed, [`TrailError::Events`] for the
/// first event page that failed (a digest is all or nothing, as in the frozen consumer).
pub async fn typed_bug_trails(
    repo: &RepoContext,
    query: &TypedBugQuery,
) -> Result<TypedBugDigest, TrailError> {
    let filter = IssueFilter {
        state: Some(StateFilter::Open),
        issue_type: Some(query.issue_type.clone()),
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
    let mut digest = TypedBugDigest {
        bugs: Vec::new(),
        pull_requests_skipped: 0,
        issue_pages: 0,
        request_ids: Vec::new(),
        outcome: Outcome::Exhausted,
    };
    while let Some(page) = chain.next().await {
        let page = page?;
        digest.issue_pages += 1;
        if let Some(id) = page.meta.response.request_id() {
            digest.request_ids.push(id.to_owned());
        }
        for item in page.items {
            match item {
                IssueItem::Issue(issue) => {
                    let trail = trail(repo, issue.into(), query.event_pages).await?;
                    digest.bugs.push(trail);
                }
                IssueItem::PullRequest { .. } => digest.pull_requests_skipped += 1,
            }
        }
    }
    digest.outcome = chain.finish().outcome;
    Ok(digest)
}

async fn trail(
    repo: &RepoContext,
    bug: BugSummary,
    event_pages: NonZeroUsize,
) -> Result<BugTrail, TrailError> {
    let pages = IssueEventPages::new(repo, bug.number).per_page(PerPage::MAX);
    let mut chain = sequential(Resume::Start, Budget::new(event_pages), |key| {
        pages.fetch(key)
    });
    let mut events = Vec::new();
    let mut event_request_ids = Vec::new();
    while let Some(page) = chain.next().await {
        let page = page.map_err(|error| TrailError::Events {
            number: bug.number,
            error,
        })?;
        if let Some(id) = page.meta.request_id() {
            event_request_ids.push(id.to_owned());
        }
        events.extend(page.items);
    }
    Ok(BugTrail {
        bug,
        events,
        events_outcome: chain.finish().outcome,
        event_request_ids,
    })
}

/// The most recently opened issue of type `issue_type` among the ten newest open ones (one
/// request, through the generated setter), for a status badge.
///
/// # Errors
///
/// [`TrailError::GitHub`] if the request failed.
pub async fn newest_of_type(
    repo: &RepoContext,
    issue_type: &str,
) -> Result<Option<BugSummary>, TrailError> {
    let items = repo
        .handle()
        .issues()
        .set_state(StateFilter::Open)
        .set_issue_type(issue_type)
        .set_sort(IssueSort::Created)
        .set_direction(Direction::Desc)
        .set_per_page(PerPage::new(10).expect("10 is a valid page size"))
        .send()
        .await
        .map_err(GitHubError::from)?;
    Ok(items.into_iter().find_map(|item| match item {
        IssueItem::Issue(issue) => Some(issue.into()),
        IssueItem::PullRequest { .. } => None,
    }))
}
