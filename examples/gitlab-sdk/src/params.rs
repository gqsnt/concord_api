//! Typed inputs: the project reference, identifiers, the private token and the listing filters.
//!
//! Each type's `Display` is its wire spelling; Concord percent-encodes it for its placement. Values
//! the SDK can refuse locally are refused when they are constructed ([`InputError`]), so a refused
//! value never creates a request.

use std::fmt;
use std::num::NonZeroU64;

use concord::WireDisplay;
use concord::reqwest::header::HeaderValue;
use serde::{Deserialize, Serialize};

/// Why an input was refused before any request was built. `Display` names the rule, never the
/// value (a token or a project path can be private).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum InputError {
    /// A project path is empty, or has no `/` (a full path is `namespace/project`, and a bare
    /// number would be read by GitLab as a project ID).
    ProjectPathNotNamespaced,
    /// A project path has an empty segment (a leading, trailing or doubled `/`) or a `.`/`..`
    /// segment.
    ProjectPathSegment,
    /// A project path contains `%`: pass the path as written in GitLab (`group/project`), not
    /// already URL-encoded; the SDK encodes it exactly once.
    ProjectPathPreEncoded,
    /// A project path contains whitespace or a control character.
    ProjectPathCharacter,
    /// A project ID, issue IID or issue ID is 0 (GitLab's identifiers start at 1).
    ZeroId,
    /// A private token is empty, starts or ends with whitespace, or is not visible ASCII.
    Token,
    /// `per_page` is outside GitLab's documented range, 1 to 100.
    PerPage,
    /// A label name is empty or contains a comma (the `labels` filter is comma-separated).
    Label,
}

impl fmt::Display for InputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ProjectPathNotNamespaced => {
                "a project path must be `namespace/project` (at least one `/`)"
            }
            Self::ProjectPathSegment => {
                "a project path segment is empty, `.` or `..` (leading, trailing or doubled `/`)"
            }
            Self::ProjectPathPreEncoded => {
                "a project path contains `%`: pass it unencoded, the SDK encodes it once"
            }
            Self::ProjectPathCharacter => {
                "a project path contains whitespace or a control character"
            }
            Self::ZeroId => "GitLab identifiers start at 1",
            Self::Token => {
                "a private token must be non-empty visible ASCII without surrounding whitespace"
            }
            Self::PerPage => "per_page must be between 1 and 100",
            Self::Label => "a label name must be non-empty and contain no comma",
        })
    }
}

impl std::error::Error for InputError {}

/// A project's global ID (`id` of a project, `project_id` of an issue).
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Deserialize, Serialize, WireDisplay,
)]
#[serde(transparent)]
pub struct ProjectId(pub NonZeroU64);

impl ProjectId {
    /// The ID `id`.
    ///
    /// # Errors
    ///
    /// [`InputError::ZeroId`] for 0.
    pub fn new(id: u64) -> Result<Self, InputError> {
        NonZeroU64::new(id).map(Self).ok_or(InputError::ZeroId)
    }

    /// The number.
    pub fn get(self) -> u64 {
        self.0.get()
    }
}

/// An issue's project-scoped number (`iid`): the `#7` of `group/project#7`, and the number in
/// `GET /projects/:id/issues/:issue_iid`. Not the global [`IssueId`].
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Deserialize, Serialize, WireDisplay,
)]
#[serde(transparent)]
pub struct IssueIid(pub NonZeroU64);

impl IssueIid {
    /// The IID `iid`.
    ///
    /// # Errors
    ///
    /// [`InputError::ZeroId`] for 0.
    pub fn new(iid: u64) -> Result<Self, InputError> {
        NonZeroU64::new(iid).map(Self).ok_or(InputError::ZeroId)
    }

    /// The number.
    pub fn get(self) -> u64 {
        self.0.get()
    }
}

/// An issue's global ID (`id`). No endpoint of this slice is addressed by it.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Deserialize, Serialize, WireDisplay,
)]
#[serde(transparent)]
pub struct IssueId(pub NonZeroU64);

impl IssueId {
    /// The number.
    pub fn get(self) -> u64 {
        self.0.get()
    }
}

/// The `:id` of `/projects/:id/...`: "the ID or URL-encoded path of the project".
///
/// A path is kept as GitLab writes it (`group/subgroup/project`) and is written into the request
/// as **one** path segment, so every `/` is sent as `%2F`
/// (`/projects/group%2Fsubgroup%2Fproject/issues`) — Concord encodes each path value exactly once.
#[derive(Clone, PartialEq, Eq, Hash, Serialize, WireDisplay)]
#[serde(untagged)]
pub enum ProjectRef {
    /// A project ID: `/projects/42`.
    Id(ProjectId),
    /// A full project path (`namespace/project`), unencoded.
    Path(String),
}

impl ProjectRef {
    /// A project by ID.
    ///
    /// # Errors
    ///
    /// [`InputError::ZeroId`] for 0.
    pub fn id(id: u64) -> Result<Self, InputError> {
        ProjectId::new(id).map(Self::Id)
    }

    /// A project by its full path, unencoded: `"gitlab-org/gitlab"`, `"group/sub/project"`.
    ///
    /// The SDK checks only what would make the request ambiguous or wrong, not GitLab's naming
    /// rules: at least one `/`, no empty, `.` or `..` segment, no `%` (an already-encoded path
    /// would be encoded twice), no whitespace or control character.
    ///
    /// # Errors
    ///
    /// The [`InputError`] naming the rule.
    pub fn path(path: impl Into<String>) -> Result<Self, InputError> {
        let path = path.into();
        if path.contains('%') {
            return Err(InputError::ProjectPathPreEncoded);
        }
        if path.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(InputError::ProjectPathCharacter);
        }
        if !path.contains('/') {
            return Err(InputError::ProjectPathNotNamespaced);
        }
        if path
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
        {
            return Err(InputError::ProjectPathSegment);
        }
        Ok(Self::Path(path))
    }
}

impl From<ProjectId> for ProjectRef {
    fn from(id: ProjectId) -> Self {
        Self::Id(id)
    }
}

impl fmt::Debug for ProjectRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Id(id) => f.debug_tuple("Id").field(&id.get()).finish(),
            Self::Path(path) => f.debug_tuple("Path").field(path).finish(),
        }
    }
}

/// A personal, project or group access token, sent as `PRIVATE-TOKEN` (GitLab's recommended
/// header). The value is flagged sensitive; `Debug` never shows it.
#[derive(Clone, concord::Debug)]
pub struct PrivateToken(#[sensitive] HeaderValue);

impl PrivateToken {
    /// A token as GitLab issued it.
    ///
    /// # Errors
    ///
    /// [`InputError::Token`] if it is empty, starts or ends with whitespace (a line break copied
    /// from a file, say) or is not visible ASCII.
    pub fn new(token: impl AsRef<str>) -> Result<Self, InputError> {
        let token = token.as_ref();
        let visible = !token.is_empty() && token.bytes().all(|b| b.is_ascii_graphic());
        if !visible {
            return Err(InputError::Token);
        }
        let mut value = HeaderValue::from_str(token).map_err(|_| InputError::Token)?;
        value.set_sensitive(true);
        Ok(Self(value))
    }

    /// The header value (flagged sensitive), for [`GitLab::set_token`](crate::GitLab::set_token)
    /// when rotating a token on an existing client.
    pub fn into_header_value(self) -> HeaderValue {
        self.0
    }
}

/// `state`: which issues to list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, WireDisplay)]
#[serde(rename_all = "snake_case")]
pub enum IssueState {
    /// `opened`
    Opened,
    /// `closed`
    Closed,
    /// `all`
    All,
}

/// `order_by` for the **offset-paginated** project issue listing, the strategy this SDK pins
/// (GitLab's default `created_at`). `title` and `id` are documented for keyset pagination only
/// and are deliberately absent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, WireDisplay)]
#[serde(rename_all = "snake_case")]
pub enum IssueOrder {
    /// `created_at` (GitLab's default)
    CreatedAt,
    /// `updated_at`
    UpdatedAt,
    /// `priority`
    Priority,
    /// `due_date`
    DueDate,
    /// `relative_position`
    RelativePosition,
    /// `label_priority`
    LabelPriority,
    /// `milestone_due`
    MilestoneDue,
    /// `popularity`
    Popularity,
    /// `weight`
    Weight,
}

/// `sort`: `asc` or `desc` (GitLab's default `desc`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, WireDisplay)]
#[serde(rename_all = "snake_case")]
pub enum SortOrder {
    /// `asc`
    Asc,
    /// `desc`
    Desc,
}

/// `per_page`: 1 to 100 (GitLab's default 20). It sets the size of the **first** page; later
/// pages are requested with GitLab's `Link` URLs exactly as sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, WireDisplay)]
#[serde(transparent)]
pub struct PerPage(u8);

impl PerPage {
    /// GitLab's documented maximum.
    pub const MAX: Self = Self(100);

    /// `n` items per page.
    ///
    /// # Errors
    ///
    /// [`InputError::PerPage`] outside 1–100.
    pub fn new(n: u8) -> Result<Self, InputError> {
        if (1..=100).contains(&n) {
            Ok(Self(n))
        } else {
            Err(InputError::PerPage)
        }
    }

    /// The number.
    pub fn get(self) -> u8 {
        self.0
    }
}

/// `labels`: issues having **all** these labels (comma-separated on the wire), or GitLab's
/// special values `Any` (at least one label) and `None` (no label).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Labels(LabelsRepr);

#[derive(Clone, Debug, PartialEq, Eq)]
enum LabelsRepr {
    AllOf(Vec<String>),
    Any,
    None,
}

impl Labels {
    /// Issues having every one of `labels`.
    ///
    /// # Errors
    ///
    /// [`InputError::Label`] if there is no label, or one is empty or contains a comma.
    pub fn all_of<I, S>(labels: I) -> Result<Self, InputError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let labels: Vec<String> = labels.into_iter().map(Into::into).collect();
        if labels.is_empty() || labels.iter().any(|l| l.is_empty() || l.contains(',')) {
            return Err(InputError::Label);
        }
        Ok(Self(LabelsRepr::AllOf(labels)))
    }

    /// `Any`: issues with at least one label.
    pub fn any() -> Self {
        Self(LabelsRepr::Any)
    }

    /// `None`: issues without labels.
    pub fn none() -> Self {
        Self(LabelsRepr::None)
    }
}

impl fmt::Display for Labels {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            LabelsRepr::AllOf(labels) => f.write_str(&labels.join(",")),
            LabelsRepr::Any => f.write_str("Any"),
            LabelsRepr::None => f.write_str("None"),
        }
    }
}
