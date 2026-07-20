#[test]
fn source_only_public_surface_failures_use_trybuild() {
    let cases = trybuild::TestCases::new();
    #[cfg(not(feature = "dangerous-dev-tools"))]
    cases.compile_fail("tests/ui/fail/development_namespace_escape.rs");
    cases.compile_fail("tests/ui/fail/generated_executor_escape.rs");
    cases.compile_fail("tests/ui/fail/authentication_private_material.rs");
    cases.compile_fail("tests/ui/fail/managed_reqwest_escape.rs");
    cases.compile_fail("tests/ui/fail/proxy_raw_url_escape.rs");
    cases.compile_fail("tests/ui/fail/raw_sanitized_headers.rs");
    cases.compile_fail("tests/ui/fail/status_body_traits.rs");
    cases.compile_fail("tests/ui/fail/rate_limit_private_state.rs");
    cases.compile_fail("tests/ui/fail/rate_limit_context_exclusions.rs");
    cases.compile_fail("tests/ui/fail/rate_limit_key_debug_absent.rs");
    cases.compile_fail("tests/ui/fail/rate_limit_opaque_values.rs");
    cases.compile_fail("tests/ui/fail/pagination_private_state.rs");
    #[cfg(not(feature = "json"))]
    cases.compile_fail("tests/ui/fail/no_content_request_spec.rs");
    #[cfg(not(feature = "json"))]
    cases.compile_fail("tests/ui/fail/response_only_request_spec.rs");
    cases.compile_fail("tests/ui/fail/request_spec_non_send_input.rs");
    cases.compile_fail("tests/ui/fail/request_spec_metadata_signature.rs");
    #[cfg(not(feature = "records"))]
    cases.compile_fail("tests/ui/fail/response_mode_is_sealed.rs");
    #[cfg(not(feature = "json"))]
    cases.compile_fail("tests/ui/fail/json_spec_feature_absent.rs");
    #[cfg(not(feature = "multipart"))]
    cases.compile_fail("tests/ui/fail/multipart_spec_feature_absent.rs");
    #[cfg(feature = "records")]
    cases.compile_fail("tests/ui/fail/records_pagination_absent.rs");
}
