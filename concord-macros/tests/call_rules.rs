//! Compile-time rules for the generated call and endpoint types.

#[test]
fn c1_12_authored_types_resolve_at_the_invocation() {
    trybuild::TestCases::new().pass("tests/accepted/endpoint_type_names.rs");
}

#[test]
fn c1_11_c8_6_c8_8_calls_keep_their_endpoint_and_eligibility() {
    trybuild::TestCases::new().compile_fail("tests/compiler-errors/calls/*.rs");
}

#[test]
fn c10_2_clients_handles_calls_and_their_results_are_unpin_whatever_their_inputs() {
    trybuild::TestCases::new().pass("tests/accepted/unpin_inputs.rs");
}

#[test]
fn c6_4_endpoint_name_is_public_and_an_inherent_name_does_not_replace_it() {
    trybuild::TestCases::new().pass("tests/accepted/endpoint_name.rs");
}

#[test]
fn c10_1_a_shadowed_primitive_name_does_not_reach_generated_code() {
    trybuild::TestCases::new().pass("tests/accepted/shadowed_str.rs");
}

#[test]
fn c1_5_a_client_name_is_read_after_normalization() {
    trybuild::TestCases::new().pass("tests/accepted/nfc_client.rs");
}

#[test]
fn c10_1_c2_4_names_of_the_author_s_module_do_not_reach_generated_code() {
    trybuild::TestCases::new().pass("tests/accepted/hostile_names.rs");
}
