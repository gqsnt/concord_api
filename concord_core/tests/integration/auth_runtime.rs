use crate::support;

#[test]
fn auth_runtime_harness_is_available() {
    let auth = support::FakeAuthProvider::default();
    auth.events.record("auth_prepare");
    support::assert_event_order(&auth.events.snapshot(), &["auth_prepare"]);
}
