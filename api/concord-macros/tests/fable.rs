//! Regressions from the sentence-by-sentence reading of M4.
#[test]
fn c1_10_fable_1_each_request_writer_has_its_own_lexical_block() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/fable-fail/headers_*.rs");
    t.compile_fail("tests/fable-fail/query_*.rs");
    t.compile_fail("tests/fable-fail/body_*.rs");
    t.compile_fail("tests/fable-fail/path_*.rs");
    t.pass("tests/fable-pass/module_function.rs");
    t.pass("tests/fable-pass/placements.rs");
}

#[test]
fn c1_11_fable_13_authored_types_are_reached_from_the_invocation_module() {
    trybuild::TestCases::new().pass("tests/fable-pass/invocation_types.rs");
}
#[test]
fn c1_16_a69_fable_5_arm_returns_keep_the_result_context() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/fable-fail/arm_return.rs");
    t.pass("tests/readers-pass/returns.rs");
}
#[test]
fn c1_16_fable_11_expressions_cannot_name_self() {
    trybuild::TestCases::new().compile_fail("tests/fable-fail/self_expr.rs");
}
#[test]
fn c1_7_fable_8_bare_none_needs_a_type_for_a_three_state_input() {
    trybuild::TestCases::new().compile_fail("tests/fable-fail/bare_none.rs");
}
#[test]
fn c1_20_fable_15_unit_problem_compiles_through_rustc() {
    trybuild::TestCases::new().pass("tests/fable-pass/unit_problem.rs");
}
#[test]
fn c1_18_a68_the_actual_compiler_pins_each_page_message_and_place() {
    trybuild::TestCases::new().compile_fail("tests/messages-page/*.rs");
}

#[test]
fn c1_20_a71_fable_12_the_readers_flat_expression_probes_reach_rustc() {
    let t = trybuild::TestCases::new();
    t.pass("tests/fable-pass/flat_expressions.rs");
    t.compile_fail("tests/fable-fail/flat_too_long.rs");
}
#[test]
fn c1_7_c2_14_fable_8_the_readers_three_states_reach_the_wire() {
    trybuild::TestCases::new().pass("tests/fable-pass/three_states.rs");
}
