//! Wire spellings copied from `ba10189b` source literals, with SDK-owned token redaction.

use concord_sdk_gitlab::{
    IssueOrder, IssueState, IssueStatus, PerPage, PrivateToken, ProjectId, ProjectRef, SortOrder,
};

#[test]
fn base_enum_literals() {
    for (value, text) in [
        (IssueState::Opened, "opened"),
        (IssueState::Closed, "closed"),
        (IssueState::All, "all"),
    ] {
        assert_eq!(value.to_string(), text);
    }
    for (value, text) in [
        (IssueOrder::CreatedAt, "created_at"),
        (IssueOrder::UpdatedAt, "updated_at"),
        (IssueOrder::Priority, "priority"),
        (IssueOrder::DueDate, "due_date"),
        (IssueOrder::RelativePosition, "relative_position"),
        (IssueOrder::LabelPriority, "label_priority"),
        (IssueOrder::MilestoneDue, "milestone_due"),
        (IssueOrder::Popularity, "popularity"),
        (IssueOrder::Weight, "weight"),
    ] {
        assert_eq!(value.to_string(), text);
    }
    for (value, text) in [(SortOrder::Asc, "asc"), (SortOrder::Desc, "desc")] {
        assert_eq!(value.to_string(), text);
    }
    assert_eq!(IssueStatus::Opened.to_string(), "opened");
    assert_eq!(IssueStatus::Closed.to_string(), "closed");
    assert_eq!(
        IssueStatus::Other("future/value".into()).to_string(),
        "future/value"
    );
    assert_eq!(
        ProjectRef::from(ProjectId::new(42).unwrap()).to_string(),
        "42"
    );
    assert_eq!(
        ProjectRef::path("group/sub/project").unwrap().to_string(),
        "group/sub/project"
    );
    assert_eq!(PerPage::new(100).unwrap().to_string(), "100");
}

#[test]
fn token_debug_withholds_the_header_in_both_modes() {
    let token = PrivateToken::new("TOKEN_SECRET").unwrap();
    assert_eq!(format!("{token:?}"), "PrivateToken(<redacted>)");
    assert_eq!(format!("{token:#?}"), "PrivateToken(\n    <redacted>,\n)");
    let header = token.into_header_value();
    assert!(header.is_sensitive());
    assert_eq!(header.to_str().unwrap(), "TOKEN_SECRET");
}
