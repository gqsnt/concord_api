#[test]
fn named_generated_surface_compiles_without_generated_import_conventions() {
    let cases = trybuild::TestCases::new();
    cases.pass("tests/ui/pass/named_surface.rs");
    cases.pass("tests/ui/pass/configured_client_builder.rs");
    cases.pass("tests/ui/pass/dsl_v2.rs");
    cases.pass("tests/ui/pass/runtime_endpoint.rs");
}

#[test]
fn invalid_generated_capabilities_are_absent_or_focused() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/fail/non_paginated_no_paginate.rs");
    cases.compile_fail("tests/ui/fail/wrong_client_endpoint.rs");
    cases.compile_fail("tests/ui/fail/storage_terminal_source_only.rs");
    cases.compile_fail("tests/ui/fail/non_clone_body_endpoint.rs");
    cases.compile_fail("tests/ui/fail/duplicate_clause.rs");
    cases.compile_fail("tests/ui/fail/inferred_body.rs");
    cases.compile_fail("tests/ui/fail/multiple_bodies.rs");
    cases.compile_fail("tests/ui/fail/invalid_body_attribute.rs");
    cases.compile_fail("tests/ui/fail/pascal_source_name.rs");
    cases.compile_fail("tests/ui/fail/duplicate_contract_id.rs");
    cases.compile_fail("tests/ui/fail/custom_retry_dsl_removed.rs");
    cases.compile_fail("tests/ui/fail/prepared_endpoint_alias_deprecated.rs");
    #[cfg(feature = "dangerous-raw-response")]
    cases.compile_fail("tests/ui/fail/raw_stream_mode_rejected.rs");
}

#[test]
fn configured_client_secrets_require_explicit_wrapping() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/fail/secret_requires_explicit_wrapper.rs");
}

#[cfg(all(feature = "brotli", feature = "default-tls"))]
#[test]
fn configured_client_has_no_positional_constructor() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/fail/configured_client_no_new.rs");
}
