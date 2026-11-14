# Contracts checked under other numbers

For contributors reading the contract matrix: these contracts are checked by the compiler or by the named tests below, rather than by a test with their own number.

C1.1: Statement/part/child order is checked by `c1_3_all_twelve_statements_and_part_base_keep_the_written_order`, `c1_18_after_part_has_its_own_label_and_help`, and the structural compile-fail cases in `concord-macros/tests/diagnostics/structure/`.

C1.14: Inherited edits, removals and duplicate keys are checked by the `c1_13_table2_` resolution tests and `c10_3_two_orders_replacements_and_removal_keep_first_wire_position`.

C1.15: Literal paths, escapes and forbidden characters are checked by the `c1_20_templates_` parser tests, the rule diagnostics compiled by `c1_18_rule_diagnostics_on_stable`, and the `c2_2_`/`c2_3_` URL tests.

C1.22: Timeout inheritance is checked by `c1_3_table1_idle_timeout`, `c1_13_table2_idle_timeout`, and the `c7_6_` call-clock tests.

C3.8: Shared try handling is checked by the `c3_2_` replay tests, `c3_6_` raw-send tests, and `c4_` retry tests in the core and conformance suites, on both hand-written and generated clients.

C10.1: Absolute paths and shadowed names are checked by `c1_12_prelude_and_crate_names_may_be_shadowed`, `c1_12_internal_locals_are_hygienic`, and `every_helper_is_hygienic_for_all_call_shapes`, with the same conformance tests compiled against both client implementations.
