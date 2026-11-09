//! Wire spellings copied from `ba10189b` source literals.

use concord_example_github_sdk::models::*;

#[test]
fn base_enum_literals() {
    assert_eq!(State::Open.to_string(), "open");
    assert_eq!(State::Closed.to_string(), "closed");
    assert_eq!(StateReason::Completed.to_string(), "completed");
    assert_eq!(StateReason::Reopened.to_string(), "reopened");
    assert_eq!(StateReason::NotPlanned.to_string(), "not_planned");
    assert_eq!(StateReason::Duplicate.to_string(), "duplicate");
    assert_eq!(AuthorAssociation::Collaborator.to_string(), "COLLABORATOR");
    assert_eq!(AuthorAssociation::Contributor.to_string(), "CONTRIBUTOR");
    assert_eq!(AuthorAssociation::FirstTimer.to_string(), "FIRST_TIMER");
    assert_eq!(
        AuthorAssociation::FirstTimeContributor.to_string(),
        "FIRST_TIME_CONTRIBUTOR"
    );
    assert_eq!(AuthorAssociation::Mannequin.to_string(), "MANNEQUIN");
    assert_eq!(AuthorAssociation::Member.to_string(), "MEMBER");
    assert_eq!(AuthorAssociation::None.to_string(), "NONE");
    assert_eq!(AuthorAssociation::Owner.to_string(), "OWNER");
    assert_eq!(ReviewState::Approved.to_string(), "APPROVED");
    assert_eq!(
        ReviewState::ChangesRequested.to_string(),
        "CHANGES_REQUESTED"
    );
    assert_eq!(ReviewState::Commented.to_string(), "COMMENTED");
    assert_eq!(ReviewState::Dismissed.to_string(), "DISMISSED");
    assert_eq!(ReviewState::Pending.to_string(), "PENDING");
    assert_eq!(RunState::Queued.to_string(), "queued");
    assert_eq!(RunState::InProgress.to_string(), "in_progress");
    assert_eq!(RunState::Completed.to_string(), "completed");
    assert_eq!(RunState::Waiting.to_string(), "waiting");
    assert_eq!(RunState::Requested.to_string(), "requested");
    assert_eq!(RunState::Pending.to_string(), "pending");
    assert_eq!(Conclusion::Success.to_string(), "success");
    assert_eq!(Conclusion::Failure.to_string(), "failure");
    assert_eq!(Conclusion::Neutral.to_string(), "neutral");
    assert_eq!(Conclusion::Cancelled.to_string(), "cancelled");
    assert_eq!(Conclusion::Skipped.to_string(), "skipped");
    assert_eq!(Conclusion::TimedOut.to_string(), "timed_out");
    assert_eq!(Conclusion::ActionRequired.to_string(), "action_required");
    assert_eq!(Conclusion::Stale.to_string(), "stale");
    assert_eq!(FileStatus::Added.to_string(), "added");
    assert_eq!(FileStatus::Removed.to_string(), "removed");
    assert_eq!(FileStatus::Modified.to_string(), "modified");
    assert_eq!(FileStatus::Renamed.to_string(), "renamed");
    assert_eq!(FileStatus::Copied.to_string(), "copied");
    assert_eq!(FileStatus::Changed.to_string(), "changed");
    assert_eq!(FileStatus::Unchanged.to_string(), "unchanged");
}

#[test]
fn decoded_values_keep_their_redaction_without_a_debug_bound() {
    use concord::reqwest::{StatusCode, Url, header::HeaderMap};
    use concord_example_github_sdk::meta::{ResponseMeta, WithMeta};

    struct Unprintable;
    let value = WithMeta {
        value: Unprintable,
        meta: ResponseMeta::from_head(
            StatusCode::OK,
            &HeaderMap::new(),
            &Url::parse("https://api.github.com").unwrap(),
        ),
    };
    assert!(format!("{value:?}").starts_with("WithMeta { value: <redacted>, meta: "));
    assert!(format!("{value:#?}").starts_with("WithMeta {\n    value: <redacted>,\n"));
}
