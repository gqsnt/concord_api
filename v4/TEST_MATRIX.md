# V4 acceptance matrix

Classification: **required** (executable test exists), **unsupported** (deliberately not offered;
how to do it instead), **deferred**. Test paths are relative to `crates/concord/tests/` unless they
start with another directory. **Status** is bound to the verification receipt
[`refinement/evidence/verify_b0fe26e.txt`](refinement/evidence/verify_b0fe26e.txt): `scripts/verify.sh
--msrv` on code revision `b0fe26e4ec3d0c2ac7e1ebb4216d05a38a4aff4e`, clean tree, Linux x86_64, rustc
1.97.1 and 1.85.1, all 11 steps PASS. Commits after `b0fe26e` change only records under `v4/` (this
file, `refinement/`), which no build or test reads; `refinement/FINAL.md` lists them and shows the
code is identical. Earlier alpha receipts in `evidence/GATES.md` are historical. Rows added or changed by the refinement are in the last section
or marked *(refined)*. The final campaign's rows (native header names, representation statements,
the Riot example) are in the last three sections; their status is bound to the receipt named in
`finalization/STATUS.md`, and rows of earlier sections that the final campaign changed are marked
*(final)*.

## Minimal surface

| ID | Class | Expected | Test | Status |
|---|---|---|---|---|
| M01 | required | zero-input client; `GET /v1/health` → typed JSON over loopback | `slice.rs::m01_health_round_trip` | PASS |
| M02 | required | application `reqwest::Client` used as is (default headers, UA on the wire) | `slice.rs::m02_application_client_is_used`, `composition.rs::client_default_headers_fill_only_missing_names` | PASS |
| M03 | required | native request available before sending; server sees nothing | `slice.rs::m03_prepare_does_not_contact_the_server` | PASS |
| M04 | required | native response without status check or decoding | `slice.rs::m04_send_raw_returns_the_native_response`, `bodies.rs::r08_raw_response_streaming` | PASS |
| M05 | required | one `send` = one `execute`; failures are not retried (server sees one request / one connection) | `slice.rs::m05_failure_is_not_retried`, `representations.rs::m05_transport_failure_is_one_attempt`, `representations.rs::transport_error_message` | PASS |
| — | required | native parts modified, executed by the app, decoded as declared | `slice.rs::into_parts_then_decode` | PASS |

## Values and builders

| ID | Class | Expected | Test | Status |
|---|---|---|---|---|
| V01 | required *(refined)* | required client values are fields of the generated `{Client}Config` (one compile error naming every missing field); scope/endpoint required values are positional; `""` valid | `values.rs::v01_required_root_value_empty_string_is_valid`, `ui/config_missing_field.rs`, `ui/missing_required_argument.rs`, `hygiene.rs::many_required_values` | PASS |
| V02 | required (reset: unsupported) | initializer used when unset; `set_` replaces it. No `reset_` (D3) — keep the original handle | `values.rs::v02_initialized_value_and_override` | PASS |
| V03 | required | initializer runs once per `new`/scope entry/endpoint call, in declaration order, even if immediately overridden; never on clone, setter, prepare or send | `values.rs::v03_initializer_evaluation_points`, `values.rs::v03_v09_initializer_order_and_clone_retention` | PASS |
| V04 | required | absent optional: no occurrence; present `""`: `key=` | `values.rs::v04_optional_absent_vs_present_empty` | PASS |
| V05 | required | required value with optional payload stays required; spelling is the type's `Display`; bare `Option<T>` has no wire spelling (compile error) | `values.rs::v05_required_optional_payload_is_not_omission`, `ui/option_has_no_wire_spelling.rs` | PASS |
| V06 | required | absent / present-inner-None / present `Some("")` distinguishable | `values.rs::v06_nested_absence` | PASS |
| V07 | required | `maybe_x(None)` = absent (not "set if some"); `maybe_x` takes exactly `Option<T>`; `set_if_some` unsupported (use `if let`) | `values.rs::v07_maybe_is_exact`, `ui/conversions_are_closed.rs` | PASS |
| V08 | unsupported | reset of an optional initialized value — no `reset_`; use the original handle | — | N/A |
| V09 | required | cloning a scope keeps set values and does not re-run initializers | `values.rs::v03_initializer_evaluation_points`, `values.rs::v03_v09_initializer_order_and_clone_retention` | PASS |
| V10 | required | request built before a parent change keeps its values | `values.rs::v10_snapshots_are_not_changed_by_later_parent_changes` | PASS |
| V11 | required | generated-name collisions are compile errors naming both origins; reserved names rejected | `ui/method_collisions.rs`, `ui/builtin_collisions.rs`, `ui/reserved_value_names.rs`, `ui/duplicate_names.rs`, `ui/duplicate_value.rs` | PASS |
| V12 | required | newtype not converted from a primitive; no numeric widening; literals infer; `&str`/`&String` → `String` | `ui/newtype_is_not_converted.rs`, `ui/conversions_are_closed.rs`, `values.rs::v12_newtype_value`, `values.rs::v12_borrowed_string_and_literals` | PASS |
| — | required *(refined)* | every value (and body) has `set_x`, required ones included; only optional values and an optional body have `maybe_x` | `values.rs::required_values_have_setters`, `ui/only_optional_values_have_maybe.rs` | PASS |
| — | required | initializers cannot read other values or arguments | `ui/default_cannot_capture_arguments.rs` | PASS |

## Hierarchy and composition

| ID | Class | Expected | Test | Status |
|---|---|---|---|---|
| C01 | required | base path + root + 2 scopes + endpoint segments in order | `composition.rs::c01_segments_compose_in_order` | PASS |
| C02 | required | `a/b`→`a%2Fb`, `%2F`→`%252F`, space, Unicode, `;`, `?#`, `\` encoded once; literals verbatim | `composition.rs::c02_dynamic_segments_are_encoded_once`, `c02_mixed_segments_and_literal_pchar` | PASS |
| C03 | required | literal `//`, `.`, `..`, leading `/`, `%`, space: compile errors; value `""`/`.`/`..`: build error | `ui/path_errors.rs`, `composition.rs::c03_dot_and_empty_segments_are_rejected` | PASS |
| C04 | required | labels lowercased, prepended deepest-first; dotted/Unicode/full-width/`_`/port injection/64-byte rejected, 63-byte accepted; host > 253 bytes rejected; IP base rejected | `composition.rs::c04_*`, `composition.rs::dns_length_limits`, `ui/subdomain_errors.rs` | PASS |
| C05 | required | bare name = current owner; `client.x`/`scope.x` explicit; suggestion when only an ancestor has it | `composition.rs::c05_bare_names_bind_the_current_owner`, `ui/ancestor_needs_qualification.rs`, `ui/qualified_non_ancestor.rs`, `ui/self_qualified.rs`, `composition.rs::c4_r_003_endpoint_named_like_scope` | PASS |
| C06 | required | `each` repeats keys in order; `append` joins the key's slot | `composition.rs::c06_repeated_keys` | PASS |
| C07 | required | inherited key introduced again without an operation: compile error (query and case-folded header) | `ui/duplicate_inherited_key.rs`, `ui/duplicate_header_case.rs` | PASS |
| C08 | required | override present/absent/nested, remove, reintroduce, present-empty `each`; suppressed values never formatted | `composition.rs::q2_*`, `q4_*`, `q5_*`, `q6_*`, `q10_*`, `remove_then_introduce_absent_removes`, `c08_suppressed_contributions_are_not_formatted`, `ui/override_without_target.rs`, `ui/remove_without_target.rs`, `ui/override_after_remove.rs`, `composition.rs::each_over_option_under_override` | PASS |
| C09 | required | header names case-insensitive; values of one name keep contribution order (`append` adds after, `override` replaces); no order between different names is asserted (L2) | `composition.rs::c09_header_slots_on_the_wire` | PASS |
| C10 | required | `Authorization` ordinary; absent optional template omits the header | `composition.rs::c10_authorization_is_an_ordinary_header` | PASS |
| C11 | required | omitted optional vs required optional payload differ on the wire | `values.rs::v05_*`, `values.rs::v04_*` | PASS |
| C12 | required | no shared state between clients built on one reqwest client | `values.rs::c12_no_shared_state_between_clients` | PASS |
| C13 | required | base with port/path/trailing slash (unchanged without generated segments); invalid runtime bases (incl. port 0, bad punycode) rejected without echo; invalid static bases are compile errors | `composition.rs::c13_*`, `c4_r_004_strict_base_subset_constructs`, `base_path_without_generated_segments_is_unchanged`, `runtime_base_with_invalid_punycode_is_an_error`, `ui/base_errors.rs`, `ui/base_invalid_port.rs` | PASS |
| — | required | build checks in documented order (body, host, path, query, headers); a format's media type is checked at compile time since formats (F8g) | `composition.rs::c4_r_013_build_error_precedence`, `ui/format_invalid_content_type.rs` | PASS |
| — | required | query keys case-sensitive; form encoding; no `?` when empty | `composition.rs::query_keys_are_case_sensitive`, `query_values_are_form_encoded`, `no_query_string_when_nothing_survives` | PASS |
| — | required | escaped braces identical with and without holes | `composition.rs::c4_r_002_escaped_braces` | PASS |
| — | required | declaration order is wire order | `values.rs::query_order_follows_declaration_order` | PASS |
| — | deferred | `path replace/off`, `subdomain replace/off`, `query off` | — | N/A |

## Sensitivity

| ID | Class | Expected | Test | Status |
|---|---|---|---|---|
| S01 | required | sensitive header sent verbatim; withheld from Concord output | `sensitivity.rs::s01_s02_sensitive_values_are_sent`, `s02_query_on_wire_redacted_in_debug` | PASS |
| S02 | required | sensitive query sent; raw and encoded forms absent from `Prepared` Debug | `sensitivity.rs::s02_query_on_wire_redacted_in_debug` | PASS |
| S03 | required | sensitive path segment and host label redacted | `sensitivity.rs::s03_path_and_host_redacted` | PASS |
| S04 | required | sticky key sensitivity across a public override; safety names without marking, normalized (`Client_Secret`), exact (`page_token`, `secret_id` stay visible); effective sensitivity reaches every placement | `sensitivity.rs::s04_sensitivity_is_sticky`, `s02_*`, `s03_*`, `c4_r_001_debug_uses_effective_sensitivity`, `adv_2_effective_sensitivity_reaches_every_placement`, `safety_list_normalization_is_exact` | PASS |
| S05 | required | build and transport errors (Display, Debug, full source chain) never show the secret — including nested reqwest errors carrying URLs (refused HTTPS→HTTP redirect) and host names; native baselines do show them | `sensitivity.rs::s05_build_error_does_not_leak`, `s05_transport_error_does_not_leak`, `adv_1_sensitive_host_hides_transport_source`, `build_errors_have_no_source`, `tls.rs::c4_r_008_redirect_error_chain_does_not_leak`, `tls.rs::c4_r_008_header_reflected_into_redirect_does_not_leak`, `bodies.rs::r03_malformed_json_is_a_decode_error`, `r05_status_error_keeps_the_response` (`Error` has no `source()` chain) | PASS |
| S06 | required (documented limit) | native request Debug prints the URL (outside the guarantee); sensitive headers flagged | `sensitivity.rs::s06_native_request_debug_is_outside_the_guarantee`; `SEMANTICS.md` §9; crate docs | PASS |
| — | required | generated client/scope/endpoint Debug withholds effective-sensitive values | `sensitivity.rs::generated_debug_redacts`, `c4_r_001_debug_uses_effective_sensitivity` | PASS |

## Bodies and responses

| ID | Class | Expected | Test | Status |
|---|---|---|---|---|
| R01 | required | POST JSON with `application/json`; typed output | `bodies.rs::r01_post_json` | PASS |
| R02 | required | no body / zero-length / JSON `null` distinct (hyper omits `Content-Length: 0`) | `bodies.rs::r02_no_body_vs_empty_vs_null`, `optional_body_controls` | PASS |
| R03 | required | malformed JSON → `Decode` without body echo; `Content-Type` not consulted | `bodies.rs::r03_*` | PASS |
| R04 | required *(refined)* | an endpoint without `->` accepts 204 and any 2xx body, drained to the end without buffering (truncated body → transport error); JSON on 204 → decode error | `bodies.rs::r04_empty_responses`, `representations.rs::r04_unit_accepts_any_2xx_body`, `custom_decoder_gets_the_body_and_no_output_drains`, `unit_drain_detects_truncated_body`, `checks/heap/tests/drain.rs` (peak heap: `()` 0.75 MB vs `Binary` 137 MB for a 64 MiB body) | PASS |
| R05 | required | non-2xx → `Status`; no body in Display/Debug; unread response available; same through `Decoder::decode` | `bodies.rs::r05_status_error_keeps_the_response`, `representations.rs::decoder_applies_status_handling` | PASS |
| R06 | required | `Option<T>` output: 404 is an error; `null` is `None` | `bodies.rs::r06_option_output_has_no_status_magic` | PASS |
| R07 | required | text/binary bodies and responses preserved; invalid UTF-8 → decode error (runs without the `json` feature) | `representations.rs::r07_*` | PASS |
| R08 | required | raw response streamable natively | `bodies.rs::r08_raw_response_streaming` | PASS |
| — | required | declared `Content-Type` wins over the representation's | `composition.rs::declared_content_type_wins` | PASS |
| — | not covered | streaming *request* bodies: the loopback fixture does not decode chunked requests; use `into_parts` + `body_mut` natively | — | NOT_RUN |

## Language and tooling

| ID | Class | Expected | Evidence | Status |
|---|---|---|---|---|
| L01 | required | actionable diagnostics: unknown method/value/owner, duplicates, malformed templates, missing base, representations, `else inherit`, attributes | `tests/ui/*.rs` (53 cases) and `tests/oracle_ui/*.rs` (6 generated rejections), `.stderr` reviewed; gallery `docs/DIAGNOSTICS.md`. Cases asserting rustc's own trait-error wording are pinned to 1.97.1 | PASS on 1.97.1 |
| L02 | required | header names validated at compile time (all 8 transport-owned names rejected); literal values at compile time; formatted values at build time | `ui/invalid_header_name.rs`, `ui/reserved_header.rs`, `ui/reserved_headers_all.rs`, `ui/header_literal_value.rs`, `composition.rs::l02_invalid_header_value_is_a_build_error`, `header_value_policy` | PASS |
| L03 | required | minimal and nested examples compile and run as downstream crates (incl. a renamed dependency) | `examples/shapes/tests/wire.rs` (S1–S3 on the wire), `examples/renamed` | PASS |
| L04 | required | fmt, clippy (`-D warnings`, all targets), tests, strict rustdoc (default, no-default, json-only) on the recorded SHA | `scripts/verify.sh`; `refinement/evidence/verify_b0fe26e.txt` | PASS |
| L05 | required | feature combinations, package-isolated; JSON and TLS suites declare `required-features` and are reported as skipped, not passed | `scripts/verify.sh` feature steps; independent separate-manifest consumers in the Codex reviews (`refinement/reviews/codex/`) | PASS |
| L06 | required | no auth/rate/retry/pagination concepts in source | source scan; independent confirmation in the refinement's Codex milestone and final reviews; application-owned policies only in `examples/native` | PASS |
| L07 | required | declared MSRV 1.85 builds and passes all runtime suites and every example crate | `scripts/verify.sh --msrv` steps `msrv-concord`, `msrv-examples` (UI suites pinned to 1.97.1 are not run on 1.85) | PASS on 1.85.1 |
| L08 | required | verification is reproducible: `--locked`, tracked files unchanged | `scripts/verify.sh` | PASS |

## Refinement additions

| ID | Class | Expected | Test | Status |
|---|---|---|---|---|
| N1 | required | a literal header name is validated by the native `HeaderName` constructor at compile time (65,536 bytes: compile error; 65,535: works) | `native_limits.rs::longest_native_header_name_is_accepted`, `ui/header_name_exceeds_native_limit.rs` | PASS |
| N2 | required | a full `HeaderMap` (header lines or the body's content type) is a build error, not a panic | `native_limits.rs::a_full_header_map_is_a_build_error_not_a_panic`, `a_body_content_type_that_does_not_fit_is_a_build_error` | PASS |
| N3 | required | a value whose `Display` fails is a value-free build error in every placement; generated `Debug` shows `<Display failed>` | `display_errors.rs::every_placement_reports_a_build_error`, `generated_debug_shows_a_placeholder` | PASS |
| D04 | required | `override ... else inherit` required for an optional source and rejected for a total one; `remove` acts first in an owner; inherit/omit/replace per call; reintroduced key keeps its slot | `composition.rs::d04_override_truth_table`, `ui/override_needs_else_inherit.rs`, `ui/else_inherit_needs_optional_source.rs`, `ui/remove_acts_on_inherited.rs` | PASS |
| D06 | superseded | `-> T via R` / `body b: T via R` was replaced by formats (N06): rows F1–F12e below | — | — |
| D08 | required | an endpoint request is `Clone` exactly when its data is | `values.rs::v10_request_clone_follows_the_data`, `ui/endpoint_clone_needs_clone_values.rs` | PASS |
| D13 | required | `into_response()` returns the unread response or the error unchanged; `Decoder::finish` converts native transport errors | `representations.rs::into_response_keeps_other_errors`, `examples/native/tests/native_edit.rs`, `examples/native/tests/errors.rs` | PASS |
| D14 | required | `#[cfg]` removes a whole subtree; `#[deprecated]` warns at the caller; other attributes rejected | `attributes.rs::cfg_and_deprecated`, `ui/deprecated_endpoint.rs`, `ui/endpoint_attribute_unsupported.rs` | PASS |
| D11 | required | every base the static checker accepts is accepted by the runtime parser | `crates/concord-macros` `model::base_consistency::{corpus, random_structured_bases}` | PASS |
| T02 | required | a raw-keyword client name works | `hygiene.rs::raw_keyword_client_name` | PASS |
| O1 | required | independent composition oracle (1,728 chains, 1,233 checks) and 6 generated rejections | `oracle_composition.rs`, `oracle_ui.rs` (generator `tests-gen/oracle_composition.py --check`) | PASS |
| O2 | required | independent sensitivity oracle (252 cases, wire-side positive controls, stickiness) | `oracle_sensitivity.rs` (generator `tests-gen/oracle_sensitivity.py --check`) | PASS |
| S07–S09 | required | runtime base visible but not echoed; custom encoder text trusted; JSON encode errors content-free; classification without printing; TLS name-mismatch leak with a positive control | `d15_surfaces.rs` (all) | PASS |
| R09/U28–U32 | required | application-owned recipes: error document, native edit + finish, streaming early drop, form/multipart, cursor loop, bounded retry, admission, key rotation, interleaved query | `examples/native/tests/*.rs` (29 tests) | PASS |
| G1 | required | generated docs name the actual default of a per-call override (final review R1) | `crates/concord-macros` `emit::tests::per_call_note_names_the_actual_default` | PASS |
| G2 | required | every current-syntax Rust block in README, `docs/*.md` and `examples/native/README.md` is compiled (doctest) or a verbatim, checked excerpt of compiled code. **Exempt:** blocks marked `rust,ignore` and labelled as old (V3/alpha) syntax that does not compile, in `docs/MIGRATION.md` — neither compiled nor checked (the final re-review showed an invalid edit to one is not detected) | `examples/quickstart/tests/doc_excerpts.rs`, quickstart doctests | PASS (with the stated exemption) |
| M1 | evidence | targeted mutations are killed by these tests | `refinement/evidence/m4_mutations.md` (9/9), Codex final review (5/5) | recorded |

Windows: **PASS** — GitHub Actions run 41379941540 (`x86_64-pc-windows-msvc`, rustc 1.97.1) on
`a40788c`, code-identical to `b0fe26e`; see `refinement/evidence/ci_41379941540.md`. macOS: **NOT_RUN**.

## Native header values (Track H prototype)

Status: **prototype** — passing in `scripts/verify.sh --msrv` on the uncommitted Track H working
tree (rustc 1.97.1 and 1.85); receipt `native-repr/evidence/track_h/verify-msrv-prototype.txt`, not bound to a commit.

| ID | Class | Expected | Test | Status |
|---|---|---|---|---|
| H1 | required | owned native value from config and setter; setters make a new handle, earlier scopes/requests keep theirs | `native_headers.rs::h1_h4_client_values_on_the_wire`, `h1_setter_and_snapshots` | prototype |
| H2 | required | a `static` `from_static` value as initializer (`.clone()` or `&STATIC`), per-call and setter value; bytes reused, not copied (pointer equality) | `native_headers.rs::h2_static_reused`, `h1_h4_client_values_on_the_wire`, `initializers_run_per_client` | prototype |
| H3 | required | optional and initialized native values; absent sends nothing; empty is not absent | `native_headers.rs::h3_optional_and_initialized`, `empty_versus_absent` | prototype |
| H4 | required | `each` over native values keeps order and each item's own flag | `native_headers.rs::h4_each_order_and_flags` | prototype |
| H5 | required | bytes valid for `HeaderValue` but not text reach the wire exactly (raw-byte server); Concord output escapes them | `native_headers.rs::h5_non_text_bytes` | prototype |
| H6 | required | native sensitive flag kept without a marker on an unlisted name; OR with the declaration; withheld from every Concord output; secret reaches the server | `native_headers.rs::h6_native_sensitivity_without_marker`, `h6_native_and_declared_sensitivity_combine`, `declaration_classification_is_added` | prototype |
| H7 | required | flagged inherited value through `append`/`override`/`remove`/`else inherit`/exact replacement; flags per value | `native_headers.rs::h7_inherited_composition` | prototype |
| H8 | required | aliases, qualified paths, renamed import, renamed `concord` and `reqwest` dependencies | `native_headers.rs` (fixture), `examples/renamed/src/lib.rs::native_headers_through_renamed_dependencies`, `native-repr/evidence/track_h/coherence/out/summary.txt` | prototype |
| H9 | required | integers/`bool`/text; domain types through a template; *(final)* text follows `HeaderValue::from_str` (UTF-8 accepted, control bytes rejected); non-header types are compile errors | `native_headers.rs::h9_scalars`, `forwarded_forms`, `ui/header_value_type.rs`, `display_errors.rs` | final |
| H10 | required | a native value in a query, path or template is a compile error at its type naming the explicit conversion | `ui/native_header_cross_placement.rs` | prototype |
| H10b | required | *(final)* `ToHeaderValue` is sealed, so no flag-carrying value can be text; a marked value is withheld in the header, the template header, the path and the query; a `String` may be both and stays visible | `ui/to_header_value_sealed.rs`, `native_headers.rs::dual_use_values_are_withheld_everywhere_or_are_text` | final |
| H11 | required | generated `Debug` of config/client/scope/request with native values; `Prepared` `Debug` for sensitive / text / non-text values | `native_headers.rs::generated_debug_of_every_owner`, `h5_non_text_bytes`, `h6_*`, `h7_*` | prototype |
| H12 | required | native `Content-Type` wins over a representation's; an optional native header does not affect body presence; full header map is a build error | `native_headers.rs::native_headers_and_bodies`, `full_header_map_with_native_values` | prototype |
| H13 | evidence | payload allocations per route (static/shared native 0, text 1, integer 2, formatted 2 per value) | `checks/heap/tests/headers.rs`, `native-repr/evidence/track_h/alloc/prototype.txt` | recorded |
| H14 | evidence | mutants (flag cleared, Debug ignores flag, obs-text text, literal not lowered, `Prepared` prints secrets) killed | `native-repr/evidence/track_h/mutants/run.py`, `native-repr/evidence/track_h/mutants/result.txt` (5/5) | recorded |

## Body and response formats (track I)

Status: **track** — the implementation of `native-repr/CONTRACT.md` on branch
`native-repr-formats`, passing `scripts/verify.sh --msrv` there (rustc 1.97.1 and 1.85; report
`native-repr/evidence/tracks/track_i_formats.md`), not yet bound to a reviewed receipt. Runtime tests are
in `formats.rs` unless stated; `ui/`, `ui_json/`, `ui_features/` are trybuild cases.

| ID | Class | Expected | Test | Status |
|---|---|---|---|---|
| F1 | required | no body under a `request` default: no body, no `Content-Type` from the format | `row1_no_body_under_a_request_default` | track |
| F2a–c | required | required `Option` body = JSON `null`; absent optional body = no body, no `Content-Type`; empty `text`/`bytes` bodies are present with their media type | `row2a_*`, `row2b_*`, `row2c_*`, `bodies.rs::r02_no_body_vs_empty_vs_null` | track |
| F2d | required | a body cannot have an initializer; one body per endpoint; endpoints only | `ui/body_errors.rs` | track |
| F3a–c | required | no `->` drains any 2xx under a `response` default; `-> ()` / `-> json ()` decode a typed unit (empty 204 → decode error) | `row3a_*`, `row3b_3c_typed_unit_is_decoded` | track |
| F4 | required | `-> String` under `response json;` is a JSON string; `-> text String` raw text | `row4_json_string_versus_raw_text` | track |
| F5 | required | aliases of payloads and formats follow the written format and the defaults only | `row5_aliases_follow_the_written_format` | track |
| F6a/6d | required | request and response defaults independent, nearest owner wins, no clearing | `row6a_nearest_owner_per_direction` | track |
| F6b | required | *(final)* an endpoint statement (format and media type together) replaces the inherited one for that direction | `row6b_in_place_exception_with_media_type`, `row8a_body_media_types` | final |
| F6c | required | *(final)* statements on an endpoint need something to apply to; endpoint `format` with one direction is accepted | `ui/format_endpoint_statement.rs`, `row6c_endpoint_format_sets_both_directions`, `row6c_endpoint_statements_override_per_direction` | final |
| F7 | required | no statement reaches a body/output: compile error at the payload naming the statements | `ui/representation_missing_via.rs`, `ui/representation_alpha_spelling.rs` | final |
| F8a | required | body media type: declared by the `request` statement (`request F "m";`) or the format's; sent exactly with the body | `row8a_body_media_types`, `forms.rs::pairs_and_form_default` | final |
| F8b | required | four runtime cases with a `content-type` contribution; empty native value present; empty `each` absent; conversion failure is a build error | `row8b_four_runtime_cases`, `row8b_empty_value_empty_each_and_conversion_failure`, `native_headers.rs::native_headers_and_bodies` | track |
| F8c | required | declared body media type + surviving `content-type` contribution (conditional included) rejected; `remove` resolves it | `ui/format_header_media_conflict.rs`, `row8c_remove_resolves_the_conflict` | track |
| F8d/8e | required | no `Accept`, response `Content-Type` unchecked; an output takes no media type | `row8d_no_accept_and_no_response_type_check`, `ui/format_output_media_type.rs` | track |
| F8f | required | inserted `Content-Type` carries the key's static sensitivity (flag, `Prepared` Debug, still sent) | `row8f_inserted_content_type_is_classified` | track |
| F8g | required | declared media types checked by the macro; a format's `CONTENT_TYPE` checked at compile time where it may be sent, not read where replaced | `ui/format_media_type_invalid.rs`, `ui/format_invalid_content_type.rs`, `representations.rs::declared_media_type_replaces_an_invalid_constant`, `row8a_body_media_types` (`CONTENT_TYPE = None`) | track |
| F9 | required | custom codec by path in place and as a default; unsupported payload reported at the word or the payload | `row9_custom_codec_in_place_and_as_default`, `ui/representation_unsupported_payload.rs`, `ui/format_inherited_unsupported.rs`, `ui_json/format_serde_bound.rs` | track |
| F10/F11 | required | native exits and status handling unchanged | `row11_status_and_native_decoder_unchanged`, `bodies.rs::r05_*`, `r08_*`, `examples/multipart`, `examples/native` | track |
| F12a | required | `json`/`form` off: one error per in-place word, one per used default | `ui_features/feature_disabled_formats.rs` (no-default lanes) | track |
| F12b | required | defaults reached only through aliases gated by consumers' `cfg`s; unused defaults never referenced | `row12b_unused_and_cfg_disabled_defaults_are_never_referenced`, `attributes.rs::cfg_and_deprecated`, `ui_features/feature_disabled_formats.rs` | track |
| F12c | required | words need no import under a renamed dependency; paths resolve normally | `examples/renamed/src/lib.rs::formats_through_the_renamed_crate` | track |
| F12d | required | leftover `via` → exact replacement statement *(final: the migration script is withdrawn)* | `ui/format_via_removed.rs` | final |
| F12e | required | rustdoc states format, media type and origin per endpoint, defaults per owner | `concord-macros` `emit::tests::format_docs_*` | track |
| FL | required | lexical precedence table (values named like words, word-prefixed paths, `-> r#json`, reserved words, swapped order) | `lexical_precedence`, `lexical_raw_identifier_is_a_payload`, `ui/format_reserved_words.rs`, `ui/format_swapped_order.rs` | track |
| FX | required | CHARTER §7 cross-feature: optional native headers vs body presence (H11), defaults vs header presence/inheritance (H12), natively sensitive header under defaults | `h11_*`, `h12_*`, `native_sensitive_header_stays_protected_under_defaults` | track |
| FF | required | built-in `form` (feature `form`): encoding, pairs, defaults, responses, encode errors, form in / JSON out | `forms.rs` | track |

## Final: native header names (F06–F09)

| ID | Class | Expected | Test | Status |
|---|---|---|---|---|
| K1 | required | `header(AUTHORIZATION)`, a custom constant, an alias, a re-export, qualified/absolute paths and a `static` send their names | `native_keys.rs::h01_h03_native_keys_send_their_names` | final |
| K2 | required | composition by resolved identity across spellings: `override "Authorization"`, `remove AUTH`, absolute `append`, `static` override, `else inherit` | `native_keys.rs::h05_composition_follows_identity_not_spelling` | final |
| K3 | required | a local constant named `AUTHORIZATION` holding `x-other` is `x-other`; identity errors that depend on a constant are `Build` errors at `prepare()` naming the declaration | `native_keys.rs::identity_errors_are_deferred_build_errors` | final |
| K4 | required | errors the macro proves stay compile errors in a chain with native keys (a literal set twice, an optional override without `else inherit`); the same constant set twice is a `Build` error from `prepare()` naming both declarations (equal tokens are not proof of one header) | `ui/native_key_static_errors.rs`, `native_keys.rs::the_same_constant_twice_is_decided_when_the_request_is_built` | final |
| K10 | required | `$crate::KEY` from two crates' macros (`x-public`, `authorization`) stays two headers: names, values, sensitivity flag and `Prepared` redaction follow the resolved names (final review X-R1) | `checks/hygiene/tests/cross_crate.rs` | final |
| K11 | required | native spelling never redacts less than literal spelling: a value classified at run time on one chain is withheld on chains with literal keys only; stickiness covers overridden values and literals (final review O-R1, O-P2) | `spelling_equivalence.rs` | final |
| K5 | required | removed or replaced native contributions are never formatted | `native_keys.rs::h15_suppressed_values_are_never_formatted` | final |
| K6 | required | classification by resolved name: `header(AUTHORIZATION)` unmarked is withheld and flagged; a shadowing `x-other` is not; a marked native contribution makes a later literal spelling sensitive; a conditional value that is also text is withheld from the URL | `native_keys.rs::h14_classification_by_resolved_name`, `marked_native_contribution_is_sticky_across_spellings` | final |
| K7 | required | reserved names through native constants are `Build` errors; a native `CONTENT_TYPE` wins over the format's media type and conflicts with a declared one | `native_keys.rs::reserved_and_content_type_through_native_constants` | final |
| K8 | required | a key path that is not a `HeaderName` is one compile error at the path; a query key cannot be a path | `ui/native_key_type.rs` | final |
| K9 | required | the run-time header plan equals the sequential composition rule on every chain of up to four contributions; sensitivity sticky per header and by name | `header_plan.rs::plan_equals_the_sequential_rule`, `sensitivity_is_sticky_per_header_and_by_name` | final |
| K10 | required | literal names, literal values and template fragments checked by `http` at expansion (control bytes, NUL, DEL, a fragment no hole can repair, over-long names); UTF-8 literals accepted | `ui/invalid_header_name.rs`, `ui/header_literal_value.rs`, `ui/header_name_exceeds_native_limit.rs`, `composition.rs::header_value_policy` | final |
| K11 | required | the macro's and the runtime's sensitive and reserved name lists are equal | `concord-macros` `model::tests::name_lists_match_the_runtime` | final |
| K12 | required | `ToHeaderValue` cannot be implemented downstream | `ui/to_header_value_sealed.rs` | final |

## Final: representation statements (F01–F05)

| ID | Class | Expected | Test | Status |
|---|---|---|---|---|
| S1 | required | `format`/`request`/`response` on client, scope and endpoint; nearest per direction; endpoint `format` with one direction | `formats.rs::row6a_nearest_owner_per_direction`, `row6c_endpoint_format_sets_both_directions`, `row6c_endpoint_statements_override_per_direction` | final |
| S2 | required | every removed in-place spelling is a compile error naming the replacement statement | `ui/format_removed_spellings.rs`, `ui/format_swapped_order.rs` | final |
| S3 | required | a media type without a format, on a response or on `format` is a compile error with the reason | `ui/format_media_type_needs_format.rs`, `ui/format_output_media_type.rs` | final |
| S4 | required | an unsupported payload is reported with Concord's `Encode<T>`/`Decode<T>` message (not a `BodyFormat` mismatch), on the endpoint, a scope or the client | `ui/representation_unsupported_payload.rs`, `ui/format_inherited_unsupported.rs` | final |
| S5 | required | declared media types parsed by the `mime` crate | `ui/format_media_type_invalid.rs` | final |
| S6 | required | the 38-test common corpus (A01–A20) behaves identically under the control and the selected surface | external: `corpus_master/` in the campaign work directory (`v4/finalization/MODEL_RUNS.md`) | recorded |

## Final: the Riot example (F11)

| ID | Class | Expected | Test | Status |
|---|---|---|---|---|
| R1 | required | account-v1 on the regional host; Riot ID path encoding; key in `x-riot-token`, no `Authorization`; optional account fields | `examples/riot/tests/wire.rs::account_by_riot_id_is_regional_and_path_encoded`, `optional_account_fields_may_be_omitted` | final |
| R2 | required | summoner-v4, champion-mastery-v4, champion-v3 on the platform host; omitted zero fields | `examples/riot/tests/wire.rs::platform_operations_use_the_platform_host` | final |
| R3 | required | match-v5 ids send only the filters that are set | `examples/riot/tests/wire.rs::match_ids_send_only_the_filters_that_are_set` | final |
| R4 | required | tournament-stub bodies: exact JSON bytes, query, `Content-Type`; optional fields omitted | `examples/riot/tests/wire.rs::tournament_codes_send_the_documented_body`, `optional_tournament_fields_are_omitted_not_null` | final |
| R5 | required | RSO `/accounts/me`: `AUTHORIZATION` replaces the API key (native `remove`) | `examples/riot/tests/wire.rs::rso_access_token_replaces_the_api_key` | final |
| R6 | required | Data Dragon: credential-free host, JSON and PNG bytes, never the key | `examples/riot/tests/wire.rs::data_dragon_is_credential_free_json_and_bytes` | final |
| R7 | required | bounded read workflow on routed hosts; history capped; tournament codes workflow | `examples/riot/tests/workflows.rs::player_summary_reads_a_bounded_history`, `history_is_capped_whatever_the_caller_asks`, `tournament_codes_workflow` | final |
| R8 | required | non-200 JSON/empty/HTML bodies kept unread; 429 stops with `Retry-After`, no retry; malformed 200 is a decode error; key and access token withheld, positive control on the wire | `examples/riot/tests/errors.rs::non_success_bodies_are_kept_unread`, `a_429_stops_the_workflow_with_retry_after`, `a_429_without_retry_after_still_stops`, `malformed_success_body_is_a_decode_error`, `the_key_and_access_tokens_are_withheld_from_debug` | final |
