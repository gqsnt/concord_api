//! GH-A — repository activity audit.
//!
//! **Input:** a [`RepoContext`] (credentials, version, repository), explicit [`IssueFilter`]s, an
//! [`AuditBudget`], optionally the repository `ETag` of an earlier audit and a continuation.
//! **Output:** an [`ActivityReport`]: the repository, one entry per issue or pull request (with
//! their comments, or details and reviews), the request log (request ids, rate limits), and a
//! [`Completion`] that carries a usable [`AuditContinuation`] whenever the audit did not finish.
//!
//! Traversal (serial, as GitHub asks):
//!
//! 1. The repository, conditionally when an `ETag` is given (`304` → [`RepositoryState::NotModified`]).
//! 2. The issue listing with the filters (`GET .../issues`, pull requests included) as the parent
//!    collection, through `concord-pagination`'s item consumer (`resume_items`), so the audit can
//!    stop between items and resume at an item-level checkpoint.
//! 3. For each issue its comments, for each pull request its details (`GET .../pulls/{n}`) and
//!    its reviews: child collections, each a `concord-pagination` sequential chain with its own
//!    page budget. A child bounded by its budget is reported with its own resume link.
//!
//! The audit stays on `concord-pagination`'s engine rather than the SDK's ordinary `pages()` /
//! `items()`: the parent resumes from an item-level checkpoint (`resume_items`), which the
//! ordinary traversal cannot start from, and the continuation is assembled from the engine's
//! reports (the parent's checkpoint before each item, each child's failed and resume pages).
//!
//! **Failures.** The first failure stops the audit (nothing is retried). The report keeps
//! everything collected so far, says what failed ([`FailedAt`]: the repository, an issue page,
//! or a child page with its issue number) and gives a continuation: the parent checkpoint *before*
//! the item whose child failed, plus the child's restart page. Resuming re-emits that item (its
//! page is fetched again and the earlier items are skipped), refetches pull-request details, and
//! continues the child at the failed page. Items already reported are not reported again.
//!
//! **Consistency.** GitHub listings are live: issues opened, closed or relabelled between two
//! requests can move between pages. Item checkpoints replay a page and assume it is stable, so the
//! audit checks the replayed item's number against the continuation's and reports
//! [`AuditError::StaleContinuation`] instead of attaching a child cursor to another issue. The
//! report is a best-effort enumeration ([`Consistency::LiveBestEffort`]), never a snapshot.

use std::fmt;
use std::num::NonZeroUsize;

use concord_example_github_sdk::filters::IssueFilter;
use concord_example_github_sdk::ids::{
    CommentId, IssueNumber, Login, PullNumber, PullRequestId, ReviewId,
};
use concord_example_github_sdk::meta::{Conditional, send_conditional, send_with_meta};
use concord_example_github_sdk::models::{IssueComment, IssueItem, Review, ReviewState, State};
use concord_example_github_sdk::paging::{LinkPage, NextLink, operation};
use concord_example_github_sdk::params::{ETag, Timestamp};
use concord_example_github_sdk::{GitHubError, RepoContext, RepoRef};
use concord_pagination::{Budget, ItemCheckpoint, Outcome, Resume, resume_items, sequential};

use crate::requests::{RequestLog, RequestRecord, record_page};

/// Page budgets of one audit run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuditBudget {
    /// Issue-listing pages fetched by this run (the parent collection).
    pub issue_pages: NonZeroUsize,
    /// Pages per child collection (the comments of one issue, the reviews of one pull request).
    pub child_pages: NonZeroUsize,
}

/// What to audit.
#[derive(Clone, Debug)]
pub struct AuditInput {
    /// Issue filters (state, labels, since, ...); `None` fields stay absent on the wire.
    pub filter: IssueFilter,
    /// Page budgets.
    pub budget: AuditBudget,
    /// The repository `ETag` of an earlier audit: the repository is then refreshed
    /// conditionally.
    pub repository_etag: Option<ETag>,
    /// Where an earlier run stopped ([`Completion::continuation`]); `None` starts at the first
    /// issue page.
    pub from: Option<AuditContinuation>,
}

/// The repository as this run saw it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RepositoryState {
    /// Fetched (first audit, or changed since the given `ETag`).
    Fetched {
        /// Name, default branch and counts.
        summary: RepositorySummary,
        /// The new `ETag`, for the next audit.
        etag: Option<ETag>,
    },
    /// `304 Not Modified`: unchanged since the given `ETag`.
    NotModified {
        /// The `ETag` that is still current.
        etag: ETag,
    },
}

/// The repository fields the audit reports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepositorySummary {
    /// `owner/name`.
    pub full_name: String,
    /// Default branch.
    pub default_branch: String,
    /// Open issues and pull requests, as GitHub counts them.
    pub open_issues_count: u64,
    /// Archived.
    pub archived: bool,
}

/// A comment as the audit reports it (no body).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommentSummary {
    /// Comment id.
    pub id: CommentId,
    /// Author.
    pub author: Option<Login>,
    /// Creation time.
    pub created_at: Timestamp,
}

/// A review as the audit reports it (no body).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewSummary {
    /// Review id.
    pub id: ReviewId,
    /// Reviewer.
    pub reviewer: Option<Login>,
    /// State.
    pub state: ReviewState,
    /// Submission time.
    pub submitted_at: Option<Timestamp>,
}

/// Pull-request details from the Pulls API (not available in the Issues API representation).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PullDetails {
    /// The pull-request id (distinct from the issue id).
    pub id: PullRequestId,
    /// Merged.
    pub merged: bool,
    /// Mergeable (`None` while GitHub computes it).
    pub mergeable: Option<bool>,
    /// Commits.
    pub commits: u64,
    /// Added lines.
    pub additions: u64,
    /// Deleted lines.
    pub deletions: u64,
    /// Changed files.
    pub changed_files: u64,
    /// Head branch.
    pub head: String,
    /// Base branch.
    pub base: String,
}

/// How a child collection ended in this run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChildCompletion {
    /// Every page was fetched.
    Complete,
    /// The child page budget was used while GitHub had more; continue with this link.
    Bounded {
        /// The next page of this child collection.
        resume: NextLink,
    },
    /// The run stopped in this collection (see [`Completion::Interrupted`]).
    Interrupted,
    /// Not fetched in this run (the run stopped before it).
    NotStarted,
}

/// A child collection of one issue or pull request, as collected by this run.
#[derive(Clone, PartialEq, Eq)]
pub struct Collected<T> {
    /// Items, in GitHub's order (from the restart page when this run resumed the collection).
    pub items: Vec<T>,
    /// Pages fetched by this run.
    pub pages: u64,
    /// How it ended.
    pub completion: ChildCompletion,
}

impl<T> fmt::Debug for Collected<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Collected")
            .field("items", &self.items.len())
            .field("pages", &self.pages)
            .field("completion", &self.completion)
            .finish()
    }
}

/// One audited item: the Issues API distinguishes issues from pull requests only by the
/// `pull_request` key; the audit keeps the two representations apart.
#[derive(Clone, PartialEq, Eq)]
pub enum ActivityEntry {
    /// A plain issue and its comments.
    Issue {
        /// Number.
        number: IssueNumber,
        /// Title.
        title: String,
        /// State.
        state: State,
        /// Label names.
        labels: Vec<String>,
        /// Comments.
        comments: Collected<CommentSummary>,
    },
    /// A pull request: details from the Pulls API and its reviews.
    PullRequest {
        /// Number (also its issue number).
        number: PullNumber,
        /// Title.
        title: String,
        /// State.
        state: State,
        /// Details; `None` if their request failed.
        details: Option<PullDetails>,
        /// Reviews.
        reviews: Collected<ReviewSummary>,
    },
}

impl ActivityEntry {
    /// The issue number of either shape.
    pub fn number(&self) -> IssueNumber {
        match self {
            Self::Issue { number, .. } => *number,
            Self::PullRequest { number, .. } => (*number).into(),
        }
    }
}

/// `Debug` shows numbers, states and counts; never titles or labels (record contents).
impl fmt::Debug for ActivityEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Issue {
                number,
                state,
                comments,
                ..
            } => f
                .debug_struct("Issue")
                .field("number", number)
                .field("state", state)
                .field("comments", comments)
                .finish_non_exhaustive(),
            Self::PullRequest {
                number,
                state,
                details,
                reviews,
                ..
            } => f
                .debug_struct("PullRequest")
                .field("number", number)
                .field("state", state)
                .field("details", &details.is_some())
                .field("reviews", reviews)
                .finish_non_exhaustive(),
        }
    }
}

/// Which child collection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChildKind {
    /// An issue's comments.
    Comments,
    /// A pull request's details (one request).
    PullDetails,
    /// A pull request's reviews.
    Reviews,
}

/// Where to restart a child collection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChildResume {
    /// The issue or pull-request number it belongs to.
    pub number: IssueNumber,
    /// Which collection.
    pub kind: ChildKind,
    /// Its restart page (`Start` for pull-request details).
    pub page: Resume<NextLink>,
}

/// Where to continue an audit: the parent item checkpoint and, after a child failure, the
/// child's restart page. Belongs to the repository and filters of the run that produced it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuditContinuation {
    /// The issue-listing position: a page and the items of it already reported.
    pub parent: ItemCheckpoint<NextLink>,
    /// The child collection to restart first, if the run stopped inside one.
    pub child: Option<ChildResume>,
}

/// What failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FailedAt {
    /// The repository request.
    Repository,
    /// An issue-listing page.
    IssuesPage {
        /// The page that failed.
        page: Option<Resume<NextLink>>,
    },
    /// A child collection page.
    Child {
        /// The issue or pull-request number.
        number: IssueNumber,
        /// Which collection.
        kind: ChildKind,
        /// The page that failed.
        page: Option<Resume<NextLink>>,
    },
}

/// Why an audit stopped early.
#[non_exhaustive]
#[derive(concord::Debug)]
pub enum AuditError {
    /// A GitHub request failed (Concord's error and response metadata, unchanged).
    GitHub(GitHubError),
    /// The replayed parent page no longer has the continuation's item at its position (the
    /// listing changed between runs).
    StaleContinuation {
        /// The number the continuation expected.
        expected: IssueNumber,
        /// The number found at that position.
        found: IssueNumber,
    },
}

impl AuditError {
    /// The GitHub error, if this is one.
    pub fn github(&self) -> Option<&GitHubError> {
        match self {
            Self::GitHub(error) => Some(error),
            Self::StaleContinuation { .. } => None,
        }
    }
}

impl fmt::Display for AuditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GitHub(error) => write!(f, "activity audit: {error}"),
            Self::StaleContinuation { expected, found } => write!(
                f,
                "activity audit: continuation expected issue {expected}, the listing now has {found}"
            ),
        }
    }
}

impl std::error::Error for AuditError {}

/// How the run ended.
// One value per audit run, moved once into the report: boxing the larger variants would only
// complicate matching on it.
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum Completion {
    /// The issue listing was exhausted and every item was audited (child collections may still
    /// be [`ChildCompletion::Bounded`]).
    Complete,
    /// The issue-page budget was used while GitHub had more.
    Bounded {
        /// Continue here.
        continuation: AuditContinuation,
    },
    /// A failure stopped the run.
    Interrupted {
        /// The failure (typed, not stringified).
        error: AuditError,
        /// What failed.
        failed: FailedAt,
        /// Continue here (refetches the failed page).
        continuation: AuditContinuation,
    },
}

impl Completion {
    /// The continuation, unless the audit completed.
    pub fn continuation(&self) -> Option<&AuditContinuation> {
        match self {
            Self::Complete => None,
            Self::Bounded { continuation } | Self::Interrupted { continuation, .. } => {
                Some(continuation)
            }
        }
    }
}

/// The consistency an audit can claim.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Consistency {
    /// A live listing walked page by page: items may move between pages while it runs. Not a
    /// snapshot.
    LiveBestEffort,
}

/// The audit's result.
#[derive(Debug)]
pub struct ActivityReport {
    /// The repository audited.
    pub repo: RepoRef,
    /// The repository state; `None` if its request failed.
    pub repository: Option<RepositoryState>,
    /// Audited items, in listing order.
    pub entries: Vec<ActivityEntry>,
    /// Every request sent, with request ids and rate-limit headers.
    pub requests: Vec<RequestRecord>,
    /// How the run ended.
    pub completion: Completion,
    /// Always [`Consistency::LiveBestEffort`].
    pub consistency: Consistency,
}

impl ActivityReport {
    /// Issues (not pull requests) audited by this run.
    pub fn issues(&self) -> impl Iterator<Item = &ActivityEntry> {
        self.entries
            .iter()
            .filter(|e| matches!(e, ActivityEntry::Issue { .. }))
    }

    /// Pull requests audited by this run.
    pub fn pull_requests(&self) -> impl Iterator<Item = &ActivityEntry> {
        self.entries
            .iter()
            .filter(|e| matches!(e, ActivityEntry::PullRequest { .. }))
    }
}

/// A child failure: the partial entry, what failed and where to restart.
struct ChildFailure {
    entry: ActivityEntry,
    error: GitHubError,
    kind: ChildKind,
    failed: Option<Resume<NextLink>>,
    resume: Resume<NextLink>,
}

/// Runs the audit. Never panics on provider data; every failure ends up in
/// [`ActivityReport::completion`].
pub async fn activity_audit(repo: &RepoContext, input: AuditInput) -> ActivityReport {
    let log = RequestLog::default();
    let mut report = ActivityReport {
        repo: repo.repo(),
        repository: None,
        entries: Vec::new(),
        requests: Vec::new(),
        completion: Completion::Complete,
        consistency: Consistency::LiveBestEffort,
    };
    let (start, mut pending_child) = match input.from {
        None => (
            ItemCheckpoint {
                page: Resume::Start,
                skip: 0,
            },
            None,
        ),
        Some(continuation) => (continuation.parent, continuation.child),
    };
    let unchanged = AuditContinuation {
        parent: start.clone(),
        child: pending_child.clone(),
    };

    // 1. The repository, conditionally when an ETag is known.
    match repository(repo, input.repository_etag, &log).await {
        Ok(state) => report.repository = Some(state),
        Err(error) => {
            report.requests = log.take();
            report.completion = Completion::Interrupted {
                error: AuditError::GitHub(error),
                failed: FailedAt::Repository,
                continuation: unchanged,
            };
            return report;
        }
    }

    // 2. The parent collection, item by item.
    let pages = repo.issue_pages(repo.issues(&input.filter));
    let parent_log = log.clone();
    let budget = Budget::new(input.budget.issue_pages);
    let mut items = resume_items(start, budget, |key| {
        let page = pages.fetch(key);
        let log = parent_log.clone();
        async move { record_page(operation::ISSUES, page.await, &log) }
    });
    let child_budget = Budget::new(input.budget.child_pages);
    loop {
        // The checkpoint *before* this item: a child failure restarts here, re-emitting it.
        let before = items.checkpoint().cloned();
        let Some(next) = items.next().await else {
            break;
        };
        let item = match next {
            Ok(item) => item,
            Err(error) => {
                let parent = items.finish();
                report.requests = log.take();
                report.completion = Completion::Interrupted {
                    error: AuditError::GitHub(error),
                    failed: FailedAt::IssuesPage {
                        page: parent.failed,
                    },
                    continuation: AuditContinuation {
                        parent: parent
                            .resume
                            .expect("a failed chain keeps its failed page as resume point"),
                        child: None,
                    },
                };
                return report;
            }
        };
        let before = before.expect("an item was emitted, so a checkpoint existed before it");
        let child_from = match pending_child.take() {
            None => None,
            Some(child) if child.number == item.number() => Some(child),
            Some(child) => {
                report.requests = log.take();
                report.completion = Completion::Interrupted {
                    error: AuditError::StaleContinuation {
                        expected: child.number,
                        found: item.number(),
                    },
                    failed: FailedAt::IssuesPage {
                        page: Some(before.page.clone()),
                    },
                    continuation: AuditContinuation {
                        parent: before,
                        child: Some(child),
                    },
                };
                return report;
            }
        };
        match audit_item(repo, item, child_from, child_budget, &log).await {
            Ok(entry) => report.entries.push(entry),
            Err(failure) => {
                let number = failure.entry.number();
                report.entries.push(failure.entry);
                report.requests = log.take();
                report.completion = Completion::Interrupted {
                    error: AuditError::GitHub(failure.error),
                    failed: FailedAt::Child {
                        number,
                        kind: failure.kind,
                        page: failure.failed,
                    },
                    continuation: AuditContinuation {
                        parent: before,
                        child: Some(ChildResume {
                            number,
                            kind: failure.kind,
                            page: failure.resume,
                        }),
                    },
                };
                return report;
            }
        }
    }
    let parent = items.finish();
    report.requests = log.take();
    report.completion = match (parent.outcome, parent.resume) {
        (Outcome::Exhausted, _) | (_, None) => Completion::Complete,
        (_, Some(parent)) => Completion::Bounded {
            continuation: AuditContinuation {
                parent,
                child: None,
            },
        },
    };
    report
}

async fn repository(
    repo: &RepoContext,
    etag: Option<ETag>,
    log: &RequestLog,
) -> Result<RepositoryState, GitHubError> {
    let prepared = repo
        .handle()
        .get()
        .maybe_if_none_match(etag.clone())
        .prepare()?;
    match send_conditional(prepared).await {
        Ok(Conditional::NotModified { meta }) => {
            log.meta("repos/get", &meta);
            let etag = meta
                .etag()
                .cloned()
                .or(etag)
                .expect("a 304 answers an ETag");
            Ok(RepositoryState::NotModified { etag })
        }
        Ok(Conditional::Modified(with)) => {
            log.meta("repos/get", &with.meta);
            let repository = with.value;
            Ok(RepositoryState::Fetched {
                summary: RepositorySummary {
                    full_name: repository.full_name,
                    default_branch: repository.default_branch,
                    open_issues_count: repository.open_issues_count,
                    archived: repository.archived,
                },
                etag: with.meta.etag().cloned(),
            })
        }
        Err(error) => {
            log.error("repos/get", &error);
            Err(error)
        }
    }
}

/// Audits one item: comments for an issue; details and reviews for a pull request.
async fn audit_item(
    repo: &RepoContext,
    item: IssueItem,
    child_from: Option<ChildResume>,
    budget: Budget,
    log: &RequestLog,
) -> Result<ActivityEntry, Box<ChildFailure>> {
    let pull_number = item.pull_number();
    let issue = match item {
        IssueItem::Issue(issue) | IssueItem::PullRequest { issue, .. } => issue,
    };
    let labels = issue
        .labels
        .iter()
        .filter_map(|l| l.name().map(str::to_owned))
        .collect();
    let Some(number) = pull_number else {
        let from = child_from.map_or(Resume::Start, |c| c.page);
        let pages = repo.issue_comment_pages(issue.number, |op| op);
        let (comments, failure) = collect(from, budget, log, operation::ISSUE_COMMENTS, |key| {
            pages.fetch(key)
        })
        .await;
        let comments = comments.map_items(|c: IssueComment| CommentSummary {
            id: c.id,
            author: c.user.map(|u| u.login),
            created_at: c.created_at,
        });
        let entry = ActivityEntry::Issue {
            number: issue.number,
            title: issue.title,
            state: issue.state,
            labels,
            comments,
        };
        return match failure {
            None => Ok(entry),
            Some((error, failed, resume)) => Err(Box::new(ChildFailure {
                entry,
                error,
                kind: ChildKind::Comments,
                failed,
                resume,
            })),
        };
    };

    // A pull request: details (always refetched, also on resume), then reviews.
    let mut entry = ActivityEntry::PullRequest {
        number,
        title: issue.title,
        state: issue.state,
        details: None,
        reviews: Collected {
            items: Vec::new(),
            pages: 0,
            completion: ChildCompletion::NotStarted,
        },
    };
    let details = match repo.handle().pull(number).prepare() {
        Err(error) => Err(GitHubError::from(error)),
        Ok(prepared) => send_with_meta(prepared).await,
    };
    let details = match details {
        Ok(with) => {
            log.meta("pulls/get", &with.meta);
            let pull = with.value;
            PullDetails {
                id: pull.summary.id,
                merged: pull.merged,
                mergeable: pull.mergeable,
                commits: pull.commits,
                additions: pull.additions,
                deletions: pull.deletions,
                changed_files: pull.changed_files,
                head: pull.summary.head.branch,
                base: pull.summary.base.branch,
            }
        }
        Err(error) => {
            log.error("pulls/get", &error);
            return Err(Box::new(ChildFailure {
                entry,
                error,
                kind: ChildKind::PullDetails,
                failed: Some(Resume::Start),
                resume: Resume::Start,
            }));
        }
    };
    let from = match child_from {
        Some(ChildResume {
            kind: ChildKind::Reviews,
            page,
            ..
        }) => page,
        _ => Resume::Start,
    };
    let pages = repo.pull_review_pages(number, |op| op);
    let (reviews, failure) = collect(from, budget, log, operation::PULL_REVIEWS, |key| {
        pages.fetch(key)
    })
    .await;
    let reviews = reviews.map_items(|r: Review| ReviewSummary {
        id: r.id,
        reviewer: r.user.map(|u| u.login),
        state: r.state,
        submitted_at: r.submitted_at,
    });
    if let ActivityEntry::PullRequest {
        details: slot,
        reviews: reviews_slot,
        ..
    } = &mut entry
    {
        *slot = Some(details);
        *reviews_slot = reviews;
    }
    match failure {
        None => Ok(entry),
        Some((error, failed, resume)) => Err(Box::new(ChildFailure {
            entry,
            error,
            kind: ChildKind::Reviews,
            failed,
            resume,
        })),
    }
}

type ChildError = (GitHubError, Option<Resume<NextLink>>, Resume<NextLink>);

/// Walks one child collection with a sequential chain, collecting its items and recording each
/// page's request.
async fn collect<T, F, Fut>(
    from: Resume<NextLink>,
    budget: Budget,
    log: &RequestLog,
    operation: &'static str,
    mut fetch: F,
) -> (Collected<T>, Option<ChildError>)
where
    F: FnMut(Option<NextLink>) -> Fut,
    Fut: Future<Output = Result<LinkPage<T>, GitHubError>>,
{
    let mut chain = sequential(from, budget, |key| {
        let page = fetch(key);
        async move { record_page(operation, page.await, log) }
    });
    let mut items = Vec::new();
    let mut error = None;
    while let Some(page) = chain.next().await {
        match page {
            Ok(page) => items.extend(page.items),
            Err(e) => error = Some(e),
        }
    }
    let report = chain.finish();
    let pages = report.progress.succeeded;
    match error {
        Some(error) => (
            Collected {
                items,
                pages,
                completion: ChildCompletion::Interrupted,
            },
            Some((
                error,
                report.failed,
                report.resume.expect("a failed chain keeps its failed page"),
            )),
        ),
        None => {
            let completion = match (report.outcome, report.resume) {
                (Outcome::Bounded, Some(Resume::At(resume))) => ChildCompletion::Bounded { resume },
                _ => ChildCompletion::Complete,
            };
            (
                Collected {
                    items,
                    pages,
                    completion,
                },
                None,
            )
        }
    }
}

impl<T> Collected<T> {
    fn map_items<U>(self, f: impl FnMut(T) -> U) -> Collected<U> {
        Collected {
            items: self.items.into_iter().map(f).collect(),
            pages: self.pages,
            completion: self.completion,
        }
    }
}
