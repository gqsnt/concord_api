# 4.2 — maintained regressions

Every row runs in the maintained gate (`scripts/verify.sh`, the one inventory CI calls on Linux and
Windows). "Qualified" means the fix (or the checked behaviour) was removed alone and this test failed
on its intended assertion, then the source was restored byte for byte.

| ID | Test (path :: name) | Gate step | Asserts | Control / qualification |
|---|---|---|---|---|
| F01 | `checks/clone-hygiene/tests/clone_hygiene.rs` :: `mixed_declarations_compile_and_build_in_both_orders`, `non_clone_controls_build`, `clone_requests_clone_to_the_same_request` | `test` (workspace) | same-printed `$crate::Payload` from two crates: declarations compile, requests build (required, optional, body), Clone requests clone to equal requests | revert → E0277 at `forward` declaration |
| F01 | `checks/clone-hygiene/tests/clone_hygiene.rs` :: `clone_of_non_clone_request_is_rejected` (trybuild, 4 cases) | `test` | `.clone()` of a request holding the non-Clone value fails, naming that type (forward, reverse/body, both-non-Clone/optional, direct paths) | intended negative |
| F02 | `checks/clone-hygiene/tests/type_macros.rs` :: `type_macro_outputs_decode` | `test` | absolute/relative cross-crate type-macro outputs in `()`, `[]`, `{}` and a type-macro body decode over localhost | — |
| F02 | `crates/concord/tests/output_types.rs` :: `macro_outputs_are_the_macros_types` | `test` + feature lanes | 3 delimiters, `crate::`/`self::`/`r#mod::` paths, nested, alias, unit, array, generic, body + output; `Prepared<T>` type-checked | revert → "expected curly braces" |
| F02 | `crates/concord/tests/ui/output_type_macro_errors.rs` | `test` (ui) | removed spellings around a type macro keep their messages; braced macro taking the block gets a hint | snapshot |
| F03 | `crates/concord/tests/ui/migration_literal_advice.rs` + `applied_advice.rs` :: `applied_literal_advice_keeps_the_const_value` | `test` | advice keeps `"a :: b"` / `r#"x , y"#`; the applied text compiles with `Payload<6>`/`<5>` and is checked against the snapshot | revert → snapshot shows `"a::b"` |
| F03 | `concord-macros` unit :: `syntax::tests::{literals_keep_their_contents, types_are_spaced_as_written}`, `emit::tests::documented_types_keep_literal_contents` | `test` | renderer keeps every literal kind and raw identifiers; round-trips; generated rustdoc keeps literals | revert → 3 failures |
| F04 | `crates/concord/tests/ui/migration_raw_advice.rs` + `applied_advice.rs` :: `applied_raw_advice_keeps_the_raw_name` | `test` | `body r#type?: String` / `body r#match: String`; applied text compiles and prepares | revert → snapshot shows `body type?` |
| M01 | `benches/criterion/src/tests.rs` (10 tests: `registered_pairs_match`, six comparator controls, S3 client-default pin, …) | `test` | 47 registered comparisons equal at the prepared/decoded stage; altered Accept, repeated value/order, URL/method, body/absence, native config/sensitivity detected | lane report |
| T01 | `examples/native/tests/metadata_stage.rs` (3 tests) | `test` | 2xx head refused before a withheld body → `Metadata`, response recoverable; valid → decodes; bad body → `Decode`; Status recoverable | head check skipped → intended "Metadata before body" assertion |
| I20-A | `crates/concord/tests/lifecycle.rs` :: `stages_cancellation_and_controls` | `test` | prepare no I/O; one send one request; cancel before head (no format events) and during body (context dropped); success/short/refuse/status | — |
| D16-sol | `crates/concord/tests/d16_followup.rs` :: `nested_owners_and_sibling_defaults`, `compiler_rejections` (3 trybuild) | `test` | hand-calculated multi-owner composition; resolution diagnostics | wrong expectation fails |
| F03 | `crates/concord/tests/ui/migration_token_advice.rs` + `applied_advice.rs` :: `applied_token_advice_keeps_the_payload_type` | `test` | macro input token for token (`. .`, `a < b < c > > d`, `1. . 2`): applied advice selects the same arm (bytes 1/3/5); forwarded payloads named `T` | R1, R4, R6 |
| F03 | `crates/concord/tests/forwarded_types.rs` :: `forwarded_fragments_keep_their_grouping` | `test` | forwarded fragments mean what Rust says in query/body/optional/output types, initializers, const arguments, array lengths; type fragments in turbofish/qualified/`as` stay types (`deny(unused_parens)`) | R3, R7, R8, R9 |
| M02 | `scripts/verify.sh` step `test-examples-isolated`; `scripts/consumers.sh` no-extras check | `test-examples-isolated`, `consumers` | every example alone (`cargo test -p`, ≥ 1 test); no-extras consumer has no extras feature | renamed without `extras` fails; unlisted example fails |
| M02 | `scripts/tests/output-dirs.sh` | `output-dirs` | packaged steps recreate only marked output, refuse others, detect stale sources | 14 controls |
| M03 | `scripts/tests/test_check_ci.py` | `tool-tests` | structural guard: 16 positive, 47 negative workflow cases | 8 real-workflow mutants rejected |
| M04 | `scripts/tests/test_m4_retired.py` | `tool-tests` | retired helper refuses to run; no maintained caller | old helper restored / planted caller fail |
| D01 | `docs/AUTHOR_GUIDE.md` §11.1 doctest (via `concord-example-quickstart`), `concord-core` `IntoValue` doctest pair | `test`, `test-core` | default moved into a scope with pinned siblings and pair order; E0283 inference note | sibling-pin and override removals fail |
