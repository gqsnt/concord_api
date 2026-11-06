# Acceptance matrix — modular codecs increment

Rows are the handoff's `TEST_MATRIX.md` IDs. "Evidence" names the test or command that checks
the row; a test path is relative to the repository root unless it starts with `crates/concord/tests/`'s
convention (bare names are in `crates/concord/tests/`). Status: **met** (a discriminating test or
command), **partial** (what is missing is said), **NOT_RUN** (environment), **deferred**.

## Toolchain and package graph

| ID | Status | Evidence |
|---|---|---|
| T01 | met | `scripts/check-ci.py --toolchain` (minimum = pin = 1.99.1; every production crate inherits it) |
| T02 | met | state 1 (`dfdeed8`, 1.97.1) and state 2 (`686cd24`, 1.99.1) gate runs in STATUS; PERF compares state 2 with the split and the candidate on 1.99.1 |
| T03 | met | `scripts/verify.sh` is the inventory; `scripts/check-ci.py --workflow`; previously omitted suites run in the ordinary lane; `clippy-features` lints the feature subsets the workspace build unifies away |
| T04 | met | UI snapshots reviewed on 1.99.1 (`686cd24`); nightly lane `scripts/verify.sh --nightly-only` (`checks/clone-compat`) |
| A01 | met | `checks/consumers/core-only` |
| A02 | met | `checks/consumers/facade-no-extras` (graph without extras); `scripts/check-graph.py` |
| A03 | met | `checks/consumers/extras-core`; `crates/concord-codecs/tests/media.rs` |
| A04 | met | `checks/consumers/facade-full`, `facade-json-only` (`Form` absent: compile_fail); verify lanes per feature |
| A05 | met | `crates/concord-macros/src/syntax.rs` (no `Word`, no keywords); `ui/format_unknown_type.rs` |
| A06 | met | `formats.rs::lexical_precedence`, `checks/consumers/facade-no-extras` (`type json = MyJson;`), `hygiene.rs` (absolute path), `bodies.rs` (`IgnoreContentType<Json>`) |
| A07 | met | `examples/renamed`, `checks/consumers/facade-no-extras` (`sdk`), `hygiene.rs` (`macro_rules!` forwarding, no prelude) |
| A08 | met | `checks/consumers/core-only`, `facade-no-extras` (encoder-only, decoder-only) |
| A09 | met | `attributes.rs` (nonexistent format types under `cfg(any())`, unused defaults) |
| A10 | met | `scripts/package-check.sh`, `scripts/consumers.sh --packaged` (gate `--package`) |

## HTTP metadata and codecs

| ID | Status | Evidence |
|---|---|---|
| C01 | met | `metadata.rs`, `bodies.rs::r01_post_json` |
| C02 | met | `crates/concord-codecs/tests/media.rs::accepted_media_types`; `metadata.rs::a_downstream_format_uses_response_metadata` |
| C03 | met | `bodies.rs::r03_content_type_is_checked_unless_deliberately_ignored`, `metadata.rs::the_head_is_refused_before_the_body_exists` |
| C04 | met | `crates/concord-codecs/tests/media.rs::refused_media_types_with_their_labels` (missing, repeated equal/conflicting, comma-joined, malformed, obs-text); `charset::the_parameter_grammar_is_rfc_9110` (RFC 9110 forms `mime` refused alone are accepted; what the grammar excludes, an empty subtype included, stays refused) |
| C05 | met | `crates/concord-codecs/tests/media.rs` (case, parameters, `charset`: quoted before another parameter, repeated refused); `metadata.rs::charset_parameters_are_read_exactly`; no transcoding (Json, Text docs). Closes final-audit X-F1/X-F2 |
| C06 | met | `metadata.rs::metadata_errors_print_no_response_content` |
| C07 | met | `metadata.rs::status_comes_before_the_format` |
| C08 | met | `error_inspection.rs::empty_responses_are_not_null`, `crates/concord-codecs/tests/media.rs::empty_bodies_are_not_null` |
| C09 | met | `metadata.rs::no_output_checks_nothing`; `accept.rs` (no suggestion without `->`) |
| C10 | met | `examples/compat/tests/json_or_empty.rs` |
| C11 | met | `crates/concord-codecs/tests/media.rs::binary_checks_nothing`, `text_accepts_any_media_type_but_a_foreign_charset`; `representations.rs` |
| C12 | met | `forms.rs::with_json::form_in_json_out`, `crates/concord-codecs/tests/media.rs::form_response_head` |
| C13 | met | `metadata.rs::a_downstream_format_uses_response_metadata` |
| C14 | met | `metadata.rs::a_one_shot_payload_is_moved_into_the_body` |
| C15 | met | `metadata.rs::futures_stay_send`; `Decoder` is `Copy`; Json's context is `()` (no header clone) |
| C16 | met | `metadata.rs::native_exits_apply_the_same_rules` |
| C17 | met | same test (`send_raw`), `examples/riot/tests/redirects.rs` |
| C18 | met | `accept.rs::the_wire_and_the_client_defaults` |
| C19 | partial | `metadata.rs::content_encoding_is_not_a_media_type`; reqwest's decompression features are off in this build, so their transformation of metadata is source inspection only (tower-http removes `Content-Encoding`/`Content-Length`) — NOT_RUN with decompression |
| C20 | met | `metadata.rs::the_head_is_refused_before_the_body_exists`; body-first mutant times out (evidence/mutants) |

## Header defaults and representation interactions

| ID | Status | Evidence |
|---|---|---|
| M01 | met | `accept.rs::prepared_requests_follow_the_truth_table` |
| M02 | met | same (literal any case, native value) |
| M03 | met | same (absent optional; removal) |
| M04 | met | same (reintroduced present/absent; repeated) |
| M05 | met | same (empty value; q-values; `each` over an empty list places nothing → the suggestion) |
| M06 | met | `accept.rs::the_wire_and_the_client_defaults` (default `*/*` and a configured default) |
| M07 | met | `bodies.rs` (no body, no Content-Type), `formats.rs` rows 6–8 |
| M08 | met | `bodies.rs::r02_*`, `error_inspection.rs` |
| M09 | met | `formats.rs` row 8c, `ui/format_header_media_conflict.rs`, `validate_names.rs` (native `CONTENT_TYPE` with a declared media type) |
| M10 | met | `examples/compat/tests/apply.rs` |
| M11 | met | `accept.rs::a_sensitive_accept_classifies_the_suggestion`; `spelling_equivalence.rs`, `oracle_sensitivity.rs` unchanged |
| M12 | met | `ui/format_media_type_invalid.rs`, `ui/format_invalid_accept.rs` (a format's `ACCEPT` is a compile-time static), `native_headers.rs` (checked native construction) |

## Contract validation and native-key closure

| ID | Status | Evidence |
|---|---|---|
| H01 | met | `validate_names.rs::validation_runs_nothing_and_matches_prepare` (associated function; no client) |
| H02 | met | same (counters on defaults, `Display`, encoder stay 0; uninhabited body type) |
| H03 | met | same (stable order, labels); `ui/builtin_collisions.rs` (method name) |
| H04 | met | same (each problem = `prepare()`'s header-composition error); `validate_names.rs::earlier_build_steps_fail_first` (a failing encoder runs first, SEMANTICS §6); `crates/concord-core/tests/header_plan.rs` |
| H05 | met | `header_plan.rs::the_validator_and_prepare_share_one_verdict` (every presence assignment) |
| H06 | met | `validate_names.rs` (cfg-disabled endpoint with a nonexistent key); `Literal`/`Coherent` clients |
| H07 | met | `validate_names.rs` (aliases, equal names under two constants); `checks/hygiene/tests/cross_crate.rs` |
| H08 | met | `ui/native_key_sealed.rs` (E0277 on `sealed::Sealed`, through `concord::__macro::concord_core`; removing the supertrait fails it: work directory evidence/mutants/S1_unsealed.txt), `ui/native_key_deref.rs`; `native_keys.rs` (constants, statics, aliases) |
| H09 | met | `emit::tests::key_occurrence_owner_matches_a_scan_of_every_owner` |
| H10 | met | `spelling_equivalence.rs`, `oracle_sensitivity.rs`, `native_keys.rs` (unchanged, passing) |
| H11 | met | `sensitivity.rs`, `native_keys.rs` (removed/overridden sticky classification) |
| H12 | met | `examples/riot/tests/wire.rs::header_names_validate_without_credentials`; `synthetic_sdk.rs` (480 endpoints, generated) |
| H13 | met | `header_plan.rs` (sequential oracle; validator verdict) |
| H14 | met | Criterion group `plan_scaling` (PERF) |

## Codec error guardrails

| ID | Status | Evidence |
|---|---|---|
| E01 | met | `concord-core` doctest `compile_fail,E0308` on `CodecError::new(e.to_string())` |
| E02 | met | `crates/concord-codecs/tests/media.rs::decode_errors_show_positions_not_content`; `error_inspection.rs::source_of_a_codec_error` |
| E03 | met | `examples/native/tests/bodies.rs` (record/line/column) |
| E04 | met | `metadata.rs` (unread response; `Error::response` documented unredacted) |
| E05 | met | `scripts/check-graph.py` (no serializer in core/macro graphs); core diagnostics name `Encode`/`Decode` only |
| E06 | met | `Decode::decode_body` docs; `Decoder::new` docs |

## Application and adoption

| ID | Status | Evidence |
|---|---|---|
| R01 | met | `examples/riot/tests/redirects.rs` |
| R02 | met | same |
| R03 | met | `examples/riot/tests/{workflows,wire,errors}.rs` |
| R04 | met | `examples/riot/tests/errors.rs::non_json_success_responses_are_refused_or_fail_to_decode` (fixtures carry real Content-Types; no bypass) |
| R05 | met | `examples/riot/SOURCES.md` (unchanged facts and open questions) |
| R06 | met | README, QUICKSTART, CHEATSHEET, guides, `docs/CUSTOM_FORMATS.md` (rewritten by Sonnet; doctests and excerpt checks in the gate); no built-in format word remains (A05); fresh-use notes applied (REVIEW_TRIAGE) |
| R07 | met | `v4/modular-codecs/reports/fresh-terra-REPORT.md`, `v4/modular-codecs/reports/fresh-sonnet-REPORT.md`: both completed a metadata-aware vendor format and all-endpoint name validation from the docs alone, without reading the implementation first |
| R08 | met | `emit::tests::format_docs_*` (links to the format type, `Encode::CONTENT_TYPE`) |
| R09 | met | `docs/MIGRATION.md` (compiler-guided; no rewriter) |

## Optional path gate

| ID | Status | Evidence |
|---|---|---|
| P01–P07 | deferred | Terra prototype report (work directory); `examples/compat` native recipe `write_resource_segments` and its tests |
