//! Wire spellings copied from `ba10189b` source literals.

use concord_sdk_elasticsearch::{Action, DocumentId, IndexName, Refresh};

#[test]
fn base_literals() {
    assert_eq!(Refresh::True.to_string(), "true");
    assert_eq!(Refresh::False.to_string(), "false");
    assert_eq!(Refresh::WaitFor.to_string(), "wait_for");
    assert_eq!(IndexName::new("products").unwrap().to_string(), "products");
    assert_eq!(
        DocumentId::new("id/+% café").unwrap().to_string(),
        "id/+% café"
    );
}

#[test]
fn base_action_literals() {
    for (action, literal) in [
        (Action::Index, "index"),
        (Action::Create, "create"),
        (Action::Update, "update"),
        (Action::Delete, "delete"),
    ] {
        assert_eq!(action.to_string(), literal, "{action:?}");
    }
}
