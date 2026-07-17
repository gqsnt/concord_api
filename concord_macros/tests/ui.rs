#[test]
fn unsupported_method_has_the_intentional_macro_diagnostic() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/fail/malformed.rs");
}

#[test]
fn explicit_endpoint_constructor_requires_outer_scope_values() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/fail/missing_scope_value.rs");
}

#[test]
fn normalized_generated_method_collisions_have_a_focused_diagnostic() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/fail/normalized_collision.rs");
}
