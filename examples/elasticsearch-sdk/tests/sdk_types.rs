//! Wire spellings copied from `ba10189b` source literals.

use concord_sdk_elasticsearch::{DocumentId, IndexName, Refresh};

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
