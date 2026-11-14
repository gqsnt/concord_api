//! Compile accepted programs and verify macro and Rust diagnostics.

/// C1.18: each structural message uses the page's three lines and original token.
#[test]
fn c1_18_messages_15_through_20_on_stable() {
    let tests = trybuild::TestCases::new();
    tests.compile_fail("tests/diagnostics/structure/*.rs");
}

/// C1.18: the compiler refuses unknown messages and incomplete bindings.
#[test]
fn c1_18_typed_catalogue_cannot_be_partially_bound() {
    let tests = trybuild::TestCases::new();
    tests.compile_fail("tests/compiler-errors/catalogue-api/*.rs");
}

/// C1.18: all 46 messages and their contextual variants on stable Rust.
#[test]
fn c1_18_rule_diagnostics_on_stable() {
    let tests = trybuild::TestCases::new();
    tests.compile_fail("tests/diagnostics/rules/*.rs");
}

/// every catalogue message also occurs in a complete docs/examples description.
#[test]
fn c1_18_api_diagnostics_on_stable() {
    let tests = trybuild::TestCases::new();
    tests.compile_fail("tests/diagnostics/api/*.rs");
}

#[test]
fn c1_10_each_request_writer_has_its_own_lexical_block() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compiler-errors/expressions/headers_*.rs");
    t.compile_fail("tests/compiler-errors/expressions/query_*.rs");
    t.compile_fail("tests/compiler-errors/expressions/body_*.rs");
    t.compile_fail("tests/compiler-errors/expressions/path_*.rs");
    t.pass("tests/accepted/module_function.rs");
    t.pass("tests/accepted/placements.rs");
}

#[test]
fn c1_11_authored_types_are_reached_from_the_invocation_module() {
    trybuild::TestCases::new().pass("tests/accepted/invocation_types.rs");
}
#[test]
fn c1_16_arm_returns_keep_the_result_context() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compiler-errors/expressions/arm_return.rs");
    t.pass("tests/accepted/returns.rs");
}
#[test]
fn c1_16_expressions_cannot_name_self() {
    trybuild::TestCases::new().compile_fail("tests/compiler-errors/expressions/self_expr.rs");
}
#[test]
fn c1_7_bare_none_needs_a_type_for_a_three_state_input() {
    trybuild::TestCases::new().compile_fail("tests/compiler-errors/expressions/bare_none.rs");
}
#[test]
fn c1_20_unit_problem_compiles_through_rustc() {
    trybuild::TestCases::new().pass("tests/accepted/unit_problem.rs");
}
#[test]
fn c1_18_the_actual_compiler_pins_each_page_message_and_place() {
    trybuild::TestCases::new().compile_fail("tests/diagnostics/catalogue/*.rs");
}

#[test]
fn c1_20_flat_expressions_reach_rustc() {
    let t = trybuild::TestCases::new();
    t.pass("tests/accepted/flat_expressions.rs");
    t.compile_fail("tests/compiler-errors/expressions/flat_too_long.rs");
}
#[test]
fn c1_7_c2_14_three_state_inputs_reach_the_wire() {
    trybuild::TestCases::new().pass("tests/accepted/three_states.rs");
}

#[test]
fn c1_18_one_refused_endpoint_keeps_two_callable_siblings_under_one_message() {
    trybuild::TestCases::new().compile_fail("tests/compiler-errors/bindings/recovery.rs");
}
#[test]
fn c1_12_internal_locals_are_hygienic() {
    let t = trybuild::TestCases::new();
    t.pass("tests/accepted/hygiene.rs");
    t.compile_fail("tests/compiler-errors/bindings/capture.rs");
}
#[test]
fn c1_16_return_precedes_adaptation() {
    trybuild::TestCases::new().pass("tests/accepted/returns.rs");
}
#[test]
fn c1_12_prelude_and_crate_names_may_be_shadowed() {
    trybuild::TestCases::new().pass("tests/accepted/shadow.rs");
}
#[test]
fn c1_12_generated_deprecated_uses_are_quiet_and_author_expressions_are_not() {
    let t = trybuild::TestCases::new();
    t.pass("tests/accepted/deprecated_node.rs");
    t.compile_fail("tests/compiler-errors/bindings/deprecated_expression.rs");
}
#[test]
fn c1_12_relative_visibility_rejects_outside_access() {
    trybuild::TestCases::new().compile_fail("tests/compiler-errors/bindings/visibility.rs");
}
#[test]
fn c1_16_rust_bounds_point_at_author_tokens() {
    trybuild::TestCases::new().compile_fail("tests/compiler-errors/spans/*.rs");
}
/// Rust reports constant-pattern conflicts and concrete trait-bound failures.
#[test]
fn stable_rust_limits_have_compiler_diagnostics() {
    trybuild::TestCases::new().compile_fail("tests/compiler-errors/limits/*.rs");
}

#[test]
fn c1_18_guessed_settings_get_contextual_help() {
    trybuild::TestCases::new().compile_fail("tests/diagnostics/suggestions/settings_*.rs");
}
#[test]
fn c1_16_rustc_resolves_bare_constructor_names_and_their_payloads() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/diagnostics/suggestions/constructor_*.rs");
    t.pass("tests/accepted/*.rs");
}

#[test]
fn problem_reader_factories_are_evaluated_before_refusal_evidence_is_checked() {
    trybuild::TestCases::new().pass("tests/accepted/problem_reader_factory.rs");
}
#[test]
fn reader_expressions_can_use_question_mark_with_a_compatible_return_type() {
    trybuild::TestCases::new().pass("tests/accepted/reader_question_mark.rs");
}
