//! Stable compiler snapshots for the six structure messages.

/// C1.18: every step-1 message uses the page's three lines and original token.
#[test]
fn c1_18_messages_15_through_20_on_stable() {
    let tests = trybuild::TestCases::new();
    tests.compile_fail("tests/ui/*.rs");
}

/// C1.18: the compiler refuses unknown messages and incomplete bindings.
#[test]
fn c1_18_o6_typed_catalogue_cannot_be_partially_bound() {
    let tests = trybuild::TestCases::new();
    tests.compile_fail("tests/typed/*.rs");
}

/// C1.18: all 46 messages and their contextual variants on stable Rust.
#[test]
fn c1_18_steps_two_through_four_on_stable() {
    let tests = trybuild::TestCases::new();
    tests.compile_fail("tests/semantic/*.rs");
}

/// A39: every catalogue message also occurs in a complete docs/apis description.
#[test]
fn c1_18_real_author_mistakes_on_stable() {
    let tests = trybuild::TestCases::new();
    tests.compile_fail("tests/real/*.rs");
}
