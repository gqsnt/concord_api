# V4 acceptance matrix

Classification: **required** (executable test exists), **unsupported** (deliberately not offered;
how to do it instead), **deferred**. Test paths are relative to `crates/concord/tests/` unless they
start with another directory. **Status** is bound to the verification receipt
[`refinement/evidence/verify_b0fe26e.txt`](refinement/evidence/verify_b0fe26e.txt): `scripts/verify.sh
--msrv` on code revision `b0fe26e4ec3d0c2ac7e1ebb4216d05a38a4aff4e`, clean tree, Linux x86_64, rustc
1.97.1 and 1.85.1, all 11 steps PASS. Commits after `b0fe26e` change only records under `v4/` (this
file, `refinement/`), which no build or test reads; `refinement/FINAL.md` lists them and shows the
code is identical. Earlier alpha receipts in `evidence/GATES.md` are historical. Rows added or changed by the refinement are in the last section
or marked *(refined)*.

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
| — | required | build checks in documented order (body incl. content type, host, path, query, headers) | `composition.rs::c4_r_013_build_error_precedence`, `representations.rs::content_type_error_precedes_path_error` | PASS |
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
| D06 | required | `-> T via R` / `body b: T via R`; missing `via` and unsupported payloads diagnosed at the declaration; custom `Encode<T>`/`Decode<T>` | `ui/representation_missing_via.rs`, `ui/representation_unsupported_payload.rs`, `ui/representation_alpha_spelling.rs`, `representations.rs::custom_decoder_gets_the_body_and_no_output_drains`, `examples/native` NDJSON/form | PASS |
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
