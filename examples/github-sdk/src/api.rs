//! The Concord declaration of the GitHub REST slice.
//!
//! Every operation is an ordinary generated endpoint: callers keep its setters, `prepare()`,
//! `send()`, `send_raw()` and `into_parts()`. Shared configuration lives where GitHub applies it:
//! the token, `Accept: application/vnd.github+json`, `X-GitHub-Api-Version` and the required
//! `User-Agent` on the client; `repos/{owner}/{repo}` on the repository scope; `actions` on the
//! Actions scope.
//!
//! What the declaration deliberately does not do:
//!
//! - **No `page` parameter.** GitHub's pagination guide says to follow the `Link` header rather
//!   than build page queries; `page next link` identifies its continuation source.
//!   [`crate::paging`] validates and follows those URLs through the same metadata policy.
//! - **No typed output on the artifact download.** `GET .../artifacts/{id}/zip` answers `302`
//!   with a `Location`; it is handled at the native boundary by [`crate::download`], never decoded
//!   as JSON.
//! - **Conditional requests** send `If-None-Match` from an optional [`ETag`]; a `304` is a status
//!   error for `send()`, and [`crate::meta::send_conditional`] turns it into
//!   [`crate::meta::Conditional::NotModified`].

use concord::Json;
use concord::reqwest::header::{ACCEPT, AUTHORIZATION, HeaderName, IF_NONE_MATCH, USER_AGENT};

use crate::ids::{
    ArtifactId, CheckSuiteId, CommentId, CommitSha, IssueNumber, JobId, Login, Owner, PullNumber,
    RepoName, ReviewId, RunId, WorkflowRef,
};
use crate::models::{
    Artifact, ArtifactList, IssueComment, IssueItem, Job, JobList, Label, PullRequest,
    PullRequestFile, PullRequestSummary, Repository, Review, WorkflowList, WorkflowRun,
    WorkflowRunList,
};
use crate::params::{
    ApiVersion, ArchiveFormat, CommentSort, Direction, ETag, IssueSort, JobFilter, Labels, PerPage,
    PullSort, RunStatus, StateFilter, Timestamp,
};

/// `X-GitHub-Api-Version`.
pub const X_GITHUB_API_VERSION: HeaderName = HeaderName::from_static("x-github-api-version");

/// The `User-Agent` sent unless the caller sets another one. GitHub rejects requests without a
/// `User-Agent` and asks for the application's name.
pub const DEFAULT_USER_AGENT: &str = "concord-example-github-sdk";

fn default_user_agent() -> String {
    DEFAULT_USER_AGENT.to_owned()
}

concord::api! {
    /// A bounded slice of the GitHub REST API (version pinned by [`ApiVersion::pinned`]).
    pub client GitHub(
        /// A personal access token, GitHub App installation token or `GITHUB_TOKEN`, sent as
        /// `Authorization: Bearer <token>`. Never printed by Concord.
        #[sensitive] token: String,
        /// The REST API version, sent as `X-GitHub-Api-Version`.
        api_version: ApiVersion = ApiVersion::pinned(),
        /// The `User-Agent`; GitHub requires one.
        header(USER_AGENT) user_agent: String = default_user_agent(),
    ) {
        base "https://api.github.com";
        headers {
            AUTHORIZATION = "Bearer {token}";
            ACCEPT = "application/vnd.github+json";
            X_GITHUB_API_VERSION = "{api_version}";
        }
        format Json;
        /// Every declared operation: [`paging::operation::listing_id`](crate::paging::operation)
        /// retains the provider operation ids used by stored listing bindings.
        operations GitHubOperation;

        /// One repository: `repos/{owner}/{repo}`.
        scope repo(owner: Owner, name: RepoName) {
            path "repos/{owner}/{name}";

            /// `GET /repos/{owner}/{repo}` (`repos/get`). With `if_none_match`, a conditional
            /// request (`304` when unchanged).
            GET get(if_none_match?: ETag) -> Repository {
                headers { IF_NONE_MATCH = "{if_none_match}"; }
            }

            /// `GET /repos/{owner}/{repo}/labels` (`issues/list-labels-for-repo`).
            GET labels(query per_page?: PerPage) -> Vec<Label> {
                path "labels";
                page next link;
            }

            /// `GET /repos/{owner}/{repo}/issues` (`issues/list-for-repo`). Includes pull
            /// requests ([`IssueItem::PullRequest`]). Every filter is optional; GitHub's defaults
            /// are `state=open`, `sort=created`, `direction=desc`.
            GET issues(
                /// A milestone number, `*` (any) or `none`.
                query milestone?: String,
                /// `open`, `closed` or `all`.
                query state?: StateFilter,
                /// A login, `*` (any assignee) or `none`.
                query assignee?: String,
                /// `type`: an issue type's name (`Bug`), `*` (issues with any type) or `none`
                /// (issues without a type).
                query("type") issue_type?: String,
                /// The author's login.
                query creator?: Login,
                /// A mentioned login.
                query mentioned?: Login,
                /// Issues having all of these labels.
                query labels?: Labels,
                /// Sort key.
                query sort?: IssueSort,
                /// Sort direction.
                query direction?: Direction,
                /// Only issues updated at or after this time.
                query since?: Timestamp,
                /// Page size, 1–100 (GitHub's default 30).
                query per_page?: PerPage,
            ) -> Vec<IssueItem> {
                path "issues";
                page next link;
            }

            /// `GET /repos/{owner}/{repo}/issues/{number}` (`issues/get`). A pull request's number
            /// returns its issue representation. With `if_none_match`, a conditional request.
            GET issue(number: IssueNumber, if_none_match?: ETag) -> IssueItem {
                path "issues/{number}";
                headers { IF_NONE_MATCH = "{if_none_match}"; }
            }

            /// `GET /repos/{owner}/{repo}/issues/{number}/comments` (`issues/list-comments`),
            /// ascending by id.
            GET issue_comments(
                number: IssueNumber,
                /// Only comments updated at or after this time.
                query since?: Timestamp,
                /// Page size, 1–100.
                query per_page?: PerPage,
            ) -> Vec<IssueComment> {
                path "issues/{number}/comments";
                page next link;
            }

            /// `GET /repos/{owner}/{repo}/issues/comments` (`issues/list-comments-for-repo`).
            GET repo_comments(
                /// Sort key.
                query sort?: CommentSort,
                /// Ignored by GitHub unless `sort` is set.
                query direction?: Direction,
                /// Only comments updated at or after this time.
                query since?: Timestamp,
                /// Page size, 1–100.
                query per_page?: PerPage,
            ) -> Vec<IssueComment> {
                path "issues/comments";
                page next link;
            }

            /// `GET /repos/{owner}/{repo}/issues/comments/{comment_id}` (`issues/get-comment`).
            GET comment(comment_id: CommentId) -> IssueComment {
                path "issues/comments/{comment_id}";
            }

            /// `GET /repos/{owner}/{repo}/pulls` (`pulls/list`).
            GET pulls(
                /// `open`, `closed` or `all`.
                query state?: StateFilter,
                /// `user:ref-name` or `organization:ref-name`.
                query head?: String,
                /// Base branch name.
                query base?: String,
                /// Sort key.
                query sort?: PullSort,
                /// Sort direction.
                query direction?: Direction,
                /// Page size, 1–100.
                query per_page?: PerPage,
            ) -> Vec<PullRequestSummary> {
                path "pulls";
                page next link;
            }

            /// `GET /repos/{owner}/{repo}/pulls/{number}` (`pulls/get`). With `if_none_match`, a
            /// conditional request.
            GET pull(number: PullNumber, if_none_match?: ETag) -> PullRequest {
                path "pulls/{number}";
                headers { IF_NONE_MATCH = "{if_none_match}"; }
            }

            /// `GET /repos/{owner}/{repo}/pulls/{number}/reviews` (`pulls/list-reviews`),
            /// chronological.
            GET pull_reviews(
                number: PullNumber,
                /// Page size, 1–100.
                query per_page?: PerPage,
            ) -> Vec<Review> {
                path "pulls/{number}/reviews";
                page next link;
            }

            /// `GET /repos/{owner}/{repo}/pulls/{number}/reviews/{review_id}` (`pulls/get-review`).
            GET pull_review(number: PullNumber, review_id: ReviewId) -> Review {
                path "pulls/{number}/reviews/{review_id}";
            }

            /// `GET /repos/{owner}/{repo}/pulls/{number}/files` (`pulls/list-files`), at most
            /// 3,000 files.
            GET pull_files(
                number: PullNumber,
                /// Page size, 1–100.
                query per_page?: PerPage,
            ) -> Vec<PullRequestFile> {
                path "pulls/{number}/files";
                page next link;
            }

            /// GitHub Actions: `repos/{owner}/{repo}/actions`.
            scope actions {
                path "actions";

                /// `GET .../actions/workflows` (`actions/list-repo-workflows`).
                GET workflows(query per_page?: PerPage) -> WorkflowList {
                    path "workflows";
                    page next link;
                }

                /// `GET .../actions/runs` (`actions/list-workflow-runs-for-repo`).
                GET runs(
                    /// The login that triggered the run.
                    query actor?: Login,
                    /// Branch of the push.
                    query branch?: String,
                    /// Triggering event (`push`, `pull_request`, ...).
                    query event?: String,
                    /// Status or conclusion.
                    query status?: RunStatus,
                    /// Creation-time range in GitHub's search syntax (`>=2026-01-01`).
                    query created?: String,
                    /// Omit `pull_requests` from the runs.
                    query exclude_pull_requests?: bool,
                    /// Check suite.
                    query check_suite_id?: CheckSuiteId,
                    /// Head commit.
                    query head_sha?: CommitSha,
                    /// Page size, 1–100.
                    query per_page?: PerPage,
                ) -> WorkflowRunList {
                    path "runs";
                    page next link;
                }

                /// `GET .../actions/workflows/{workflow}/runs` (`actions/list-workflow-runs`).
                GET workflow_runs(
                    workflow: WorkflowRef,
                    /// The login that triggered the run.
                    query actor?: Login,
                    /// Branch of the push.
                    query branch?: String,
                    /// Triggering event.
                    query event?: String,
                    /// Status or conclusion.
                    query status?: RunStatus,
                    /// Creation-time range in GitHub's search syntax.
                    query created?: String,
                    /// Head commit.
                    query head_sha?: CommitSha,
                    /// Page size, 1–100.
                    query per_page?: PerPage,
                ) -> WorkflowRunList {
                    path "workflows/{workflow}/runs";
                    page next link;
                }

                /// `GET .../actions/runs/{run_id}` (`actions/get-workflow-run`).
                GET run(run_id: RunId, query exclude_pull_requests?: bool) -> WorkflowRun {
                    path "runs/{run_id}";
                }

                /// `GET .../actions/runs/{run_id}/jobs` (`actions/list-jobs-for-workflow-run`).
                GET run_jobs(
                    run_id: RunId,
                    /// `latest` (default) or `all` attempts.
                    query filter?: JobFilter,
                    /// Page size, 1–100.
                    query per_page?: PerPage,
                ) -> JobList {
                    path "runs/{run_id}/jobs";
                    page next link;
                }

                /// `GET .../actions/jobs/{job_id}` (`actions/get-job-for-workflow-run`).
                GET job(job_id: JobId) -> Job { path "jobs/{job_id}"; }

                /// `GET .../actions/artifacts` (`actions/list-artifacts-for-repo`).
                GET artifacts(
                    /// Exact artifact name.
                    query name?: String,
                    /// Page size, 1–100.
                    query per_page?: PerPage,
                ) -> ArtifactList {
                    path "artifacts";
                    page next link;
                }

                /// `GET .../actions/runs/{run_id}/artifacts`
                /// (`actions/list-workflow-run-artifacts`).
                GET run_artifacts(
                    run_id: RunId,
                    /// Exact artifact name.
                    query name?: String,
                    /// Order (GitHub's default `desc`).
                    query direction?: Direction,
                    /// Page size, 1–100.
                    query per_page?: PerPage,
                ) -> ArtifactList {
                    path "runs/{run_id}/artifacts";
                    page next link;
                }

                /// `GET .../actions/artifacts/{artifact_id}` (`actions/get-artifact`).
                GET artifact(artifact_id: ArtifactId) -> Artifact {
                    path "artifacts/{artifact_id}";
                }

                /// `GET .../actions/artifacts/{artifact_id}/{archive_format}`
                /// (`actions/download-artifact`): `302` with a short-lived `Location`, or `410`.
                /// No typed output: use [`crate::download`], which handles the redirect natively.
                GET download_artifact(
                    artifact_id: ArtifactId,
                    archive_format: ArchiveFormat = ArchiveFormat::Zip,
                ) {
                    path "artifacts/{artifact_id}/{archive_format}";
                }
            }
        }
    }
}
