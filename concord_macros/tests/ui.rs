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
fn request_only_multipart_fails_through_the_response_specification_bound() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/fail/multipart_response.rs");
}

#[test]
fn breaking_dsl_migration_diagnostics_are_focused() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/fail/second_client.rs");
    cases.compile_fail("tests/ui/fail/legacy_var.rs");
    cases.compile_fail("tests/ui/fail/child_inside_client.rs");
    cases.compile_fail("tests/ui/fail/root_client_config.rs");
    cases.compile_fail("tests/ui/fail/vars_after_credentials.rs");
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
    cases.compile_fail("tests/ui/fail/authentication_custom_set_absent.rs");
}

#[test]
fn credential_references_do_not_expose_mutation() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/fail/authentication_credential_ref_mutation_absent.rs");
}

#[test]
fn rate_limit_diagnostics_are_source_only_and_focused() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/fail/rate_limit_optional_dimension.rs");
    cases.compile_fail("tests/ui/fail/rate_limit_invalid_cost.rs");
    cases.compile_fail("tests/ui/fail/rate_limit_off_exclusive.rs");
    cases.compile_fail("tests/ui/fail/rate_limit_clause_order.rs");
    cases.compile_fail("tests/ui/fail/rate_limit_undefined_attachment.rs");
    cases.compile_fail("tests/ui/fail/rate_limit_secret_dimension.rs");
    cases.compile_fail("tests/ui/fail/rate_limit_removed_syntax.rs");
}

#[test]
fn lexical_rate_limits_do_not_require_display_for_stored_values() {
    let cases = trybuild::TestCases::new();
    cases.pass("tests/ui/pass/rate_limit_no_display.rs");
}

#[test]
fn pagination_diagnostics_are_source_only_and_focused() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/fail/pagination_syntax.rs");
    cases.compile_fail("tests/ui/fail/pagination_types.rs");
    cases.compile_fail("tests/ui/fail/pagination_modes.rs");
    cases.compile_fail("tests/ui/fail/pagination_bounds.rs");
    cases.compile_fail("tests/ui/fail/pagination_method_absence.rs");
}

#[test]
fn prepared_endpoint_capabilities_are_exact_and_private() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/fail/prepared_endpoint_pagination_absent.rs");
    cases.compile_fail("tests/ui/fail/prepared_endpoint_builder_methods_absent.rs");
    cases.compile_fail("tests/ui/fail/prepared_endpoint_fields_private.rs");
    cases.compile_fail("tests/ui/fail/prepared_endpoint_no_prelude_export.rs");
    cases.compile_fail("tests/ui/fail/dangerous_raw_prepared_absent.rs");
}

#[test]
fn generated_client_runtime_internals_cannot_be_supplied_downstream() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/fail/generated_executor_escape.rs");
}

#[test]
fn generated_request_specifications_require_request_capabilities() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/fail/no_content_request_spec.rs");
    cases.compile_fail("tests/ui/fail/response_only_request_spec.rs");
}

#[test]
fn generated_stream_endpoints_have_no_raw_response_terminal() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/fail/dangerous_raw_stream_absent.rs");
}
