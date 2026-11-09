//! Compile-pass regressions isolated from host derives so mutations reach the fixtures.
#[test]
fn compile_pass_hygiene() {
    trybuild::TestCases::new().pass("tests/pass_derive_error/*.rs");
}
