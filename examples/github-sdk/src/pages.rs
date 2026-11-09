//! Ready-made listings ([`LinkPages`]) for the declared list operations, as [`RepoContext`]
//! methods. A top-level listing takes the generated operation (the caller sets filters and
//! `per_page` with the generated setters); a child listing builds the operation for its parent
//! itself and lets the caller configure it, so a continuation's binding (operation, repository,
//! parent number or id) always matches the request it continues.
//!
//! The ordinary path is `.items()` or `.pages()` on the listing: an owned traversal whose end
//! is explicit (`Ok(None)` only after the last page; a cap or a failed page is an error, see
//! [`crate::paging`]). The listing itself is also the page factory of `concord-pagination`'s
//! engine (`|key| listing.fetch(key)`), for item-level checkpoints.
//!
//! ```no_run
//! use std::num::NonZeroUsize;
//! use concord_example_github_sdk::{GitHubConfig, GitHubContext, RepoRef};
//! use concord_example_github_sdk::ids::IssueNumber;
//! use concord_example_github_sdk::params::PerPage;
//!
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! let github = GitHubContext::new(Default::default(), GitHubConfig { token: "t".into() });
//! let repo = github.repo(RepoRef::new("octo-org", "hello-world"));
//! let per_page = PerPage::new(50)?;
//! let comments = repo.issue_comment_pages(IssueNumber(7), |op| op.set_per_page(per_page));
//! assert_eq!(comments.binding().parent(), Some(7));
//!
//! // Every comment, page after page, at most 20 pages; `?`: a cap or a failure is an error.
//! let mut items = comments.items().max_pages(NonZeroUsize::new(20).unwrap());
//! while let Some(comment) = items.next().await? {
//!     println!("{}", comment.id);
//! }
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
        LinkPages::new(self, operation::ISSUES, None, op)
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
        )
    }

    /// Pages of `GET .../issues/comments`.
    pub fn repo_comment_pages(
        &self,
        op: GitHubRepoRepoComments,
    ) -> LinkPages<GitHubRepoRepoComments, Vec<IssueComment>> {
        LinkPages::new(self, operation::REPO_COMMENTS, None, op)
    }

    /// Pages of `GET .../labels`.
    pub fn label_pages(&self, op: GitHubRepoLabels) -> LinkPages<GitHubRepoLabels, Vec<Label>> {
        LinkPages::new(self, operation::LABELS, None, op)
    }

    /// Pages of `GET .../pulls`.
    pub fn pull_pages(
        &self,
        op: GitHubRepoPulls,
    ) -> LinkPages<GitHubRepoPulls, Vec<PullRequestSummary>> {
        LinkPages::new(self, operation::PULLS, None, op)
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
        )
    }

    /// Pages of `GET .../actions/workflows` (items moved out of `workflows`).
    pub fn workflow_pages(
        &self,
        op: GitHubRepoActionsWorkflows,
    ) -> LinkPages<GitHubRepoActionsWorkflows, WorkflowList> {
        LinkPages::new(self, operation::WORKFLOWS, None, op)
    }

    /// Pages of `GET .../actions/runs` (items moved out of `workflow_runs`).
    pub fn run_pages(
        &self,
        op: GitHubRepoActionsRuns,
    ) -> LinkPages<GitHubRepoActionsRuns, WorkflowRunList> {
        LinkPages::new(self, operation::RUNS, None, op)
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
        )
    }

    /// Pages of `GET .../actions/artifacts` (items moved out of `artifacts`).
    pub fn artifact_pages(
        &self,
        op: GitHubRepoActionsArtifacts,
    ) -> LinkPages<GitHubRepoActionsArtifacts, ArtifactList> {
        LinkPages::new(self, operation::ARTIFACTS, None, op)
    }
}

#[cfg(test)]
mod tests {
    use crate::api::{
        GitHubOperation, GitHubRepoActionsArtifacts, GitHubRepoActionsRunArtifacts,
        GitHubRepoActionsRunJobs, GitHubRepoActionsRuns, GitHubRepoActionsWorkflows,
        GitHubRepoIssueComments, GitHubRepoIssues, GitHubRepoLabels, GitHubRepoPullFiles,
        GitHubRepoPullReviews, GitHubRepoPulls, GitHubRepoRepoComments,
    };
    use crate::ids::{IssueNumber, PullNumber, RunId};
    use crate::paging::operation::listing_id;
    use crate::{GitHubConfig, GitHubContext, RepoRef};

    /// The ready-made factories pass their operation id by hand: each must be the one
    /// `listing_id` gives the endpoint they prepare.
    #[test]
    fn every_factory_binds_the_listing_id_of_its_endpoint() {
        let github = GitHubContext::new(Default::default(), GitHubConfig { token: "t".into() });
        let repo = github.repo(RepoRef::new("octo-org", "hello-world"));
        let id = |operation: concord::Operation| {
            GitHubOperation::from_operation(operation).and_then(listing_id)
        };
        let h = repo.handle();
        let a = repo.actions();
        let bound = [
            (
                repo.issue_pages(h.issues()).binding().operation(),
                id(GitHubRepoIssues::operation()),
            ),
            (
                repo.issue_comment_pages(IssueNumber(1), |op| op)
                    .binding()
                    .operation(),
                id(GitHubRepoIssueComments::operation()),
            ),
            (
                repo.repo_comment_pages(h.repo_comments())
                    .binding()
                    .operation(),
                id(GitHubRepoRepoComments::operation()),
            ),
            (
                repo.label_pages(h.labels()).binding().operation(),
                id(GitHubRepoLabels::operation()),
            ),
            (
                repo.pull_pages(h.pulls()).binding().operation(),
                id(GitHubRepoPulls::operation()),
            ),
            (
                repo.pull_review_pages(PullNumber(1), |op| op)
                    .binding()
                    .operation(),
                id(GitHubRepoPullReviews::operation()),
            ),
            (
                repo.pull_file_pages(PullNumber(1), |op| op)
                    .binding()
                    .operation(),
                id(GitHubRepoPullFiles::operation()),
            ),
            (
                repo.workflow_pages(a.workflows()).binding().operation(),
                id(GitHubRepoActionsWorkflows::operation()),
            ),
            (
                repo.run_pages(a.runs()).binding().operation(),
                id(GitHubRepoActionsRuns::operation()),
            ),
            (
                repo.run_job_pages(RunId(1), |op| op).binding().operation(),
                id(GitHubRepoActionsRunJobs::operation()),
            ),
            (
                repo.run_artifact_pages(RunId(1), |op| op)
                    .binding()
                    .operation(),
                id(GitHubRepoActionsRunArtifacts::operation()),
            ),
            (
                repo.artifact_pages(a.artifacts()).binding().operation(),
                id(GitHubRepoActionsArtifacts::operation()),
            ),
        ];
        for (factory, declared) in bound {
            assert_eq!(Some(factory), declared);
        }
    }
}
