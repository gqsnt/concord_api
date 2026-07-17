#[cfg(all(
    feature = "dangerous-custom-retry",
    not(feature = "dangerous-dev-tools")
))]
#[test]
fn dangerous_namespace_excludes_development_execution_support() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/fail/dangerous_namespace_escape.rs");
}
