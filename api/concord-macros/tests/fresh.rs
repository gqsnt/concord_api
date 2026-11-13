//! Regressions from the first client written using rustdoc alone.
#[test]
fn c1_18_a39_a70_m6_guessed_settings_get_contextual_help() {
    trybuild::TestCases::new().compile_fail("tests/fresh-fail/settings_*.rs");
}
#[test]
fn c1_16_m6_rustc_resolves_bare_constructor_names_and_their_payloads() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/fresh-fail/constructor_*.rs");
    t.pass("tests/fresh-pass/*.rs");
}
