#[test]
fn source_only_public_surface_failures_use_trybuild() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/fail/benchmark_conversion_token_private.rs");
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
    #[cfg(not(feature = "multipart"))]
    cases.compile_fail("tests/ui/fail/multipart_spec_feature_absent.rs");
    #[cfg(feature = "records")]
    cases.compile_fail("tests/ui/fail/records_pagination_absent.rs");
}

#[cfg(not(any(feature = "dangerous-custom-retry", feature = "dangerous-raw-response")))]
#[test]
fn default_surface_exposes_no_dangerous_capability() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/fail/dangerous_custom_retry_absent.rs");
    cases.compile_fail("tests/ui/fail/dangerous_raw_response_absent.rs");
}

#[cfg(all(
    feature = "dangerous-custom-retry",
    not(feature = "dangerous-raw-response")
))]
#[test]
fn custom_retry_excludes_raw_response_surface() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/fail/dangerous_raw_response_absent_with_custom_retry.rs");
}

#[cfg(all(
    feature = "dangerous-raw-response",
    not(feature = "dangerous-custom-retry")
))]
#[test]
fn raw_response_flag_excludes_custom_retry_surface() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/fail/dangerous_custom_retry_absent_with_raw_response.rs");
    cases.compile_fail("tests/ui/fail/dangerous_raw_response_fields_private.rs");
}

#[cfg(not(feature = "multipart"))]
#[test]
fn multipart_public_surface_is_absent_without_its_feature() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/fail/multipart_feature_absent.rs");
}
