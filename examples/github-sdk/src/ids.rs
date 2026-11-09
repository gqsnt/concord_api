//! Typed resource identifiers.
//!
//! GitHub uses several unrelated integer spaces that are easy to confuse: an issue's `id` is not
//! its `number`, and a pull request listed by the Issues API carries its **issue** id, not its
//! pull-request id (the Issues API documentation says so explicitly). Each space has its own
//! newtype so that the compiler refuses `pull(issue.id)`. Numbers are formatted with `Display`
//! when they are placed in a path; Concord percent-encodes each value as one segment.

use std::fmt;

use serde::{Deserialize, Serialize};

macro_rules! numeric_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[derive(concord::WireDisplay)]
        #[serde(transparent)]
        pub struct $name(pub u64);

        impl $name {
            /// The provider's integer.
            pub fn get(self) -> u64 {
                self.0
            }
        }
    };
}

numeric_id!(
    /// A repository's numeric `id` (stable across renames).
    RepositoryId
);
numeric_id!(
    /// An issue's numeric `id` (also the `id` the Issues API reports for a pull request).
    IssueId
);
numeric_id!(
    /// An issue's per-repository `number`, the key of `GET /repos/{owner}/{repo}/issues/{number}`.
    IssueNumber
);
numeric_id!(
    /// A pull request's numeric `id`, reported only by the Pulls API.
    PullRequestId
);
numeric_id!(
    /// A pull request's per-repository `number`, the key of `GET .../pulls/{number}`. It is the
    /// same number as the pull request's issue number (see [`IssueNumber`]), which is why
    /// [`crate::models::IssueItem::PullRequest`] can convert one into the other.
    PullNumber
);
numeric_id!(
    /// An issue comment's `id`.
    CommentId
);
numeric_id!(
    /// A pull request review's `id`.
    ReviewId
);
numeric_id!(
    /// A user's numeric `id`.
    UserId
);
numeric_id!(
    /// A label's numeric `id`.
    LabelId
);
numeric_id!(
    /// A milestone's per-repository `number`.
    MilestoneNumber
);
numeric_id!(
    /// A workflow's numeric `id`.
    WorkflowId
);
numeric_id!(
    /// A workflow run's `id`.
    RunId
);
numeric_id!(
    /// A workflow job's `id`.
    JobId
);
numeric_id!(
    /// A workflow artifact's `id`.
    ArtifactId
);
numeric_id!(
    /// A check suite's `id` (a workflow-run filter).
    CheckSuiteId
);

impl From<PullNumber> for IssueNumber {
    fn from(number: PullNumber) -> Self {
        Self(number.0)
    }
}

impl From<IssueNumber> for PullNumber {
    /// Only meaningful for an issue that is a pull request (check
    /// [`crate::models::IssueItem::PullRequest`]); for a plain issue the Pulls API answers 404.
    fn from(number: IssueNumber) -> Self {
        Self(number.0)
    }
}

macro_rules! text_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[derive(concord::WireDisplay)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// Wraps the provider's text as given. Concord sends it as one percent-encoded path
            /// segment; an empty, `.` or `..` value is a build error when a request is prepared.
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            /// The text.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self::new(value)
            }
        }
    };
}

text_id!(
    /// A repository owner's login (a user or an organization), not case sensitive at GitHub.
    Owner
);
text_id!(
    /// A repository name, without the owner.
    RepoName
);
text_id!(
    /// A user's login.
    Login
);
text_id!(
    /// A commit SHA as GitHub reports it (40 hexadecimal characters for SHA-1 repositories).
    CommitSha
);

/// A repository: owner and name. The scope of every operation in this SDK.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RepoRef {
    /// The owner's login.
    pub owner: Owner,
    /// The repository name.
    pub name: RepoName,
}

impl RepoRef {
    /// `owner/name`.
    pub fn new(owner: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            owner: Owner::new(owner),
            name: RepoName::new(name),
        }
    }
}

impl fmt::Display for RepoRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.owner, self.name)
    }
}

/// The `workflow_id` path value of `GET .../actions/workflows/{workflow_id}/runs`: GitHub accepts
/// the numeric id or the workflow file name (`main.yaml`).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, concord::WireDisplay)]
#[serde(untagged)]
pub enum WorkflowRef {
    /// The workflow's numeric id.
    Id(WorkflowId),
    /// The workflow file name, such as `ci.yml` (sent as one segment).
    File(String),
}
