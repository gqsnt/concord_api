//! Compiler regressions from the two step-5 readings.
#[test]
fn c1_18_opus_3_one_refused_endpoint_keeps_two_callable_siblings_under_one_message() {
    trybuild::TestCases::new().compile_fail("tests/readers-fail/recovery.rs");
}
#[test]
fn c1_12_a52_astra_2_internal_locals_are_hygienic() {
    let t = trybuild::TestCases::new();
    t.pass("tests/readers-pass/hygiene.rs");
    t.compile_fail("tests/readers-fail/capture.rs");
}
#[test]
fn c1_16_a53_astra_4_return_precedes_adaptation() {
    trybuild::TestCases::new().pass("tests/readers-pass/returns.rs");
}
#[test]
fn c1_12_a51_prelude_and_crate_names_may_be_shadowed() {
    trybuild::TestCases::new().pass("tests/readers-pass/shadow.rs");
}
#[test]
fn c1_12_opus_4_generated_deprecated_uses_are_quiet_and_author_expressions_are_not() {
    let t = trybuild::TestCases::new();
    t.pass("tests/readers-pass/deprecated_node.rs");
    t.compile_fail("tests/readers-fail/deprecated_expression.rs");
}
#[test]
fn c1_12_opus_13_relative_visibility_rejects_outside_access() {
    trybuild::TestCases::new().compile_fail("tests/readers-fail/visibility.rs");
}
#[test]
fn c1_16_opus_1_and_6_rust_bounds_point_at_author_tokens() {
    trybuild::TestCases::new().compile_fail("tests/reader-spans/*.rs");
}
/// Evidence for the two limits reported under COMMON-WRITER rule 4.
#[test]
fn p44_stable_rust_limits_are_preserved_as_failing_probes() {
    trybuild::TestCases::new().compile_fail("tests/reader-limitations/*.rs");
}
