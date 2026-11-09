//! Typed responses: the issue and project representations of the REST API v4, reduced to the
//! fields this slice uses. Unknown fields are ignored; a field GitLab documents as nullable is an
//! `Option`; a field the SDK relies on (identifiers, title, state, URLs) is required, so a response
//! without it is a decode error rather than a default.

use serde::{Deserialize, Deserializer, Serialize};

use crate::params::{IssueId, IssueIid, ProjectId};

/// An issue, as returned by `GET /projects/:id/issues` and `GET /projects/:id/issues/:issue_iid`.
#[derive(Clone, Debug, Deserialize)]
#[non_exhaustive]
pub struct Issue {
    /// The global ID.
    pub id: IssueId,
    /// The project-scoped number used in URLs (`#iid`).
    pub iid: IssueIid,
    /// The project that contains the issue.
    pub project_id: ProjectId,
    /// Title.
    pub title: String,
    /// Description (Markdown); `null` when empty.
    #[serde(default)]
    pub description: Option<String>,
    /// `opened` or `closed`.
    pub state: IssueStatus,
    /// Label names.
    #[serde(default)]
    pub labels: Vec<String>,
    /// The author.
    pub author: User,
    /// Assignees.
    #[serde(default)]
    pub assignees: Vec<User>,
    /// The milestone, if any.
    #[serde(default)]
    pub milestone: Option<Milestone>,
    /// Creation time (ISO 8601, as sent).
    pub created_at: String,
    /// Last update time (ISO 8601, as sent).
    pub updated_at: String,
    /// Close time; `null` while open.
    #[serde(default)]
    pub closed_at: Option<String>,
    /// Number of user notes (comments).
    #[serde(default)]
    pub user_notes_count: u64,
    /// Whether the issue is confidential.
    #[serde(default)]
    pub confidential: bool,
    /// `issue`, `incident`, `test_case` or `task`, as sent.
    #[serde(default)]
    pub issue_type: Option<String>,
    /// The issue's page in the GitLab UI.
    pub web_url: String,
    /// `#iid` references; `full` is `namespace/project#iid`.
    #[serde(default)]
    pub references: Option<References>,
    /// API links (`self`, `notes`, `award_emoji`, `project`).
    #[serde(default, rename = "_links")]
    pub links: Option<IssueLinks>,
}

/// `state` of an issue. A value this SDK does not know is kept as sent.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, concord::WireDisplay)]
#[serde(rename_all = "snake_case")]
pub enum IssueStatus {
    /// `opened`
    Opened,
    /// `closed`
    Closed,
    /// Another value, as sent.
    #[serde(untagged)]
    Other(String),
}

impl<'de> Deserialize<'de> for IssueStatus {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let state = String::deserialize(deserializer)?;
        Ok(match state.as_str() {
            "opened" => Self::Opened,
            "closed" => Self::Closed,
            _ => Self::Other(state),
        })
    }
}

/// A user as embedded in an issue (`author`, `assignees`).
#[derive(Clone, Debug, Deserialize)]
#[non_exhaustive]
pub struct User {
    /// User ID.
    pub id: u64,
    /// Username.
    pub username: String,
    /// Display name.
    pub name: String,
    /// `active`, `blocked`, ... as sent.
    #[serde(default)]
    pub state: Option<String>,
    /// Profile page.
    #[serde(default)]
    pub web_url: Option<String>,
}

/// A milestone as embedded in an issue.
#[derive(Clone, Debug, Deserialize)]
#[non_exhaustive]
pub struct Milestone {
    /// Global ID.
    pub id: u64,
    /// Project- or group-scoped number.
    pub iid: u64,
    /// Title.
    pub title: String,
    /// `active` or `closed`, as sent.
    #[serde(default)]
    pub state: Option<String>,
    /// Due date (`YYYY-MM-DD`), if set.
    #[serde(default)]
    pub due_date: Option<String>,
}

/// `references` of an issue.
#[derive(Clone, Debug, Deserialize)]
#[non_exhaustive]
pub struct References {
    /// `#iid`
    pub short: String,
    /// Relative to the requested issue's project or group.
    pub relative: String,
    /// `namespace/project#iid`
    pub full: String,
}

/// `_links` of an issue: API URLs (`self`, `notes`, `award_emoji`, `project`).
#[derive(Clone, Debug, Deserialize)]
#[non_exhaustive]
pub struct IssueLinks {
    /// The issue's API URL.
    #[serde(rename = "self")]
    pub self_: Option<String>,
    /// The issue's notes.
    #[serde(default)]
    pub notes: Option<String>,
    /// The issue's award emoji.
    #[serde(default)]
    pub award_emoji: Option<String>,
    /// The project's API URL.
    #[serde(default)]
    pub project: Option<String>,
}

/// A project, as returned by `GET /projects/:id`.
#[derive(Clone, Debug, Deserialize)]
#[non_exhaustive]
pub struct Project {
    /// The global ID.
    pub id: ProjectId,
    /// Name.
    pub name: String,
    /// `Namespace / Name`.
    pub name_with_namespace: String,
    /// The last path segment.
    pub path: String,
    /// The full path (`namespace/project`): what [`ProjectRef::path`](crate::ProjectRef::path)
    /// takes.
    pub path_with_namespace: String,
    /// Description; `null` when empty.
    #[serde(default)]
    pub description: Option<String>,
    /// Default branch; `null` for an empty repository.
    #[serde(default)]
    pub default_branch: Option<String>,
    /// `private`, `internal` or `public`, as sent (absent for some callers).
    #[serde(default)]
    pub visibility: Option<String>,
    /// The project's page in the GitLab UI.
    pub web_url: String,
    /// Whether issues are enabled (absent for some callers).
    #[serde(default)]
    pub issues_enabled: Option<bool>,
    /// Open issues (absent when issues are disabled or not visible).
    #[serde(default)]
    pub open_issues_count: Option<u64>,
    /// Whether the project is archived.
    #[serde(default)]
    pub archived: bool,
    /// Creation time (ISO 8601, as sent).
    #[serde(default)]
    pub created_at: Option<String>,
    /// Last activity (ISO 8601, as sent).
    #[serde(default)]
    pub last_activity_at: Option<String>,
    /// The namespace (group or user) that holds the project.
    pub namespace: Namespace,
}

/// The `namespace` of a project.
#[derive(Clone, Debug, Deserialize)]
#[non_exhaustive]
pub struct Namespace {
    /// ID.
    pub id: u64,
    /// Name.
    pub name: String,
    /// Last path segment.
    pub path: String,
    /// `group` or `user`, as sent.
    pub kind: String,
    /// Full path (`group/subgroup`).
    pub full_path: String,
    /// The namespace's page in the GitLab UI.
    #[serde(default)]
    pub web_url: Option<String>,
}
