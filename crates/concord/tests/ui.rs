#[test]
fn current_language_surface_and_diagnostics() {
    let cases = trybuild::TestCases::new();
    cases.pass("tests/ui/pass/construction/*.rs");
    cases.pass("tests/ui/pass/language/*.rs");
    cases.compile_fail("tests/ui/fail/syntax/*.rs");
    cases.compile_fail("tests/ui/fail/names/*.rs");
    // V3-E08: head-only absence capability conflicts (§10.2).
    cases.compile_fail("tests/ui/fail/absence/*.rs");
    // V3-O01: call-site deprecation of generated items (§3.5, §19.2).
    cases.compile_fail("tests/ui/fail/deprecation/*.rs");
    cases.compile_fail("tests/ui/fail/authentication/*.rs");
    cases.compile_fail("tests/ui/fail/rate_limits/*.rs");
    for path in [
        "tests/ui/fail/pagination/non_paginated_no_paginate.rs",
        "tests/ui/fail/pagination/numeric_page_limit_absent.rs",
        "tests/ui/fail/pagination/pagination_bounds.rs",
        "tests/ui/fail/pagination/pagination_destinations.rs",
        "tests/ui/fail/pagination/pagination_non_clone_input.rs",
        "tests/ui/fail/pagination/pagination_syntax.rs",
        "tests/ui/fail/pagination/pagination_types.rs",
        "tests/ui/fail/pagination/stream_pagination_absent.rs",
    ] {
        cases.compile_fail(path);
    }
    #[cfg(feature = "json")]
    for path in [
        "tests/ui/fail/pagination/pagination_modes.rs",
        "tests/ui/fail/pagination/pagination_non_page_request_representation.rs",
        "tests/ui/fail/pagination/pagination_request_body.rs",
        "tests/ui/fail/pagination/pagination_stream_body.rs",
    ] {
        cases.compile_fail(path);
    }
    #[cfg(feature = "multipart")]
    cases.compile_fail("tests/ui/fail/pagination/pagination_multipart_body.rs");
    cases.compile_fail("tests/ui/fail/privacy/*.rs");
    for path in [
        "tests/ui/fail/capabilities/absent_value_controls.rs",
        "tests/ui/fail/capabilities/accepted_bytes_requires_expose.rs",
        "tests/ui/fail/capabilities/value_conversions_closed.rs",
        // VALUE-04: the conversion set is sealed against user registration.
        "tests/ui/fail/capabilities/value_conversion_registration_sealed.rs",
        "tests/ui/fail/capabilities/bare_client_builder_value_setter_absent.rs",
        "tests/ui/fail/capabilities/bare_endpoint_value_setter_absent.rs",
        "tests/ui/fail/capabilities/bare_scope_value_setter_absent.rs",
        "tests/ui/fail/capabilities/bare_value_setter_absent.rs",
        "tests/ui/fail/capabilities/clear_optional_control_absent.rs",
        "tests/ui/fail/capabilities/flat_endpoint_constructor_absent.rs",
        "tests/ui/fail/capabilities/ancestor_control_on_endpoint_absent.rs",
        "tests/ui/fail/capabilities/any_media_is_not_media_type.rs",
        "tests/ui/fail/capabilities/non_clone_body_endpoint.rs",
        "tests/ui/fail/capabilities/runtime_endpoint_builder_capabilities_absent.rs",
        "tests/ui/fail/capabilities/runtime_endpoint_accepted_bytes_terminal_absent.rs",
        "tests/ui/fail/capabilities/runtime_endpoint_pagination_absent.rs",
        "tests/ui/fail/capabilities/runtime_accept_override_absent.rs",
        "tests/ui/fail/capabilities/with_optional_control_absent.rs",
        "tests/ui/fail/capabilities/accepted_bytes_stream_mode_rejected.rs",
        "tests/ui/fail/capabilities/storage_terminal_source_only.rs",
        "tests/ui/fail/capabilities/vector_public_header.rs",
        // DIAG-01: role-specific spanned atom and representation obligations.
        "tests/ui/fail/capabilities/atom_role_obligations.rs",
        "tests/ui/fail/capabilities/representation_role_mismatch.rs",
        "tests/ui/fail/capabilities/wrong_endpoint_setter_absent.rs",
        "tests/ui/fail/capabilities/invalid_static_accept.rs",
    ] {
        cases.compile_fail(path);
    }
    #[cfg(not(feature = "records"))]
    cases.compile_fail("tests/ui/fail/capabilities/generated_accept_override_absent.rs");
    #[cfg(feature = "records")]
    cases.compile_fail("tests/ui/fail/capabilities/generated_accept_override_absent_records.rs");
    #[cfg(feature = "csv")]
    cases.compile_fail("tests/ui/fail/capabilities/csv_policy_traits_sealed.rs");
    #[cfg(feature = "json")]
    {
        cases.compile_fail("tests/ui/fail/capabilities/no_content_request_spec.rs");
        cases.compile_fail("tests/ui/fail/capabilities/response_only_request_spec.rs");
    }
    #[cfg(all(feature = "json", feature = "multipart"))]
    cases.compile_fail("tests/ui/fail/capabilities/multipart_response.rs");
    #[cfg(all(feature = "brotli", feature = "default-tls"))]
    cases.compile_fail("tests/ui/fail/construction/*.rs");
}
