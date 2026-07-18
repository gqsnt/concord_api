#[test]
fn source_only_public_surface_failures_use_trybuild() {
    let cases = trybuild::TestCases::new();
    #[cfg(not(feature = "dangerous-dev-tools"))]
    cases.compile_fail("tests/ui/fail/development_namespace_escape.rs");
    cases.compile_fail("tests/ui/fail/generated_executor_escape.rs");
    cases.compile_fail("tests/ui/fail/authentication_private_material.rs");
    cases.compile_fail("tests/ui/fail/response_terminal_requires_capability.rs");
    cases.compile_fail("tests/ui/fail/managed_reqwest_escape.rs");
    cases.compile_fail("tests/ui/fail/proxy_raw_url_escape.rs");
    cases.compile_fail("tests/ui/fail/raw_sanitized_headers.rs");
    cases.compile_fail("tests/ui/fail/status_body_traits.rs");
    cases.compile_fail("tests/ui/fail/rate_limit_private_state.rs");
    cases.compile_fail("tests/ui/fail/rate_limit_context_exclusions.rs");
    cases.compile_fail("tests/ui/fail/rate_limit_key_debug_absent.rs");
    cases.compile_fail("tests/ui/fail/rate_limit_opaque_values.rs");
}
