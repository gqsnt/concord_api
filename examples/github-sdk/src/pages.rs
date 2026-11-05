//! Ready-made page factories ([`LinkPages`]) for the declared list operations, as
//! [`RepoContext`] methods. A top-level listing takes the generated operation (the caller sets
//! filters and `per_page` with the generated setters); a child listing builds the operation for
//! its parent itself and lets the caller configure it, so a continuation's binding (operation,
//! repository, parent number or id) always matches the request it continues.
//!
//! ```
//! use concord_example_github_sdk::{GitHubConfig, GitHubContext, RepoRef};
//! use concord_example_github_sdk::ids::IssueNumber;
//! use concord_example_github_sdk::params::PerPage;
//!
//! # fn main() -> Result<(), concord_example_github_sdk::params::ParamError> {
//! let github = GitHubContext::new(Default::default(), GitHubConfig { token: "t".into() });
//! let repo = github.repo(RepoRef::new("octo-org", "hello-world"));
//! let number = IssueNumber(7);
//! let per_page = PerPage::new(50)?;
//! let comments = repo.issue_comment_pages(number, |op| op.set_per_page(per_page));
//! assert_eq!(comments.binding().parent(), Some(7));
//! # Ok(()) }
//! ```

use crate::api::{
    GitHubRepoActionsArtifacts, GitHubRepoActionsRunArtifacts, GitHubRepoActionsRunJobs,
    GitHubRepoActionsRuns, GitHubRepoActionsWorkflows, GitHubRepoIssueComments, GitHubRepoIssues,
    GitHubRepoLabels, GitHubRepoPullFiles, GitHubRepoPullReviews, GitHubRepoPulls,
    GitHubRepoRepoComments,
};
use crate::context::RepoContext;
use crate::ids::{IssueNumber, PullNumber, RunId};
use crate::models::{
    ArtifactList, IssueComment, IssueItem, JobList, Label, PullRequestFile, PullRequestSummary,
    Review, WorkflowList, WorkflowRunList,
};
use crate::paging::{LinkPages, operation};

impl RepoContext {
    /// Pages of `GET .../issues` (pull requests included).
    pub fn issue_pages(&self, op: GitHubRepoIssues) -> LinkPages<GitHubRepoIssues, Vec<IssueItem>> {
        LinkPages::new(self, operation::ISSUES, None, op, GitHubRepoIssues::prepare)
    }

    /// Pages of `GET .../issues/{number}/comments`; `configure` sets the operation's optional values.
    pub fn issue_comment_pages(
        &self,
        number: IssueNumber,
        configure: impl FnOnce(GitHubRepoIssueComments) -> GitHubRepoIssueComments,
    ) -> LinkPages<GitHubRepoIssueComments, Vec<IssueComment>> {
        LinkPages::new(
            self,
            operation::ISSUE_COMMENTS,
            Some(number.get()),
            configure(self.handle().issue_comments(number)),
            GitHubRepoIssueComments::prepare,
        )
    }

    /// Pages of `GET .../issues/comments`.
    pub fn repo_comment_pages(
        &self,
        op: GitHubRepoRepoComments,
    ) -> LinkPages<GitHubRepoRepoComments, Vec<IssueComment>> {
        LinkPages::new(
            self,
            operation::REPO_COMMENTS,
            None,
            op,
            GitHubRepoRepoComments::prepare,
        )
    }

    /// Pages of `GET .../labels`.
    pub fn label_pages(&self, op: GitHubRepoLabels) -> LinkPages<GitHubRepoLabels, Vec<Label>> {
        LinkPages::new(self, operation::LABELS, None, op, GitHubRepoLabels::prepare)
    }

    /// Pages of `GET .../pulls`.
    pub fn pull_pages(
        &self,
        op: GitHubRepoPulls,
    ) -> LinkPages<GitHubRepoPulls, Vec<PullRequestSummary>> {
        LinkPages::new(self, operation::PULLS, None, op, GitHubRepoPulls::prepare)
    }

    /// Pages of `GET .../pulls/{number}/reviews`; `configure` sets the operation's optional values.
    pub fn pull_review_pages(
        &self,
        number: PullNumber,
        configure: impl FnOnce(GitHubRepoPullReviews) -> GitHubRepoPullReviews,
    ) -> LinkPages<GitHubRepoPullReviews, Vec<Review>> {
        LinkPages::new(
            self,
            operation::PULL_REVIEWS,
            Some(number.get()),
            configure(self.handle().pull_reviews(number)),
            GitHubRepoPullReviews::prepare,
        )
    }

    /// Pages of `GET .../pulls/{number}/files`; `configure` sets the operation's optional values.
    pub fn pull_file_pages(
        &self,
        number: PullNumber,
        configure: impl FnOnce(GitHubRepoPullFiles) -> GitHubRepoPullFiles,
    ) -> LinkPages<GitHubRepoPullFiles, Vec<PullRequestFile>> {
        LinkPages::new(
            self,
            operation::PULL_FILES,
            Some(number.get()),
            configure(self.handle().pull_files(number)),
            GitHubRepoPullFiles::prepare,
        )
    }

    /// Pages of `GET .../actions/workflows` (items moved out of `workflows`).
    pub fn workflow_pages(
        &self,
        op: GitHubRepoActionsWorkflows,
    ) -> LinkPages<GitHubRepoActionsWorkflows, WorkflowList> {
        LinkPages::new(
            self,
            operation::WORKFLOWS,
            None,
            op,
            GitHubRepoActionsWorkflows::prepare,
        )
    }

    /// Pages of `GET .../actions/runs` (items moved out of `workflow_runs`).
    pub fn run_pages(
        &self,
        op: GitHubRepoActionsRuns,
    ) -> LinkPages<GitHubRepoActionsRuns, WorkflowRunList> {
        LinkPages::new(
            self,
            operation::RUNS,
            None,
            op,
            GitHubRepoActionsRuns::prepare,
        )
    }

    /// Pages of `GET .../actions/runs/{run_id}/jobs` (items moved out of `jobs`); `configure` sets
    /// the operation's optional values.
    pub fn run_job_pages(
        &self,
        run: RunId,
        configure: impl FnOnce(GitHubRepoActionsRunJobs) -> GitHubRepoActionsRunJobs,
    ) -> LinkPages<GitHubRepoActionsRunJobs, JobList> {
        LinkPages::new(
            self,
            operation::RUN_JOBS,
            Some(run.get()),
            configure(self.actions().run_jobs(run)),
            GitHubRepoActionsRunJobs::prepare,
        )
    }

    /// Pages of `GET .../actions/runs/{run_id}/artifacts` (items moved out of `artifacts`);
    /// `op` must be for the same run.
    pub fn run_artifact_pages(
        &self,
        run: RunId,
        configure: impl FnOnce(GitHubRepoActionsRunArtifacts) -> GitHubRepoActionsRunArtifacts,
    ) -> LinkPages<GitHubRepoActionsRunArtifacts, ArtifactList> {
        LinkPages::new(
            self,
            operation::RUN_ARTIFACTS,
            Some(run.get()),
            configure(self.actions().run_artifacts(run)),
            GitHubRepoActionsRunArtifacts::prepare,
        )
    }

    /// Pages of `GET .../actions/artifacts` (items moved out of `artifacts`).
    pub fn artifact_pages(
        &self,
        op: GitHubRepoActionsArtifacts,
    ) -> LinkPages<GitHubRepoActionsArtifacts, ArtifactList> {
        LinkPages::new(
            self,
            operation::ARTIFACTS,
            None,
            op,
            GitHubRepoActionsArtifacts::prepare,
        )
    }
}
