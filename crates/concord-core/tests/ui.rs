#[test]
fn source_only_public_surface_failures_use_trybuild() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/fail/authentication_private_material.rs");
    cases.compile_fail("tests/ui/fail/managed_reqwest_escape.rs");
    cases.compile_fail("tests/ui/fail/proxy_raw_url_escape.rs");
    cases.compile_fail("tests/ui/fail/raw_sanitized_headers.rs");
    cases.compile_fail("tests/ui/fail/status_body_traits.rs");
    cases.compile_fail("tests/ui/fail/rate_limit_context_exclusions.rs");
    cases.compile_fail("tests/ui/fail/rate_limit_key_debug_absent.rs");
    cases.compile_fail("tests/ui/fail/rate_limit_opaque_values.rs");
    cases.compile_fail("tests/ui/fail/pagination_private_state.rs");
    cases.compile_fail("tests/ui/fail/request_spec_non_send_input.rs");
    cases.compile_fail("tests/ui/fail/request_spec_metadata_signature.rs");
    #[cfg(not(feature = "records"))]
    cases.compile_fail("tests/ui/fail/response_mode_is_sealed.rs");
    #[cfg(feature = "dangerous-raw-response")]
    cases.compile_fail("tests/ui/fail/dangerous_raw_response_fields_private.rs");
}
