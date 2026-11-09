//! Wire spellings copied from `ba10189b` source literals.

use concord_example_kubernetes_sdk::{
    ContinueToken, NamespaceName, ObjectName, ResourceVersion, ResourceVersionMatch,
};

#[test]
fn base_literals() {
    assert_eq!(
        ResourceVersionMatch::NotOlderThan.to_string(),
        "NotOlderThan"
    );
    assert_eq!(ResourceVersionMatch::Exact.to_string(), "Exact");
    assert_eq!(
        NamespaceName::new("default").unwrap().to_string(),
        "default"
    );
    assert_eq!(ObjectName::new("app-1").unwrap().to_string(), "app-1");
    assert_eq!(ResourceVersion::new("123").to_string(), "123");
    assert_eq!(
        ContinueToken::from_server("cursor/+%=").to_string(),
        "cursor/+%="
    );
}
