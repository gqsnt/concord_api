//! Compile-time rules for the generated call and endpoint types.

#[test]
fn c1_12_authored_types_resolve_at_the_invocation() {
    trybuild::TestCases::new().pass("tests/accepted/endpoint_type_names.rs");
}

#[test]
fn c1_11_c8_6_c8_8_calls_keep_their_endpoint_and_eligibility() {
    trybuild::TestCases::new().compile_fail("tests/compiler-errors/calls/*.rs");
}
