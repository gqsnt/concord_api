# V4 acceptance matrix

Classification: **required** (executable test exists), **unsupported** (deliberately not offered;
how to do it instead), **deferred**. Status is from the runs recorded in `evidence/GATES.md`
(Linux x86_64 only). Test paths are relative to `crates/concord/tests/`.

## Minimal surface

| ID | Class | Expected | Test | Status |
|---|---|---|---|---|
| M01 | required | zero-input client; `GET /v1/health` → typed JSON over loopback | `slice.rs::m01_health_round_trip` | PASS |
| M02 | required | application `reqwest::Client` used as is (default headers, UA on the wire) | `slice.rs::m02_application_client_is_used`, `composition.rs::client_default_headers_fill_only_missing_names` | PASS |
| M03 | required | native request available before sending; server sees nothing | `slice.rs::m03_prepare_does_not_contact_the_server` | PASS |
| M04 | required | native response without status check or decoding | `slice.rs::m04_send_raw_returns_the_native_response`, `bodies.rs::r08_raw_response_streaming` | PASS |
| M05 | required | one `send` = one `execute`; failures are not retried | `slice.rs::m05_failure_is_not_retried`, `bodies.rs::transport_error` | PASS |
| — | required | native parts modified, executed by the app, decoded as declared | `slice.rs::into_parts_then_decode` | PASS |

## Values and builders

| ID | Class | Expected | Test | Status |
|---|---|---|---|---|
| V01 | required | required root value is positional (compile error if missing); `""` valid | `values.rs::v01_required_root_value_empty_string_is_valid`, `ui/missing_required_argument.rs` | PASS |
| V02 | required (reset: unsupported) | initializer used when unset; `set_` replaces it. No `reset_` (D3) — keep the original handle | `values.rs::v02_initialized_value_and_override` | PASS |
| V03 | required | initializer runs once per `new`/scope entry/endpoint call; never on clone, setter, prepare | `values.rs::v03_initializer_evaluation_points` | PASS |
| V04 | required | absent optional: no occurrence; present `""`: `key=` | `values.rs::v04_optional_absent_vs_present_empty` | PASS |
| V05 | required | required value with optional payload stays required; spelling is the type's `Display`; bare `Option<T>` has no wire spelling (compile error) | `values.rs::v05_required_optional_payload_is_not_omission`, `ui/option_has_no_wire_spelling.rs` | PASS |
| V06 | required | absent / present-inner-None / present `Some("")` distinguishable | `values.rs::v06_nested_absence` | PASS |
| V07 | required | `maybe_x(None)` = absent (not "set if some"); `set_if_some` unsupported (use `if let`) | `values.rs::v07_maybe_is_exact` | PASS |
| V08 | unsupported | reset of an optional initialized value — no `reset_`; use the original handle | — | N/A |
| V09 | required | cloning a scope keeps values and does not re-run initializers | `values.rs::v03_initializer_evaluation_points` | PASS |
| V10 | required | request built before a parent change keeps its values | `values.rs::v10_snapshots_are_not_changed_by_later_parent_changes` | PASS |
| V11 | required | generated-name collisions are compile errors naming both origins | `ui/method_collisions.rs`, `ui/duplicate_names.rs` | PASS |
| V12 | required | newtype not converted from a primitive; literals infer; `&str` → `String` | `ui/newtype_is_not_converted.rs`, `values.rs::v12_newtype_value`, `values.rs` (literal `7`, `"acme"` throughout) | PASS |
| — | required | required values have no setters; defaulted non-optional have no `maybe_` | `ui/required_has_no_setter.rs` | PASS |
| — | required | initializers cannot read other values or arguments | `ui/default_cannot_capture_arguments.rs` | PASS |

## Hierarchy and composition

| ID | Class | Expected | Test | Status |
|---|---|---|---|---|
| C01 | required | base path + root + 2 scopes + endpoint segments in order | `composition.rs::c01_segments_compose_in_order` | PASS |
| C02 | required | `a/b`→`a%2Fb`, `%2F`→`%252F`, space, Unicode, `;`, `?#`, `\` encoded once; literals verbatim | `composition.rs::c02_dynamic_segments_are_encoded_once`, `c02_mixed_segments_and_literal_pchar` | PASS |
| C03 | required | literal `//`, `.`, `..`, leading `/`, `%`, space: compile errors; value `""`/`.`/`..`: build error | `ui/path_errors.rs`, `composition.rs::c03_dot_and_empty_segments_are_rejected` | PASS |
| C04 | required | labels lowercased, prepended deepest-first; dotted/Unicode/full-width/`_`/port injection/64-byte rejected; IP base rejected | `composition.rs::c04_*`, `ui/subdomain_errors.rs` | PASS |
| C05 | required | bare name = current owner; `client.x`/`scope.x` explicit; suggestion when only an ancestor has it | `composition.rs::c05_bare_names_bind_the_current_owner`, `ui/ancestor_needs_qualification.rs`, `ui/qualified_non_ancestor.rs`, `ui/self_qualified.rs`, `composition.rs::c4_r_003_endpoint_named_like_scope` | PASS |
| C06 | required | `each` repeats keys in order; `append` joins the key's slot | `composition.rs::c06_repeated_keys` | PASS |
| C07 | required | inherited key introduced again without an operation: compile error (query and case-folded header) | `ui/duplicate_inherited_key.rs`, `ui/duplicate_header_case.rs` | PASS |
| C08 | required | override present/absent/nested, remove, reintroduce, present-empty `each`; suppressed values never formatted | `composition.rs::q2_*`, `q4_*`, `q5_*`, `q6_*`, `q10_*`, `remove_then_introduce_absent_removes`, `c08_suppressed_contributions_are_not_formatted`, `ui/override_without_target.rs`, `ui/remove_without_target.rs`, `ui/override_after_remove.rs` | PASS |
| C09 | required | header names case-insensitive; `append` groups by name; `override` replaces | `composition.rs::c09_header_slots_on_the_wire` | PASS |
| C10 | required | `Authorization` ordinary; absent optional template omits the header | `composition.rs::c10_authorization_is_an_ordinary_header` | PASS |
| C11 | required | omitted optional vs required optional payload differ on the wire | `values.rs::v05_*`, `values.rs::v04_*` | PASS |
| C12 | required | no shared state between clients built on one reqwest client | `values.rs::c12_no_shared_state_between_clients` | PASS |
| C13 | required | base with port/path/trailing slash; invalid runtime bases rejected without echo; invalid static bases are compile errors | `composition.rs::c13_*`, `c4_r_004_strict_base_subset_constructs`, `ui/base_errors.rs`, `ui/base_invalid_port.rs` | PASS |
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
| S04 | required | sticky key sensitivity across a public override; safety names without marking; exact-name list (`page_token` stays visible) | `sensitivity.rs::s04_sensitivity_is_sticky`, `s02_*`, `s03_*`, `c4_r_001_debug_uses_effective_sensitivity` | PASS |
| S05 | required | build and transport errors (Display, Debug, full source chain) never show the secret; raw reqwest error baseline does | `sensitivity.rs::s05_build_error_does_not_leak`, `s05_transport_error_does_not_leak`, `bodies.rs::r03_malformed_json_is_a_decode_error`, `r05_status_error_keeps_the_response` | PASS |
| S06 | required (documented limit) | native request Debug prints the URL (outside the guarantee); sensitive headers flagged | `sensitivity.rs::s06_native_request_debug_is_outside_the_guarantee`; `SEMANTICS.md` §9; crate docs | PASS |
| — | required | generated client/scope/endpoint Debug withholds effective-sensitive values | `sensitivity.rs::generated_debug_redacts`, `c4_r_001_debug_uses_effective_sensitivity` | PASS |

## Bodies and responses

| ID | Class | Expected | Test | Status |
|---|---|---|---|---|
| R01 | required | POST JSON with `application/json`; typed output | `bodies.rs::r01_post_json` | PASS |
| R02 | required | no body / zero-length / JSON `null` distinct (hyper omits `Content-Length: 0`) | `bodies.rs::r02_no_body_vs_empty_vs_null`, `optional_body_controls` | PASS |
| R03 | required | malformed JSON → `Decode` without body echo; `Content-Type` not consulted | `bodies.rs::r03_*` | PASS |
| R04 | required | `()` accepts 204 and any 2xx body (drained, not buffered); JSON on 204 → decode error | `bodies.rs::r04_empty_responses`, `head_request`, `c4_r_005_unit_drains_large_body` | PASS |
| R05 | required | non-2xx → `Status`; no body in Display/Debug; unread response available | `bodies.rs::r05_status_error_keeps_the_response` | PASS |
| R06 | required | `Option<T>` output: 404 is an error; `null` is `None` | `bodies.rs::r06_option_output_has_no_status_magic` | PASS |
| R07 | required | text/binary bodies and responses preserved; invalid UTF-8 → decode error | `bodies.rs::r07_*` | PASS |
| R08 | required | raw response streamable natively | `bodies.rs::r08_raw_response_streaming` | PASS |
| — | required | declared `Content-Type` wins over the representation's | `composition.rs::declared_content_type_wins` | PASS |
| — | not covered | streaming *request* bodies: the loopback fixture does not decode chunked requests; use `into_parts` + `body_mut` natively | — | NOT_RUN |

## Language and tooling

| ID | Class | Expected | Evidence | Status |
|---|---|---|---|---|
| L01 | required | actionable diagnostics: unknown method/value/owner, duplicates, malformed templates, missing base | `tests/ui/*.rs` (34 cases, `.stderr` reviewed) | PASS on 1.97.1 |
| L02 | required | header names validated at compile time; values at build time | `ui/invalid_header_name.rs`, `ui/reserved_header.rs`, `composition.rs::l02_invalid_header_value_is_a_build_error` | PASS |
| L03 | required | minimal and nested examples compile as downstream crates | `examples/` crates (see `evidence/GATES.md`) | see GATES |
| L04 | required | fmt, clippy, tests on the recorded SHA | `evidence/GATES.md` | see GATES |
| L05 | required | feature combinations; JSON suites declare `required-features` (skipped, not passed, without `json`) | `evidence/GATES.md` feature matrix | PASS |
| L06 | required | no auth/rate/retry/pagination concepts in source | `evidence/GATES.md` scope scan | see GATES |
| L07 | required | declared MSRV 1.85 builds and passes all runtime suites | `evidence/GATES.md` | PASS (UI wording differs, see GATES) |

Windows and macOS: **NOT_RUN** (no such environment was available).
