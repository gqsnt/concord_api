//! Typed response models for the pinned API version (`2026-03-10`).
//!
//! Each model keeps the fields this slice uses, typed after the OpenAPI description pinned in
//! `PROVIDER.md` (nullable fields are `Option`, optional fields have `#[serde(default)]`); every
//! other field GitHub sends is ignored by serde. Fields with a documented enumeration are enums
//! with an `Unknown(String)` variant that keeps a value added later by GitHub instead of failing
//! the whole page. Timestamps are kept as the text GitHub sent ([`Timestamp`]).

use std::fmt;

use serde::{Deserialize, Deserializer};

use crate::ids::{
    ArtifactId, CommentId, CommitSha, IssueId, IssueNumber, JobId, LabelId, Login, MilestoneNumber,
    PullNumber, PullRequestId, RepositoryId, ReviewId, RunId, UserId, WorkflowId,
};
use crate::params::Timestamp;

/// Declares an enum of a documented string enumeration that keeps unknown values.
macro_rules! provider_enum {
    ($(#[$doc:meta])* $name:ident { $($(#[$vdoc:meta])* $variant:ident = $wire:literal),+ $(,)? }) => {
        $(#[$doc])*
        #[derive(Clone, Debug, PartialEq, Eq, Hash)]
        pub enum $name {
            $($(#[$vdoc])* $variant,)+
            /// A value outside the enumeration documented for the pinned version, kept as sent.
            Unknown(String),
        }

        impl $name {
            /// The wire text.
            pub fn as_str(&self) -> &str {
                match self {
                    $(Self::$variant => $wire,)+
                    Self::Unknown(other) => other,
                }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let text = String::deserialize(deserializer)?;
                Ok(match text.as_str() {
                    $($wire => Self::$variant,)+
                    _ => Self::Unknown(text),
                })
            }
        }
    };
}

provider_enum!(
    /// `state` of an issue or a pull request.
    State {
        /// `open`
        Open = "open",
        /// `closed`
        Closed = "closed",
    }
);

provider_enum!(
    /// `state_reason` of an issue.
    StateReason {
        /// `completed`
        Completed = "completed",
        /// `reopened`
        Reopened = "reopened",
        /// `not_planned`
        NotPlanned = "not_planned",
        /// `duplicate`
        Duplicate = "duplicate",
    }
);

provider_enum!(
    /// `author_association`: how the author is associated with the repository.
    AuthorAssociation {
        /// `COLLABORATOR`
        Collaborator = "COLLABORATOR",
        /// `CONTRIBUTOR`
        Contributor = "CONTRIBUTOR",
        /// `FIRST_TIMER`
        FirstTimer = "FIRST_TIMER",
        /// `FIRST_TIME_CONTRIBUTOR`
        FirstTimeContributor = "FIRST_TIME_CONTRIBUTOR",
        /// `MANNEQUIN`
        Mannequin = "MANNEQUIN",
        /// `MEMBER`
        Member = "MEMBER",
        /// `NONE`
        None = "NONE",
        /// `OWNER`
        Owner = "OWNER",
    }
);

provider_enum!(
    /// `state` of a pull request review. The pinned description gives no enumeration (only the
    /// example `CHANGES_REQUESTED`); these are the states GitHub's review documentation names.
    ReviewState {
        /// `APPROVED`
        Approved = "APPROVED",
        /// `CHANGES_REQUESTED`
        ChangesRequested = "CHANGES_REQUESTED",
        /// `COMMENTED`
        Commented = "COMMENTED",
        /// `DISMISSED`
        Dismissed = "DISMISSED",
        /// `PENDING` (not submitted; no `submitted_at`)
        Pending = "PENDING",
    }
);

provider_enum!(
    /// `status` of a workflow run or job.
    RunState {
        /// `queued`
        Queued = "queued",
        /// `in_progress`
        InProgress = "in_progress",
        /// `completed`
        Completed = "completed",
        /// `waiting`
        Waiting = "waiting",
        /// `requested`
        Requested = "requested",
        /// `pending`
        Pending = "pending",
    }
);

provider_enum!(
    /// `conclusion` of a completed workflow run, job or step.
    Conclusion {
        /// `success`
        Success = "success",
        /// `failure`
        Failure = "failure",
        /// `neutral`
        Neutral = "neutral",
        /// `cancelled`
        Cancelled = "cancelled",
        /// `skipped`
        Skipped = "skipped",
        /// `timed_out`
        TimedOut = "timed_out",
        /// `action_required`
        ActionRequired = "action_required",
        /// `stale` (runs only)
        Stale = "stale",
    }
);

provider_enum!(
    /// `status` of a file in a pull request.
    FileStatus {
        /// `added`
        Added = "added",
        /// `removed`
        Removed = "removed",
        /// `modified`
        Modified = "modified",
        /// `renamed`
        Renamed = "renamed",
        /// `copied`
        Copied = "copied",
        /// `changed`
        Changed = "changed",
        /// `unchanged`
        Unchanged = "unchanged",
    }
);

/// A user as GitHub embeds it (`simple-user`): the fields this slice uses.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct User {
    /// Login.
    pub login: Login,
    /// Numeric id.
    pub id: UserId,
    /// `User`, `Organization` or `Bot`.
    #[serde(rename = "type")]
    pub account_type: String,
}

// ---------------------------------------------------------------------------------------------
// Repository
// ---------------------------------------------------------------------------------------------

/// `GET /repos/{owner}/{repo}` (`full-repository`).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Repository {
    /// Numeric id (stable across renames and transfers).
    pub id: RepositoryId,
    /// Name without the owner.
    pub name: String,
    /// `owner/name`.
    pub full_name: String,
    /// The owner.
    pub owner: User,
    /// Whether the repository is private.
    pub private: bool,
    /// Web page.
    pub html_url: String,
    /// Description.
    pub description: Option<String>,
    /// Whether it is a fork.
    pub fork: bool,
    /// The default branch.
    pub default_branch: String,
    /// `public`, `private` or `internal`.
    #[serde(default)]
    pub visibility: Option<String>,
    /// Archived (read-only).
    pub archived: bool,
    /// Disabled.
    pub disabled: bool,
    /// Open issues **and pull requests**, as GitHub counts them.
    pub open_issues_count: u64,
    /// Whether issues are enabled.
    pub has_issues: bool,
    /// Topics.
    #[serde(default)]
    pub topics: Vec<String>,
    /// Creation time.
    pub created_at: Option<Timestamp>,
    /// Last update of the repository object.
    pub updated_at: Option<Timestamp>,
    /// Last push.
    pub pushed_at: Option<Timestamp>,
}

/// One label of `GET /repos/{owner}/{repo}/labels`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Label {
    /// Numeric id.
    pub id: LabelId,
    /// Name.
    pub name: String,
    /// Description.
    pub description: Option<String>,
    /// Hexadecimal color without `#`.
    pub color: String,
    /// Whether it is one of the repository's default labels.
    pub default: bool,
}

/// A label embedded in an issue: GitHub documents each entry as either a bare name or an object
/// whose fields are all optional.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum IssueLabel {
    /// A bare name.
    Name(String),
    /// A label object.
    Detailed {
        /// Numeric id.
        #[serde(default)]
        id: Option<LabelId>,
        /// Name.
        #[serde(default)]
        name: Option<String>,
        /// Color.
        #[serde(default)]
        color: Option<String>,
    },
}

impl IssueLabel {
    /// The label's name, if GitHub sent one.
    pub fn name(&self) -> Option<&str> {
        match self {
            Self::Name(name) => Some(name),
            Self::Detailed { name, .. } => name.as_deref(),
        }
    }
}

/// A milestone embedded in an issue or pull request.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct MilestoneRef {
    /// Per-repository number.
    pub number: MilestoneNumber,
    /// Title.
    pub title: String,
    /// `open` or `closed`.
    pub state: State,
}

// ---------------------------------------------------------------------------------------------
// Issues
// ---------------------------------------------------------------------------------------------

/// The fields an issue has in the Issues API, whether or not it is a pull request.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Issue {
    /// Issue id (for a pull request too: **not** its pull-request id).
    pub id: IssueId,
    /// Per-repository number.
    pub number: IssueNumber,
    /// Title.
    pub title: String,
    /// Body (Markdown), if any.
    pub body: Option<String>,
    /// `open` or `closed`.
    pub state: State,
    /// Why it was closed or reopened.
    #[serde(default)]
    pub state_reason: Option<StateReason>,
    /// Author (`null` for a deleted account).
    pub user: Option<User>,
    /// Labels.
    pub labels: Vec<IssueLabel>,
    /// Assignees (`2026-03-10` removed the singular `assignee`).
    #[serde(default)]
    pub assignees: Vec<User>,
    /// Milestone.
    pub milestone: Option<MilestoneRef>,
    /// Whether the conversation is locked.
    pub locked: bool,
    /// Number of comments.
    pub comments: u64,
    /// Web page.
    pub html_url: String,
    /// Creation time.
    pub created_at: Timestamp,
    /// Last update.
    pub updated_at: Timestamp,
    /// Closing time.
    pub closed_at: Option<Timestamp>,
    /// The author's association with the repository.
    #[serde(default)]
    pub author_association: Option<AuthorAssociation>,
}

/// The `pull_request` object the Issues API adds to an issue that is a pull request.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct PullRequestLink {
    /// API URL of the pull request (use [`IssueItem::pull_number`] rather than parsing it).
    pub url: String,
    /// Web page.
    pub html_url: String,
    /// Diff URL.
    pub diff_url: String,
    /// Patch URL.
    pub patch_url: String,
    /// When it was merged, if it was.
    #[serde(default)]
    pub merged_at: Option<Timestamp>,
}

/// One entry of `GET .../issues` or the result of `GET .../issues/{number}`: GitHub's Issues API
/// returns pull requests too, recognizable only by the `pull_request` key. The two shapes are
/// kept distinct here so that a caller cannot mistake one for the other.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(from = "IssueRecord")]
pub enum IssueItem {
    /// A plain issue.
    Issue(Issue),
    /// A pull request, as the Issues API represents it. Its `issue.id` is an issue id; the
    /// pull-request details come from the Pulls API ([`IssueItem::pull_number`]).
    PullRequest {
        /// The issue fields.
        issue: Issue,
        /// The `pull_request` object.
        pull_request: PullRequestLink,
    },
}

impl IssueItem {
    /// The issue fields, for either shape.
    pub fn issue(&self) -> &Issue {
        match self {
            Self::Issue(issue) | Self::PullRequest { issue, .. } => issue,
        }
    }

    /// The number, for either shape.
    pub fn number(&self) -> IssueNumber {
        self.issue().number
    }

    /// The key for the Pulls API when this is a pull request.
    pub fn pull_number(&self) -> Option<PullNumber> {
        match self {
            Self::Issue(_) => None,
            Self::PullRequest { issue, .. } => Some(issue.number.into()),
        }
    }

    /// Whether this is a pull request.
    pub fn is_pull_request(&self) -> bool {
        matches!(self, Self::PullRequest { .. })
    }
}

#[derive(Deserialize)]
struct IssueRecord {
    #[serde(flatten)]
    issue: Issue,
    #[serde(default)]
    pull_request: Option<PullRequestLink>,
}

impl From<IssueRecord> for IssueItem {
    fn from(record: IssueRecord) -> Self {
        match record.pull_request {
            None => Self::Issue(record.issue),
            Some(pull_request) => Self::PullRequest {
                issue: record.issue,
                pull_request,
            },
        }
    }
}

/// An issue comment.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct IssueComment {
    /// Comment id.
    pub id: CommentId,
    /// Body (Markdown); returned by default.
    #[serde(default)]
    pub body: Option<String>,
    /// Author.
    pub user: Option<User>,
    /// Web page.
    pub html_url: String,
    /// API URL of the issue the comment belongs to.
    pub issue_url: String,
    /// Creation time.
    pub created_at: Timestamp,
    /// Last update.
    pub updated_at: Timestamp,
    /// The author's association with the repository.
    #[serde(default)]
    pub author_association: Option<AuthorAssociation>,
}

// ---------------------------------------------------------------------------------------------
// Pull requests
// ---------------------------------------------------------------------------------------------

/// A repository as a pull request's `head`/`base` embeds it.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct RepoSummary {
    /// Numeric id.
    pub id: RepositoryId,
    /// Name.
    pub name: String,
    /// `owner/name`.
    pub full_name: String,
}

/// A pull request's `head` or `base`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct BranchRef {
    /// `owner:branch`.
    pub label: String,
    /// The branch name.
    #[serde(rename = "ref")]
    pub branch: String,
    /// The commit.
    pub sha: CommitSha,
    /// The repository (`null` when a fork was deleted).
    pub repo: Option<RepoSummary>,
}

/// One entry of `GET .../pulls` (`pull-request-simple`).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct PullRequestSummary {
    /// Pull-request id (not the issue id).
    pub id: PullRequestId,
    /// Per-repository number.
    pub number: PullNumber,
    /// `open` or `closed`.
    pub state: State,
    /// Whether the conversation is locked.
    pub locked: bool,
    /// Title.
    pub title: String,
    /// Body.
    pub body: Option<String>,
    /// Author.
    pub user: Option<User>,
    /// Draft.
    #[serde(default)]
    pub draft: Option<bool>,
    /// Head branch.
    pub head: BranchRef,
    /// Base branch.
    pub base: BranchRef,
    /// Web page.
    pub html_url: String,
    /// Creation time.
    pub created_at: Timestamp,
    /// Last update.
    pub updated_at: Timestamp,
    /// Closing time.
    pub closed_at: Option<Timestamp>,
    /// Merge time.
    pub merged_at: Option<Timestamp>,
    /// The author's association with the repository.
    pub author_association: AuthorAssociation,
}

/// `GET .../pulls/{number}` (`pull-request`): the summary fields plus those GitHub only returns
/// for a single pull request.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct PullRequest {
    /// The fields shared with the listing.
    #[serde(flatten)]
    pub summary: PullRequestSummary,
    /// Whether it was merged.
    pub merged: bool,
    /// Whether it can be merged (`null` while GitHub computes it).
    pub mergeable: Option<bool>,
    /// Who merged it.
    pub merged_by: Option<User>,
    /// Issue comments.
    pub comments: u64,
    /// Review comments.
    pub review_comments: u64,
    /// Commits.
    pub commits: u64,
    /// Added lines.
    pub additions: u64,
    /// Deleted lines.
    pub deletions: u64,
    /// Changed files.
    pub changed_files: u64,
}

/// One review of `GET .../pulls/{number}/reviews`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Review {
    /// Review id.
    pub id: ReviewId,
    /// Reviewer.
    pub user: Option<User>,
    /// Body (may be empty).
    pub body: String,
    /// State.
    pub state: ReviewState,
    /// Web page.
    pub html_url: String,
    /// Submission time; absent for a pending review.
    #[serde(default)]
    pub submitted_at: Option<Timestamp>,
    /// The commit reviewed.
    pub commit_id: Option<CommitSha>,
    /// The reviewer's association with the repository.
    pub author_association: AuthorAssociation,
}

/// One file of `GET .../pulls/{number}/files`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct PullRequestFile {
    /// Path.
    pub filename: String,
    /// Change status.
    pub status: FileStatus,
    /// Added lines.
    pub additions: u64,
    /// Deleted lines.
    pub deletions: u64,
    /// Changed lines.
    pub changes: u64,
    /// The previous path of a renamed file.
    #[serde(default)]
    pub previous_filename: Option<String>,
}

// ---------------------------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------------------------

/// A workflow of `GET .../actions/workflows`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Workflow {
    /// Numeric id.
    pub id: WorkflowId,
    /// Name.
    pub name: String,
    /// File path, such as `.github/workflows/ci.yml`.
    pub path: String,
    /// `active`, `deleted`, `disabled_fork`, `disabled_inactivity` or `disabled_manually`.
    pub state: String,
}

/// `{ total_count, workflows }`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct WorkflowList {
    /// Total over all pages.
    pub total_count: u64,
    /// This page's workflows.
    pub workflows: Vec<Workflow>,
}

/// A workflow run.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct WorkflowRun {
    /// Run id.
    pub id: RunId,
    /// Name.
    pub name: Option<String>,
    /// Head branch.
    pub head_branch: Option<String>,
    /// Head commit.
    pub head_sha: CommitSha,
    /// Workflow file path.
    pub path: String,
    /// Run number within the workflow.
    pub run_number: u64,
    /// Attempt number.
    #[serde(default)]
    pub run_attempt: Option<u64>,
    /// Triggering event (`push`, `pull_request`, ...).
    pub event: String,
    /// Status.
    pub status: Option<RunState>,
    /// Conclusion (once completed).
    pub conclusion: Option<Conclusion>,
    /// The workflow.
    pub workflow_id: WorkflowId,
    /// Web page.
    pub html_url: String,
    /// Title shown in the UI.
    pub display_title: String,
    /// Creation time.
    pub created_at: Timestamp,
    /// Last update.
    pub updated_at: Timestamp,
    /// Start of this attempt.
    #[serde(default)]
    pub run_started_at: Option<Timestamp>,
    /// Who triggered it.
    #[serde(default)]
    pub actor: Option<User>,
}

/// `{ total_count, workflow_runs }`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct WorkflowRunList {
    /// Total over all pages (GitHub caps filtered searches at 1,000 results).
    pub total_count: u64,
    /// This page's runs.
    pub workflow_runs: Vec<WorkflowRun>,
}

/// A step of a job.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Step {
    /// Name.
    pub name: String,
    /// Status.
    pub status: RunState,
    /// Conclusion.
    pub conclusion: Option<Conclusion>,
    /// Position.
    pub number: u64,
    /// Start.
    #[serde(default)]
    pub started_at: Option<Timestamp>,
    /// End.
    #[serde(default)]
    pub completed_at: Option<Timestamp>,
}

/// A job of a workflow run.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Job {
    /// Job id.
    pub id: JobId,
    /// The run.
    pub run_id: RunId,
    /// Attempt.
    #[serde(default)]
    pub run_attempt: Option<u64>,
    /// Head commit.
    pub head_sha: CommitSha,
    /// Web page.
    pub html_url: Option<String>,
    /// Status.
    pub status: RunState,
    /// Conclusion.
    pub conclusion: Option<Conclusion>,
    /// Creation time. Required by the pinned schema, but absent from its own `job` examples:
    /// optional here (see `PROVIDER.md`, discrepancies).
    #[serde(default)]
    pub created_at: Option<Timestamp>,
    /// Start.
    pub started_at: Timestamp,
    /// End.
    pub completed_at: Option<Timestamp>,
    /// Name.
    pub name: String,
    /// Steps.
    #[serde(default)]
    pub steps: Vec<Step>,
    /// Runner labels.
    pub labels: Vec<String>,
    /// Runner name.
    pub runner_name: Option<String>,
    /// Workflow name.
    pub workflow_name: Option<String>,
    /// Head branch.
    pub head_branch: Option<String>,
}

/// `{ total_count, jobs }`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct JobList {
    /// Total over all pages.
    pub total_count: u64,
    /// This page's jobs.
    pub jobs: Vec<Job>,
}

/// The run an artifact belongs to, as embedded in the artifact (every field optional in the
/// pinned description).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct ArtifactRun {
    /// Run id.
    #[serde(default)]
    pub id: Option<RunId>,
    /// Repository id.
    #[serde(default)]
    pub repository_id: Option<RepositoryId>,
    /// Head branch.
    #[serde(default)]
    pub head_branch: Option<String>,
    /// Head commit.
    #[serde(default)]
    pub head_sha: Option<CommitSha>,
}

/// A workflow artifact.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Artifact {
    /// Artifact id.
    pub id: ArtifactId,
    /// Name.
    pub name: String,
    /// Size of the ZIP archive.
    pub size_in_bytes: u64,
    /// API URL of the download endpoint (the SDK uses the declared endpoint, not this text).
    pub archive_download_url: String,
    /// Whether it expired (its bytes are gone).
    pub expired: bool,
    /// Creation time.
    pub created_at: Option<Timestamp>,
    /// Expiry time.
    pub expires_at: Option<Timestamp>,
    /// Last update.
    pub updated_at: Option<Timestamp>,
    /// Content digest, such as `sha256:...`, when GitHub reports one.
    #[serde(default)]
    pub digest: Option<String>,
    /// The producing run.
    #[serde(default)]
    pub workflow_run: Option<ArtifactRun>,
}

/// `{ total_count, artifacts }`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct ArtifactList {
    /// Total over all pages.
    pub total_count: u64,
    /// This page's artifacts.
    pub artifacts: Vec<Artifact>,
}
