//! Filter records for the list operations with many optional parameters.
//!
//! A filter is plain data an application can build, store and pass along; the typed wrappers
//! ([`crate::RepoContext::issues`], [`crate::RepoContext::runs`]) turn it into the generated
//! operation with `maybe_*` setters, so every `None` stays absent on the wire. The generated
//! operation remains available for anything the record does not name.

use crate::ids::{CommitSha, Login};
use crate::params::{Direction, IssueSort, Labels, PerPage, RunStatus, StateFilter, Timestamp};

/// Optional filters of `GET /repos/{owner}/{repo}/issues`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IssueFilter {
    /// `state` (GitHub's default: `open`).
    pub state: Option<StateFilter>,
    /// `labels`: all of these.
    pub labels: Option<Labels>,
    /// `since`: updated at or after.
    pub since: Option<Timestamp>,
    /// `sort`.
    pub sort: Option<IssueSort>,
    /// `direction`.
    pub direction: Option<Direction>,
    /// `assignee`: a login, `*` or `none`.
    pub assignee: Option<String>,
    /// `type`: an issue type's name, `*` (any type) or `none` (no type).
    pub issue_type: Option<String>,
    /// `creator`.
    pub creator: Option<Login>,
    /// `mentioned`.
    pub mentioned: Option<Login>,
    /// `milestone`: a number, `*` or `none`.
    pub milestone: Option<String>,
    /// `per_page`.
    pub per_page: Option<PerPage>,
}

/// Optional filters of `GET /repos/{owner}/{repo}/actions/runs`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RunFilter {
    /// `actor`.
    pub actor: Option<Login>,
    /// `branch`.
    pub branch: Option<String>,
    /// `event`.
    pub event: Option<String>,
    /// `status` (a status or a conclusion).
    pub status: Option<RunStatus>,
    /// `created`: a date range in GitHub's search syntax.
    pub created: Option<String>,
    /// `head_sha`.
    pub head_sha: Option<CommitSha>,
    /// `exclude_pull_requests`.
    pub exclude_pull_requests: Option<bool>,
    /// `per_page`.
    pub per_page: Option<PerPage>,
}
