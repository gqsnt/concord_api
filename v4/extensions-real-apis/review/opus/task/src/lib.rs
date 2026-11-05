//! Review task (strand OPUS): GH-A with a changed requirement, written only against public APIs
//! (`concord-example-github-sdk`, `concord-pagination`, `concord`).
//!
//! Changed requirement ("keep-going audit"):
//!
//! 1. Items are processed **incrementally**: each issue / pull request is handed to an
//!    [`AuditSink`] as soon as its child collections were walked; nothing is collected. Child
//!    pages are streamed and counted, not kept.
//! 2. After each processed item the sink receives a **durable position** ([`StoredPosition`]:
//!    plain strings and integers, persistable) that resumes right after that item, also in a new
//!    process (the `NextLink` is persisted as its raw URL and restored with `LinkPages::restore`).
//! 3. A **child failure** (comments, pull-request details, reviews) no longer stops the audit: it
//!    is recorded with a persistable restart point ([`StoredFailure`]) and the audit continues with
//!    the next item. A parent page failure still stops (the chain has no next key).
//!    `max_child_failures` bounds how many child failures a run accepts before it stops.
//! 4. A **repair pass** ([`repair`]) re-runs only the failed children from their restart pages,
//!    serially, continuing past failures, and reports the still-failed ones with their new
//!    restart points.
//! 5. A resumed run checks that the replayed page still has the last processed item at
//!    `skip - 1` and refuses a changed listing ([`AuditError::Stale`]).

use std::fmt;
use std::num::NonZeroUsize;
use std::ops::ControlFlow;

use concord::reqwest::Url;
use concord_example_github_sdk::filters::IssueFilter;
use concord_example_github_sdk::ids::{IssueNumber, PullNumber};
use concord_example_github_sdk::meta::send_with_meta;
use concord_example_github_sdk::models::IssueItem;
use concord_example_github_sdk::paging::{LinkPage, LinkPages, Listing, NextLink};
use concord_example_github_sdk::{GitHubError, RepoContext};
use concord_pagination::{
    Budget, ItemCheckpoint, OnError, Options, Outcome, Resume, independent, resume_items,
    sequential,
};

/// A persistable item position: the issue page (its raw `Link` URL, `None` for the first page),
/// how many of its items were processed, and the number of the last processed item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredPosition {
    /// The page to replay (`None`: the first page of the configured listing).
    pub page_url: Option<String>,
    /// Items of that page already processed.
    pub skip: usize,
    /// The last processed item (checked on resume when `skip > 0`).
    pub last_number: Option<u64>,
}

/// Which child collection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChildKind {
    /// An issue's comments.
    Comments,
    /// A pull request's details (one request), then its reviews.
    PullDetails,
    /// A pull request's reviews.
    Reviews,
}

/// A persistable child failure: which item, which collection, and its restart page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredFailure {
    /// The issue or pull-request number.
    pub number: u64,
    /// The collection.
    pub kind: ChildKind,
    /// The restart page (`None`: the collection's first page).
    pub restart_url: Option<String>,
}

/// How a child collection ended for one item in this run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChildState {
    /// Every page fetched.
    Complete,
    /// The child budget was used while GitHub had more.
    Bounded {
        /// The next page.
        restart_url: String,
    },
    /// A page failed; see the run's failures.
    Failed,
}

/// One processed item (no titles, no bodies).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ItemRecord {
    /// An issue and its comments.
    Issue {
        /// Number.
        number: u64,
        /// Comments seen by this run.
        comments: u64,
        /// How the comment collection ended.
        state: ChildState,
    },
    /// A pull request, its details and reviews.
    PullRequest {
        /// Number.
        number: u64,
        /// `merged`, when the details were fetched.
        merged: Option<bool>,
        /// Reviews seen by this run.
        reviews: u64,
        /// How the details + reviews ended.
        state: ChildState,
    },
}

impl ItemRecord {
    /// The item number.
    pub fn number(&self) -> u64 {
        match self {
            Self::Issue { number, .. } | Self::PullRequest { number, .. } => *number,
        }
    }
}

/// Where processed items go. One call per item, carrying everything that must be persisted
/// atomically: the record, the position after it, and the item's child failure.
pub trait AuditSink {
    /// Returning `Break` stops the run after this item (the position already covers it).
    fn processed(
        &mut self,
        record: ItemRecord,
        after: Option<StoredPosition>,
        failure: Option<&StoredFailure>,
    ) -> ControlFlow<()>;
}

/// Why a run or a step failed.
pub enum AuditError {
    /// A GitHub request failed.
    GitHub(GitHubError),
    /// A stored URL is not a URL.
    BadStoredUrl,
    /// The replayed page no longer has the last processed item at `skip - 1`.
    Stale {
        /// The stored last number.
        expected: u64,
        /// What the page has there.
        found: Option<u64>,
    },
}

impl fmt::Debug for AuditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GitHub(e) => f.debug_tuple("GitHub").field(e).finish(),
            Self::BadStoredUrl => f.write_str("BadStoredUrl"),
            Self::Stale { expected, found } => f
                .debug_struct("Stale")
                .field("expected", expected)
                .field("found", found)
                .finish(),
        }
    }
}

impl From<GitHubError> for AuditError {
    fn from(e: GitHubError) -> Self {
        Self::GitHub(e)
    }
}

/// A child failure with its typed error.
#[derive(Debug)]
pub struct ChildFailure {
    /// The persistable part.
    pub stored: StoredFailure,
    /// The error.
    pub error: GitHubError,
}

/// How a run ended.
#[derive(Debug)]
pub enum RunEnd {
    /// Listing exhausted, no child failure.
    Complete,
    /// Listing exhausted, some children failed (see `failures`).
    CompleteWithChildFailures,
    /// The issue-page budget was used while GitHub had more.
    Bounded {
        /// Continue here.
        position: StoredPosition,
    },
    /// The sink stopped the run, or `max_child_failures` was reached.
    Stopped {
        /// Continue here (`None` if the stop came after the last item).
        position: Option<StoredPosition>,
    },
    /// The run could not continue (parent page, restore or stale listing).
    Failed {
        /// Why.
        error: AuditError,
        /// Continue here once the cause is fixed.
        position: Option<StoredPosition>,
    },
}

/// A run's report: how it ended, the child failures of this run, items processed.
#[derive(Debug)]
pub struct RunReport {
    /// How it ended.
    pub end: RunEnd,
    /// Child failures of this run, in item order.
    pub failures: Vec<ChildFailure>,
    /// Items handed to the sink.
    pub processed: u64,
}

/// Inputs of a keep-going run.
#[derive(Clone, Debug)]
pub struct KeepGoingInput {
    /// Issue filters.
    pub filter: IssueFilter,
    /// Issue pages fetched by this run.
    pub issue_pages: NonZeroUsize,
    /// Pages per child collection.
    pub child_pages: NonZeroUsize,
    /// A stored position of an earlier run.
    pub from: Option<StoredPosition>,
    /// Stop after this many child failures (`None`: never for child failures).
    pub max_child_failures: Option<NonZeroUsize>,
}

fn store(checkpoint: &ItemCheckpoint<NextLink>, last_number: Option<u64>) -> StoredPosition {
    StoredPosition {
        page_url: checkpoint.page.key().map(|l| l.raw_url().to_string()),
        skip: checkpoint.skip,
        last_number,
    }
}

fn restore_link<Op: Clone, R: Listing>(
    pages: &LinkPages<Op, R>,
    url: Option<&str>,
) -> Result<Resume<NextLink>, AuditError> {
    match url {
        None => Ok(Resume::Start),
        Some(url) => {
            let url = Url::parse(url).map_err(|_| AuditError::BadStoredUrl)?;
            Ok(Resume::At(pages.restore(url)?))
        }
    }
}

/// Runs the keep-going audit. See the crate documentation.
pub async fn keep_going_audit(
    repo: &RepoContext,
    input: KeepGoingInput,
    sink: &mut impl AuditSink,
) -> RunReport {
    let mut failures: Vec<ChildFailure> = Vec::new();
    let mut processed = 0u64;
    let pages = repo.issue_pages(repo.issues(&input.filter));
    let start = match &input.from {
        None => ItemCheckpoint {
            page: Resume::Start,
            skip: 0,
        },
        Some(stored) => match restore_link(&pages, stored.page_url.as_deref()) {
            Ok(page) => ItemCheckpoint {
                page,
                skip: stored.skip,
            },
            Err(error) => {
                return RunReport {
                    end: RunEnd::Failed {
                        error,
                        position: input.from.clone(),
                    },
                    failures,
                    processed,
                };
            }
        },
    };
    // `resume_items` skips `skip` items of the replayed page without showing them, so the
    // staleness check has to live in the factory, armed for its first call only.
    let mut verify = input
        .from
        .as_ref()
        .filter(|s| s.skip > 0)
        .and_then(|s| s.last_number.map(|n| (s.skip, n)));
    let mut items = resume_items(start, Budget::new(input.issue_pages), |key| {
        let page = pages.fetch(key);
        let check = verify.take();
        async move {
            let page: LinkPage<IssueItem> = page.await?;
            if let Some((skip, expected)) = check {
                let found = page.items.get(skip - 1).map(|i| i.number().get());
                if found != Some(expected) {
                    return Err(AuditError::Stale { expected, found });
                }
            }
            Ok(page)
        }
    });
    let child_budget = Budget::new(input.child_pages);
    loop {
        let Some(next) = items.next().await else {
            break;
        };
        let item = match next {
            Ok(item) => item,
            Err(error) => {
                let report = items.finish();
                let position = report.resume.as_ref().map(|cp| {
                    // The failed page is a page boundary unless the failure is the stale check
                    // on the replayed page, which keeps the stored position unchanged.
                    match &error {
                        AuditError::Stale { .. } => input.from.clone().unwrap_or(store(cp, None)),
                        _ => store(cp, None),
                    }
                });
                return RunReport {
                    end: RunEnd::Failed { error, position },
                    failures,
                    processed,
                };
            }
        };
        let number = item.number().get();
        let (record, failure) = audit_item(repo, item, None, child_budget).await;
        processed += 1;
        let after = items.checkpoint().map(|cp| store(cp, Some(number)));
        let flow = sink.processed(record, after.clone(), failure.as_ref().map(|f| &f.stored));
        if let Some(failure) = failure {
            failures.push(failure);
        }
        let limit_reached = input
            .max_child_failures
            .is_some_and(|limit| failures.len() >= limit.get());
        if flow.is_break() || limit_reached {
            drop(items.finish());
            return RunReport {
                end: RunEnd::Stopped { position: after },
                failures,
                processed,
            };
        }
    }
    let report = items.finish();
    let end = match (report.outcome, report.resume) {
        (Outcome::Exhausted, _) if failures.is_empty() => RunEnd::Complete,
        (Outcome::Exhausted, _) => RunEnd::CompleteWithChildFailures,
        (Outcome::Bounded, Some(cp)) => RunEnd::Bounded {
            position: store(&cp, None),
        },
        (outcome, resume) => unreachable!("{outcome:?} {resume:?}"),
    };
    RunReport {
        end,
        failures,
        processed,
    }
}

/// Walks one child chain from `from`, counting items. Returns the count, the state and, on
/// failure, the restart page and error.
async fn walk<Op: Clone, R: Listing>(
    pages: &LinkPages<Op, R>,
    from: Resume<NextLink>,
    budget: Budget,
) -> (u64, ChildState, Option<(Resume<NextLink>, GitHubError)>) {
    let mut chain = sequential(from, budget, |key| pages.fetch(key));
    let mut count = 0u64;
    let mut error = None;
    while let Some(page) = chain.next().await {
        match page {
            Ok(page) => count += page.items.len() as u64,
            Err(e) => error = Some(e),
        }
    }
    let report = chain.finish();
    match error {
        Some(error) => (
            count,
            ChildState::Failed,
            Some((
                report.resume.expect("a failed chain keeps its failed page"),
                error,
            )),
        ),
        None => match (report.outcome, report.resume) {
            (Outcome::Bounded, Some(Resume::At(link))) => (
                count,
                ChildState::Bounded {
                    restart_url: link.raw_url().to_string(),
                },
                None,
            ),
            _ => (count, ChildState::Complete, None),
        },
    }
}

fn failure_of(
    number: u64,
    kind: ChildKind,
    restart: Resume<NextLink>,
    error: GitHubError,
) -> ChildFailure {
    ChildFailure {
        stored: StoredFailure {
            number,
            kind,
            restart_url: restart.key().map(|l| l.raw_url().to_string()),
        },
        error,
    }
}

/// Audits one item's children from an optional restart (`child_from`: kind and page).
async fn audit_item(
    repo: &RepoContext,
    item: IssueItem,
    child_from: Option<(ChildKind, Resume<NextLink>)>,
    budget: Budget,
) -> (ItemRecord, Option<ChildFailure>) {
    let number = item.number();
    match item.pull_number() {
        None => comments(repo, number, child_from.map_or(Resume::Start, |c| c.1), budget).await,
        Some(pull) => pull_request(repo, pull, child_from, budget).await,
    }
}

async fn comments(
    repo: &RepoContext,
    number: IssueNumber,
    from: Resume<NextLink>,
    budget: Budget,
) -> (ItemRecord, Option<ChildFailure>) {
    let pages = repo.issue_comment_pages(number, |op| op);
    let (count, state, failed) = walk(&pages, from, budget).await;
    let record = ItemRecord::Issue {
        number: number.get(),
        comments: count,
        state,
    };
    let failure = failed.map(|(restart, e)| failure_of(number.get(), ChildKind::Comments, restart, e));
    (record, failure)
}

async fn pull_request(
    repo: &RepoContext,
    pull: PullNumber,
    child_from: Option<(ChildKind, Resume<NextLink>)>,
    budget: Budget,
) -> (ItemRecord, Option<ChildFailure>) {
    let number = IssueNumber::from(pull).get();
    // Details unless the restart is inside the reviews.
    let (merged, reviews_from) = match child_from {
        Some((ChildKind::Reviews, page)) => (None, page),
        _ => {
            let details = match repo.handle().pull(pull).prepare() {
                Err(e) => Err(GitHubError::from(e)),
                Ok(prepared) => send_with_meta(prepared).await,
            };
            match details {
                Ok(with) => (Some(with.value.merged), Resume::Start),
                Err(error) => {
                    let record = ItemRecord::PullRequest {
                        number,
                        merged: None,
                        reviews: 0,
                        state: ChildState::Failed,
                    };
                    let failure = failure_of(number, ChildKind::PullDetails, Resume::Start, error);
                    return (record, Some(failure));
                }
            }
        }
    };
    let pages = repo.pull_review_pages(pull, |op| op);
    let (count, state, failed) = walk(&pages, reviews_from, budget).await;
    let record = ItemRecord::PullRequest {
        number,
        merged,
        reviews: count,
        state,
    };
    let failure = failed.map(|(restart, e)| failure_of(number, ChildKind::Reviews, restart, e));
    (record, failure)
}

/// One repaired child.
#[derive(Debug)]
pub struct RepairReport {
    /// Records of the children that were re-walked successfully (or bounded), in failure order.
    pub repaired: Vec<ItemRecord>,
    /// Children that failed again, with their *new* restart points.
    pub still_failed: Vec<ChildFailure>,
    /// The traversal outcome.
    pub outcome: Outcome,
}

/// Re-runs failed children from their restart pages, one at a time (GitHub asks for serial
/// requests), continuing past failures.
pub async fn repair(
    repo: &RepoContext,
    failed: Vec<StoredFailure>,
    child_pages: NonZeroUsize,
) -> RepairReport {
    let Some(count) = NonZeroUsize::new(failed.len()) else {
        return RepairReport {
            repaired: Vec::new(),
            still_failed: Vec::new(),
            outcome: Outcome::Exhausted,
        };
    };
    let budget = Budget::new(child_pages);
    let options = Options::new(Budget::new(count)).on_error(OnError::Continue);
    let mut jobs = independent(failed, options, |stored: StoredFailure| async move {
        repair_one(repo, stored, budget).await
    });
    let mut repaired = Vec::new();
    let mut still_failed = Vec::new();
    while let Some(outcome) = jobs.next().await {
        match outcome {
            Ok(done) => repaired.push(done.value),
            // The key is the *old* restart point; the error value carries the new one.
            Err(failed) => still_failed.push(failed.value),
        }
    }
    let report = jobs.finish();
    RepairReport {
        repaired,
        still_failed,
        outcome: report.outcome,
    }
}

async fn repair_one(
    repo: &RepoContext,
    stored: StoredFailure,
    budget: Budget,
) -> Result<ItemRecord, ChildFailure> {
    let number = IssueNumber(stored.number);
    let restore_error = |error: AuditError| ChildFailure {
        stored: stored.clone(),
        error: match error {
            AuditError::GitHub(e) => e,
            // A stored URL that does not parse: report it as a foreign link (nothing sent).
            _ => GitHubError::ForeignLink { meta: None },
        },
    };
    let (record, failure) = match stored.kind {
        ChildKind::Comments => {
            let pages = repo.issue_comment_pages(number, |op| op);
            let from = match restore_link(&pages, stored.restart_url.as_deref()) {
                Ok(from) => from,
                Err(e) => return Err(restore_error(e)),
            };
            let (count, state, failed) = walk(&pages, from, budget).await;
            (
                ItemRecord::Issue {
                    number: stored.number,
                    comments: count,
                    state,
                },
                failed.map(|(r, e)| failure_of(stored.number, ChildKind::Comments, r, e)),
            )
        }
        ChildKind::PullDetails => {
            pull_request(repo, PullNumber::from(number), None, budget).await
        }
        ChildKind::Reviews => {
            let pull = PullNumber::from(number);
            let pages = repo.pull_review_pages(pull, |op| op);
            let from = match restore_link(&pages, stored.restart_url.as_deref()) {
                Ok(from) => from,
                Err(e) => return Err(restore_error(e)),
            };
            pull_request(repo, pull, Some((ChildKind::Reviews, from)), budget).await
        }
    };
    match failure {
        None => Ok(record),
        Some(failure) => Err(failure),
    }
}

/// Alternative attempt: the same keep-going traversal expressed with
/// `concord_pagination::manifest_jobs` (discovery = the issue listing, one job per item = its
/// child collections), window 1, `OnError::Continue`. Returns the records in discovery order,
/// the failed descriptors and the traversal's checkpoint after the first record.
pub mod via_jobs {
    use super::*;
    use concord_pagination::{JobOptions, JobsCheckpoint, Manifest, manifest_jobs};

    /// A job descriptor: the item number and whether it is a pull request.
    pub type Descriptor = (u64, bool);

    /// What the alternative produced.
    pub struct JobsRun {
        /// Records, in discovery order.
        pub records: Vec<ItemRecord>,
        /// Failed descriptors (from the report).
        pub failed: Vec<Descriptor>,
        /// The traversal's own checkpoint taken right after the first record.
        pub first_checkpoint: Option<JobsCheckpoint<Descriptor, NextLink>>,
        /// The outcome.
        pub outcome: Outcome,
    }

    /// Runs it.
    pub async fn run(repo: &RepoContext, filter: &IssueFilter, child_pages: NonZeroUsize) -> JobsRun {
        let pages = repo.issue_pages(repo.issues(filter));
        let budget = Budget::new(child_pages);
        let options = JobOptions::new(
            Budget::new(NonZeroUsize::new(10).unwrap()),
            Options::new(Budget::new(NonZeroUsize::new(100).unwrap())).on_error(OnError::Continue),
        );
        let mut jobs = manifest_jobs(
            Resume::Start,
            options,
            |key| {
                let page = pages.fetch(key);
                async move {
                    let page = page.await?;
                    let items = page
                        .items
                        .iter()
                        .map(|i| (i.number().get(), i.is_pull_request()))
                        .collect();
                    Ok::<_, GitHubError>(Manifest::new(items, page.next))
                }
            },
            |(number, is_pr): Descriptor| async move {
                let (record, failure) = if is_pr {
                    pull_request(repo, PullNumber::from(IssueNumber(number)), None, budget).await
                } else {
                    comments(repo, IssueNumber(number), Resume::Start, budget).await
                };
                match failure {
                    None => Ok(record),
                    Some(failure) => Err((record, failure)),
                }
            },
        );
        let mut records = Vec::new();
        let mut first_checkpoint = None;
        while let Some(outcome) = jobs.next().await {
            match outcome {
                Ok(done) => records.push(done.value),
                Err(concord_pagination::JobsError::Job(failed)) => records.push(failed.value.0),
                Err(concord_pagination::JobsError::Discovery(_)) => {}
            }
            if first_checkpoint.is_none() {
                first_checkpoint = Some(jobs.checkpoint());
            }
        }
        let report = jobs.finish();
        JobsRun {
            records,
            failed: report.failed().cloned().collect(),
            first_checkpoint,
            outcome: report.outcome,
        }
    }
}
