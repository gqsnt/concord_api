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

#[test]
fn explicit_endpoints_are_bound_to_their_generated_client() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/fail/cross_client_endpoint.rs");
}

#[test]
fn arbitrary_values_are_not_generated_endpoints() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/fail/arbitrary_endpoint.rs");
}

#[test]
fn downstream_types_cannot_implement_generated_endpoint_capabilities() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/fail/forged_endpoint.rs");
}

#[test]
fn generated_stream_endpoints_have_no_metadata_terminal() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/fail/stream_terminal_absent.rs");
}

#[test]
fn generated_stream_endpoints_have_no_pagination_terminal() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/fail/stream_pagination_absent.rs");
}

#[test]
fn multipart_is_rejected_as_a_generated_response_marker() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/fail/multipart_response.rs");
}
