#[cfg(not(any(
    feature = "dangerous-custom-retry",
    feature = "dangerous-dev-tools",
    feature = "dangerous-raw-response"
)))]
#[test]
fn default_surface_exposes_no_dangerous_capability() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/fail/dangerous_custom_retry_absent.rs");
    cases.compile_fail("tests/ui/fail/dangerous_development_absent.rs");
    cases.compile_fail("tests/ui/fail/dangerous_raw_response_absent.rs");
}

#[cfg(all(
    feature = "dangerous-custom-retry",
    not(feature = "dangerous-dev-tools"),
    not(feature = "dangerous-raw-response")
))]
#[test]
fn custom_retry_excludes_development_and_raw_response_surfaces() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/fail/dangerous_development_absent.rs");
    cases.compile_fail("tests/ui/fail/dangerous_raw_response_absent_with_custom_retry.rs");
}

#[cfg(all(
    feature = "dangerous-dev-tools",
    not(feature = "dangerous-custom-retry"),
    not(feature = "dangerous-raw-response")
))]
#[test]
fn development_tools_exclude_stable_custom_retry_and_raw_response_surfaces() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/fail/dangerous_custom_retry_absent.rs");
    cases.compile_fail("tests/ui/fail/dangerous_raw_response_absent.rs");
    cases.compile_fail("tests/ui/fail/dangerous_dev_stable_namespace_escape.rs");
    cases.compile_fail("tests/ui/fail/dangerous_dev_dangerous_namespace_escape.rs");
}

#[cfg(all(
    feature = "dangerous-raw-response",
    not(feature = "dangerous-custom-retry"),
    not(feature = "dangerous-dev-tools")
))]
#[test]
fn raw_response_flag_excludes_custom_retry_and_development_surfaces() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/fail/dangerous_custom_retry_absent_with_raw_response.rs");
    cases.compile_fail("tests/ui/fail/dangerous_development_absent.rs");
    cases.compile_fail("tests/ui/fail/dangerous_raw_response_fields_private.rs");
}
