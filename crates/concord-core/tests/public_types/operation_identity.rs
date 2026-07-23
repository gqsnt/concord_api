use std::hash::Hash;

use concord_core::advanced::OperationId;

fn assert_identity_traits<T: Clone + Copy + Eq + Hash + Send + Sync + 'static>() {}

#[test]
fn public_operation_identity_has_only_opaque_structural_traits() {
    assert_identity_traits::<OperationId>();
}
