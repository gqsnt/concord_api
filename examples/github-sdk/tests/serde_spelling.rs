//! Scalar serde spellings and request-build failures.

use std::fmt::Display;

use concord::{Delivery, ErrorKind, reqwest};
use concord_example_github_sdk::ids::*;
use concord_example_github_sdk::models::{AuthorAssociation, RunState, State};
use concord_example_github_sdk::params::*;
use concord_example_github_sdk::{GitHub, GitHubConfig};
use serde::Serialize;

fn text(value: impl Display, expected: &str) {
    assert_eq!(value.to_string(), expected);
}

#[test]
fn closed_enum_spellings_and_existing_accessors_stay_exact() {
    text(StateFilter::Open, "open");
    text(StateFilter::Closed, "closed");
    text(StateFilter::All, "all");
    text(IssueSort::Created, "created");
    text(IssueSort::Updated, "updated");
    text(IssueSort::Comments, "comments");
    text(PullSort::Created, "created");
    text(PullSort::Updated, "updated");
    text(PullSort::Popularity, "popularity");
    text(PullSort::LongRunning, "long-running");
    text(CommentSort::Created, "created");
    text(CommentSort::Updated, "updated");
    text(Direction::Asc, "asc");
    text(Direction::Desc, "desc");
    text(RunStatus::Completed, "completed");
    text(RunStatus::ActionRequired, "action_required");
    text(RunStatus::Cancelled, "cancelled");
    text(RunStatus::Failure, "failure");
    text(RunStatus::Neutral, "neutral");
    text(RunStatus::Skipped, "skipped");
    text(RunStatus::Stale, "stale");
    text(RunStatus::Success, "success");
    text(RunStatus::TimedOut, "timed_out");
    text(RunStatus::InProgress, "in_progress");
    text(RunStatus::Queued, "queued");
    text(RunStatus::Requested, "requested");
    text(RunStatus::Waiting, "waiting");
    text(RunStatus::Pending, "pending");
    text(JobFilter::Latest, "latest");
    text(JobFilter::All, "all");
    text(ArchiveFormat::Zip, "zip");
    let spelling: &'static str = PullSort::LongRunning.as_str();
    assert_eq!(spelling, "long-running");
}

#[test]
fn identifiers_and_scalar_newtypes_keep_their_text() {
    text(RepositoryId(1), "1");
    text(IssueId(2), "2");
    text(IssueNumber(3), "3");
    text(PullRequestId(4), "4");
    text(PullNumber(5), "5");
    text(CommentId(6), "6");
    text(ReviewId(7), "7");
    text(UserId(8), "8");
    text(LabelId(9), "9");
    text(MilestoneNumber(10), "10");
    text(WorkflowId(11), "11");
    text(RunId(12), "12");
    text(JobId(13), "13");
    text(ArtifactId(14), "14");
    text(CheckSuiteId(u64::MAX), "18446744073709551615");
    text(Owner::new("a/b & café"), "a/b & café");
    text(RepoName::new("a/b"), "a/b");
    text(Login::new("octocat"), "octocat");
    text(CommitSha::new("abc"), "abc");
    text(WorkflowRef::Id(WorkflowId(123)), "123");
    text(WorkflowRef::File("release/ci.yml".into()), "release/ci.yml");
    text(PerPage::new(100).unwrap(), "100");
    text(
        Timestamp::parse("2026-01-02T03:04:05Z").unwrap(),
        "2026-01-02T03:04:05Z",
    );
    text(ApiVersion::pinned(), "2026-03-10");
    text(ETag::new("W/\"a\\b\""), "W/\"a\\b\"");
}

#[test]
fn open_enums_keep_unknown_text_and_string_only_deserialization() {
    text(State::Open, "open");
    text(
        AuthorAssociation::FirstTimeContributor,
        "FIRST_TIME_CONTRIBUTOR",
    );
    let unknown: RunState = serde_json::from_str(r#""paused/\"later\"""#).unwrap();
    assert_eq!(unknown, RunState::Unknown("paused/\"later\"".into()));
    assert_eq!(unknown.as_str(), "paused/\"later\"");
    text(&unknown, "paused/\"later\"");
    assert_eq!(
        serde_json::to_string(&unknown).unwrap(),
        r#""paused/\"later\"""#
    );
    for invalid in [r#"{"queued":null}"#, "7", "null", "[]"] {
        assert!(serde_json::from_str::<RunState>(invalid).is_err());
    }
}

#[test]
fn labels_keep_their_comma_joiner_and_values_are_encoded_once() {
    let github = GitHub::new(
        reqwest::Client::new(),
        GitHubConfig {
            token: "synthetic".into(),
        },
    );
    let request = github
        .repo(Owner::new("octo org"), RepoName::new("a/b"))
        .issues()
        .set_labels(Labels::new(["bug", "help wanted"]).unwrap())
        .set_since(Timestamp::parse("2026-01-02T03:04:05Z").unwrap())
        .prepare()
        .unwrap();
    assert_eq!(
        request.request().url().as_str(),
        "https://api.github.com/repos/octo%20org/a%2Fb/issues?labels=bug%2Chelp+wanted&since=2026-01-02T03%3A04%3A05Z",
    );
}

// The scalar adapter discovers an unsupported shape when Serialize runs.
#[derive(Clone, Serialize)]
#[serde(transparent)]
struct Sequence(Vec<String>);
serde_plain::derive_display_from_serialize!(Sequence);

concord::api! {
    client ScalarOnly {
        base "http://127.0.0.1:9";
        GET read(#[sensitive] query values: Sequence);
    }
}

#[test]
fn a_sequence_is_a_build_error_naming_the_placement_without_its_contents() {
    let value = Sequence(vec!["WIRE_DISPLAY_CANARY".into()]);
    let mut buffer = String::new();
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            std::fmt::write(&mut buffer, format_args!("{value}"))
        }))
        .is_err()
    );
    let error = ScalarOnly::new(reqwest::Client::new())
        .read(value)
        .prepare()
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Build);
    assert_eq!(error.delivery(), Delivery::NotSent);
    let shown = format!("{error} {error:?}");
    assert!(shown.contains("query parameter `values` could not be formatted"));
    assert!(!shown.contains("WIRE_DISPLAY_CANARY"));
}
