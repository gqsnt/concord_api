#[test]
fn final_language_and_named_surface_compile_without_generated_import_conventions() {
    let cases = trybuild::TestCases::new();
    cases.pass("tests/ui/pass/construction/*.rs");
    cases.pass("tests/ui/pass/language/*.rs");
}

#[test]
fn malformed_syntax_and_name_resolution_have_focused_diagnostics() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/fail/syntax/*.rs");
    cases.compile_fail("tests/ui/fail/names/*.rs");
}

#[test]
fn authentication_and_rate_limit_contracts_reject_invalid_declarations() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/fail/authentication/*.rs");
    cases.compile_fail("tests/ui/fail/rate_limits/*.rs");
}

#[test]
fn pagination_contracts_reject_invalid_bindings_modes_and_terminals() {
    let cases = trybuild::TestCases::new();
    for path in [
        "tests/ui/fail/pagination/non_paginated_no_paginate.rs",
        "tests/ui/fail/pagination/pagination_bounds.rs",
        "tests/ui/fail/pagination/pagination_syntax.rs",
        "tests/ui/fail/pagination/pagination_types.rs",
        "tests/ui/fail/pagination/stream_pagination_absent.rs",
    ] {
        cases.compile_fail(path);
    }
    #[cfg(feature = "json")]
    cases.compile_fail("tests/ui/fail/pagination/pagination_modes.rs");
}

#[test]
fn generated_privacy_and_capability_boundaries_are_compiler_enforced() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/fail/privacy/*.rs");
    for path in [
        "tests/ui/fail/capabilities/non_clone_body_endpoint.rs",
        "tests/ui/fail/capabilities/runtime_endpoint_builder_capabilities_absent.rs",
        "tests/ui/fail/capabilities/runtime_endpoint_pagination_absent.rs",
        "tests/ui/fail/capabilities/storage_terminal_source_only.rs",
        "tests/ui/fail/capabilities/vector_public_header.rs",
        "tests/ui/fail/capabilities/wrong_endpoint_setter_absent.rs",
    ] {
        cases.compile_fail(path);
    }
    #[cfg(feature = "csv")]
    cases.compile_fail("tests/ui/fail/capabilities/csv_policy_traits_sealed.rs");
    #[cfg(all(feature = "json", feature = "dangerous-raw-response"))]
    {
        cases.compile_fail("tests/ui/fail/capabilities/no_content_request_spec.rs");
        cases.compile_fail("tests/ui/fail/capabilities/response_only_request_spec.rs");
    }
    #[cfg(all(
        feature = "json",
        feature = "multipart",
        feature = "dangerous-raw-response"
    ))]
    cases.compile_fail("tests/ui/fail/capabilities/multipart_response.rs");
    #[cfg(feature = "dangerous-raw-response")]
    cases.compile_fail("tests/ui/fail/capabilities/raw_stream_mode_rejected.rs");
}

#[cfg(all(feature = "brotli", feature = "default-tls"))]
#[test]
fn configured_client_has_no_positional_constructor() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/fail/construction/*.rs");
}
