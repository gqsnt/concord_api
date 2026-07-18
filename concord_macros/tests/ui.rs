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
fn endpoint_setters_require_the_generated_prelude_import() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/fail/missing_request_prelude.rs");
}

#[test]
fn generated_request_extension_traits_are_sealed() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/fail/forged_request_extension.rs");
}

#[test]
fn generated_request_extension_traits_are_endpoint_specific() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/fail/wrong_endpoint_request_extension.rs");
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

#[test]
fn duplicate_clients_are_rejected_in_one_invocation() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/fail/duplicate_client.rs");
}

#[test]
fn unknown_public_value_references_are_rejected() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/fail/unknown_public_reference.rs");
}

#[test]
fn ambiguous_visible_scope_references_are_rejected() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/fail/ambiguous_scope_reference.rs");
}

#[test]
fn invalid_custom_method_tokens_are_rejected() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/fail/invalid_custom_method.rs");
}

#[test]
fn vector_values_are_rejected_for_public_headers() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/fail/vector_public_header.rs");
}

#[test]
fn generated_custom_provider_handles_do_not_expose_direct_set() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/fail/phase6_custom_set_absent.rs");
}

#[test]
fn credential_references_do_not_expose_mutation() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/fail/phase6_credential_ref_mutation_absent.rs");
}

#[test]
fn phase7_rate_limit_diagnostics_are_source_only_and_focused() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/fail/phase7_rate_optional_dimension.rs");
    cases.compile_fail("tests/ui/fail/phase7_rate_invalid_cost.rs");
    cases.compile_fail("tests/ui/fail/phase7_rate_off_exclusive.rs");
    cases.compile_fail("tests/ui/fail/phase7_rate_clause_order.rs");
    cases.compile_fail("tests/ui/fail/phase7_rate_undefined_attachment.rs");
    cases.compile_fail("tests/ui/fail/phase7_rate_secret_dimension.rs");
}

#[test]
fn phase7_dynamic_dimensions_require_display_without_a_clone_contract() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/fail/phase7_rate_display_required.rs");
}
