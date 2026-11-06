//! Workflow B: one issue with its project context.

use std::fmt;

use crate::api::GitLab;
use crate::call::Call;
use crate::error::GitLabError;
use crate::meta::Response;
use crate::models::{Issue, Project};
use crate::params::{IssueIid, ProjectRef};

/// An issue and the project that contains it, each with its response metadata.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct IssueContext {
    /// `GET /projects/:id/issues/:issue_iid`.
    pub issue: Response<Issue>,
    /// `GET /projects/:id` for the issue's `project_id`.
    pub project: Response<Project>,
}

/// Why [`GitLab::issue_context`] did not complete. Each variant says what was retrieved.
#[derive(Debug)]
#[non_exhaustive]
pub enum ContextError {
    /// The issue could not be retrieved; nothing else was requested.
    Issue(GitLabError),
    /// The issue was retrieved; its project was not.
    Project {
        /// The retrieved issue.
        issue: Box<Response<Issue>>,
        /// Why the project request failed.
        error: GitLabError,
    },
    /// GitLab answered the project request with another project than the issue's `project_id`.
    Mismatch {
        /// The retrieved issue.
        issue: Box<Response<Issue>>,
        /// The project GitLab returned.
        project: Box<Response<Project>>,
    },
}

impl ContextError {
    /// The GitLab error of a failed request (`None` for a mismatch).
    pub fn gitlab_error(&self) -> Option<&GitLabError> {
        match self {
            Self::Issue(error) | Self::Project { error, .. } => Some(error),
            Self::Mismatch { .. } => None,
        }
    }

    /// The issue, if it was retrieved.
    pub fn issue(&self) -> Option<&Response<Issue>> {
        match self {
            Self::Issue(_) => None,
            Self::Project { issue, .. } | Self::Mismatch { issue, .. } => Some(issue),
        }
    }
}

impl fmt::Display for ContextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Issue(error) => write!(f, "issue not retrieved: {error}"),
            Self::Project { error, .. } => {
                write!(f, "issue retrieved, its project not: {error}")
            }
            Self::Mismatch { issue, project } => write!(
                f,
                "issue retrieved, but GitLab returned project {} for project_id {}",
                project.value.id, issue.value.project_id
            ),
        }
    }
}

impl std::error::Error for ContextError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.gitlab_error().map(|e| e as _)
    }
}

impl GitLab {
    /// Workflow B: issue `iid` of `project`, then the project that contains it.
    ///
    /// Two requests, one after the other: `GET /projects/:id/issues/:issue_iid` with `project` as
    /// given (an ID or a path), then `GET /projects/:project_id` with the ID the issue reports, so
    /// the context is the issue's own project even if a path was renamed or reassigned between
    /// the two requests. The returned project must have that ID ([`ContextError::Mismatch`]
    /// otherwise). No retry; if the issue fails, the project is not requested.
    ///
    /// ```no_run
    /// use concord_sdk_gitlab::{GitLab, IssueIid, ProjectRef};
    ///
    /// # async fn run(gitlab: GitLab) -> Result<(), Box<dyn std::error::Error>> {
    /// let context = gitlab
    ///     .issue_context(ProjectRef::path("acme/widgets")?, IssueIid::new(7)?)
    ///     .await?;
    /// println!("{} in {}", context.issue.value.title, context.project.value.path_with_namespace);
    /// # Ok(()) }
    /// ```
    ///
    /// # Errors
    ///
    /// A [`ContextError`] that keeps whatever was retrieved.
    pub async fn issue_context(
        &self,
        project: ProjectRef,
        iid: IssueIid,
    ) -> Result<IssueContext, ContextError> {
        let issue = self
            .project(project)
            .issue(iid)
            .call()
            .await
            .map_err(ContextError::Issue)?;
        let project_id = issue.value.project_id;
        let project = match self.project(ProjectRef::Id(project_id)).get().call().await {
            Ok(project) => project,
            Err(error) => {
                return Err(ContextError::Project {
                    issue: Box::new(issue),
                    error,
                });
            }
        };
        if project.value.id != project_id {
            return Err(ContextError::Mismatch {
                issue: Box::new(issue),
                project: Box::new(project),
            });
        }
        Ok(IssueContext { issue, project })
    }
}
