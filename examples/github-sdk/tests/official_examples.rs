//! The SDK's models decode the official examples of the pinned OpenAPI description
//! (github/rest-api-description, MIT; see the application's `tests/fixtures/MANIFEST.json`).
//!
//! These are body-only decodes with the endpoints' own format (`Json`); no network is involved.
//! Agreement with the provider's examples is evidence that the typed models match the published
//! shapes, independently of the synthetic fixtures this SDK's author wrote.

mod support;

use concord::bytes::Bytes;
use concord::{Decode, Json};
use concord_example_github_sdk::ids::{
    ArtifactId, CommentId, IssueNumber, JobId, PullNumber, RepositoryId, ReviewId, RunId,
};
use concord_example_github_sdk::models::{
    Artifact, ArtifactList, AuthorAssociation, Conclusion, FileStatus, IssueComment, IssueItem,
    Job, JobList, Label, PullRequest, PullRequestFile, PullRequestSummary, Repository, Review,
    ReviewState, RunState, State, WorkflowList, WorkflowRun, WorkflowRunList,
};
use concord_example_github_sdk::paging::Listing;

fn decode<T: serde::de::DeserializeOwned>(text: &'static str) -> T {
    <Json as Decode<T>>::decode_body((), Bytes::from_static(text.as_bytes()))
        .unwrap_or_else(|e| panic!("official example does not decode: {e:?}"))
}

#[test]
fn repository() {
    let repo: Repository = decode(official!("full-repository-default-response.json"));
    assert_eq!(repo.id, RepositoryId(1296269));
    assert_eq!(repo.full_name, "octocat/Hello-World");
    assert_eq!(repo.default_branch, "master");
    assert_eq!(repo.visibility.as_deref(), Some("public"));
    assert!(repo.has_issues && !repo.archived);
}

#[test]
fn issues_listing_distinguishes_pull_requests() {
    let items: Vec<IssueItem> = decode(official!("issue-items.json"));
    assert_eq!(items.len(), 1);
    // The official example carries a `pull_request` object: it is a pull request seen through
    // the Issues API, with an issue id and a number usable with the Pulls API.
    let item = &items[0];
    assert!(item.is_pull_request());
    assert_eq!(item.number(), IssueNumber(1347));
    assert_eq!(item.pull_number(), Some(PullNumber(1347)));
    assert_eq!(item.issue().state, State::Open);
    assert_eq!(item.issue().labels[0].name(), Some("bug"));
}

#[test]
fn single_issue() {
    let item: IssueItem = decode(official!("issue.json"));
    assert_eq!(item.number(), IssueNumber(1347));
    assert_eq!(
        item.issue().author_association,
        Some(AuthorAssociation::Collaborator)
    );
}

#[test]
fn comments() {
    let comments: Vec<IssueComment> = decode(official!("issue-comment-items.json"));
    assert_eq!(comments[0].id, CommentId(1));
    let comment: IssueComment = decode(official!("issue-comment.json"));
    assert_eq!(comment.body.as_deref(), Some("Me too"));
}

#[test]
fn pull_requests() {
    let pulls: Vec<PullRequestSummary> = decode(official!("pull-request-simple-items.json"));
    assert_eq!(pulls[0].number, PullNumber(1347));
    assert_eq!(pulls[0].head.branch, "new-topic");
    let pull: PullRequest = decode(official!("pull-request.json"));
    assert_eq!(pull.summary.number, PullNumber(1347));
    assert!(!pull.merged);
    assert_eq!(pull.mergeable, Some(true));
    assert_eq!(
        (pull.commits, pull.additions, pull.changed_files),
        (3, 100, 5)
    );
}

#[test]
fn reviews_and_files() {
    let reviews: Vec<Review> = decode(official!("pull-request-review-items.json"));
    assert_eq!(reviews[0].id, ReviewId(80));
    assert_eq!(reviews[0].state, ReviewState::Approved);
    let review: Review = decode(official!("pull-request-review-4.json"));
    assert_eq!(review.id, ReviewId(80));
    let files: Vec<PullRequestFile> = decode(official!("diff-entry-items.json"));
    assert_eq!(files[0].status, FileStatus::Added);
}

#[test]
fn labels() {
    let labels: Vec<Label> = decode(official!("label-items.json"));
    assert_eq!(labels.len(), 2);
    assert_eq!(labels[0].name, "bug");
}

#[test]
fn actions_envelopes_project_their_items() {
    let workflows: WorkflowList = decode(official!("workflow-paginated.json"));
    assert_eq!(workflows.total_count, 2);
    assert_eq!(workflows.into_items().len(), 2);

    let runs: WorkflowRunList = decode(official!("workflow-run-paginated.json"));
    assert_eq!(runs.total_count(), Some(1));
    let runs = runs.into_items();
    assert_eq!(runs[0].id, RunId(30433642));
    assert_eq!(runs[0].status, Some(RunState::Queued));
    assert_eq!(runs[0].conclusion, None);

    let run: WorkflowRun = decode(official!("workflow-run.json"));
    assert_eq!(run.id, RunId(30433642));

    let jobs: JobList = decode(official!("job-paginated.json"));
    let jobs = jobs.into_items();
    assert_eq!(jobs[0].id, JobId(399444496));
    assert_eq!(jobs[0].conclusion, Some(Conclusion::Success));
    assert_eq!(jobs[0].steps.len(), 10); // as in the official example
    let job: Job = decode(official!("job.json"));
    assert_eq!(job.status, RunState::Completed);

    let artifacts: ArtifactList = decode(official!("artifact-paginated.json"));
    assert_eq!(artifacts.total_count, 2);
    let artifacts = artifacts.into_items();
    assert_eq!(artifacts[0].id, ArtifactId(11));
    assert!(!artifacts[0].expired);
    let artifact: Artifact = decode(official!("artifact.json"));
    assert_eq!(artifact.name, "Rails");
    assert_eq!(artifact.size_in_bytes, 556);
}

/// A value GitHub adds later does not fail a page: it is kept as `Unknown`.
#[test]
fn unknown_enumeration_values_are_kept() {
    let state: RunState =
        <Json as Decode<RunState>>::decode_body((), Bytes::from_static(b"\"paused\"")).unwrap();
    assert_eq!(state, RunState::Unknown("paused".to_owned()));
    assert_eq!(state.as_str(), "paused");
}
