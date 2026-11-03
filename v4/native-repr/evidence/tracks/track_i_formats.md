# Track I — body and response formats: implementation report

Implementation track for Concord V4's format DSL (`v4/native-repr/CONTRACT.md`, decisions N06–N08,
Codex design pass 2). Worktree `wt/formats`, branch `native-repr-formats`, from `c112845`. Nothing
pushed; no other worktree or branch touched. Evidence files are in
`reports/track_i_formats_evidence/` (verify logs, mutation harness and logs, scale measurements).

## 1. Model, effort, toolchains

- Model: Claude Opus 5.5 (`claude-opus-5-5`), as a sub-agent. The reasoning-effort setting is
  **not observable** from inside the session, so it is not reported.
- Toolchains: rustc/cargo 1.97.1 (pinned), 1.85.1 (MSRV), nightly 1.101 (expansion only),
  hyperfine 1.19. Every build `--offline`; every gate `--locked`. No new crates.io package:
  `serde_urlencoded 0.7.1` was in `Cargo.lock`; the lockfile gains one dependency edge
  (`concord -> serde_urlencoded`).

## 2. Commits (on `native-repr-formats`)

| Commit | Content |
|---|---|
| `71d872d` feat(formats) | parser, resolution, codegen, runtime (`BodyFormat` statics, 8b/8f in `Parts::finish`, built-in `Form`), migration of every `api!` in crates/, examples/, checks/, benches/criterion; UI re-blesses and new UI cases |
| `ef0de89` test(formats) | `crates/concord/tests/formats.rs` (26 tests: rows 1–12e, lexical table, CHARTER §7 cross-feature cases), `tests/forms.rs` (5 tests) |
| `5f7a4c3` ci | `form` lanes (test + rustdoc) in `scripts/verify.sh` and CI, MSRV lists (+`formats`, `forms`; CI also gains the missing `native_headers`); `benches/scale/gen.py` `--syntax formats|via|alpha`, `typed` shape, `scale_typed` member + `ab.sh` rows |
| `6b7fca0` docs | README, crate docs (in `71d872d`), QUICKSTART, AUTHOR_GUIDE §5 rewritten, CALLER_GUIDE, RECIPES, DIAGNOSTICS, MIGRATION "From `via` to formats", MAINTAINERS, `v4/SEMANTICS.md` §7 + grammar, `v4/TEST_MATRIX.md` F rows, `scripts/migrate_via.py` |

Diff `c112845..6b7fca0`: 88 files, +4380 −506; macro + runtime sources (`crates/*/src`) +1280 −191,
a large share of it diagnostic and generated-rustdoc text and the macro's rustdoc unit tests.

Before `6b7fca0` the guides still quoted `via`, so `doc_excerpts` and the guide doctests fail on the
earlier commits (observed at `71d872d`; the `concord` lanes passed there). The full gate is
verified at `6b7fca0` only.

## 3. What was built

**Parser (`syntax.rs`).** `request FORMAT[("media")];` / `response FORMAT;` as owner statements
(client/scope; once each; before children; on an endpoint → error 6c). Bodies: `body name: T`,
`body("media") name: T`, `FORMAT name: T`, `FORMAT("media") name: T` (`?` optional); outputs
`-> T`, `-> FORMAT T`. `FORMAT` = `json|text|bytes|form` (→ `$crate::{Json, Text, Binary, Form}`,
located at the word) or a type path. Lexical rules exactly as the contract's table (`name:`/`name?:`
is always a value; a word followed by `::` is a path; after `->` a path followed by more type
tokens is a format, a path ending the output is the payload; a parenthesized string after a
format is a media type, any other group a tuple payload; bare lowercase identifier in format
position must be a word; `r#json` never a word). Media types are validated by an RFC 9110
`media-type` checker (tokens, `;` parameters with token or quoted-string values, visible ASCII, no
surrounding whitespace). Diagnostics for leftover `via` (output, `body`, and plain `x: T via R`),
alpha spellings, endpoint statements, output media types, swapped order, reserved/lowercase words
(with a placement-typo variant in parameter lists).

**Model (`model.rs`).** `OwnerDefault { format, media, alias, consumers }` per declared default.
During `flatten`, each `body`/bare `->` resolves to the nearest owner on its chain per direction
(`nearest`), with `Origin::{InPlace, Default(owner)}` kept for docs and spans, and the endpoint is
pushed into that default's `consumers`. A body's media type is `(lit, origin)`: in place,
`body("…")`, or the default's (only through `body`; an in-place format drops it). 8c:
`check_media_conflict` rejects a declared media type when the endpoint's composed `content-type`
slot has any surviving item (`Emit` or conditional `Override`), naming the writer and the fix.
Missing format (row 7) → error at the payload naming the word and the owner statement (alpha
`Json<T>`/`Text`/`Binary` and `-> ()` only change the hint).

**Codegen (`emit.rs`).**

- `format_aliases`: one `type __Concord{Owner}{Request|Response}Format = <format>;` per default
  **with consumers**, gated by `#[cfg(any(all(<consumer 1 cfgs>), …))]` (no attribute when one
  consumer is unconditional; none emitted without consumers). The alias ident carries the default's
  span (user context) and consumers name it relocated at their payload type — with the macro's own
  context rustc pointed an unsupported payload at the whole invocation; with this, at the payload.
- Body: `static __FORMAT: BodyFormat<T> = BodyFormat::new::<F>()` or
  `BodyFormat::with_media_type::<F>(HeaderValue::from_static("…"))`, located at the word (in place)
  or the payload (inherited). `BodyFormat::new` is a generic `const fn` that runs
  `HeaderValue::from_static(E::CONTENT_TYPE)`, so an invalid constant fails the static's evaluation
  — a compile error under `cargo check`, on 1.97.1 and 1.85 (§10) — and the trait bound is
  referenced once (one E0277 per unsupported body). The per-request parsing in `encode_body` is
  gone; requests clone the static value.
- `EndpointMeta::sensitive_content_type` = the composed `content-type` slot's static flag (sticky,
  also after `remove`).
- Rustdoc (12e): every endpoint's navigation method states body format, origin, media type (or the
  8b header rule, or the format's `CONTENT_TYPE`), absence for optional bodies, output format and
  origin, "none … the `response` default `json` of … does not apply" without `->`, and for
  `-> ()` what is decoded and that no `->` decodes nothing. Clients, scopes and scope navigation
  methods state the defaults that reach their endpoints (declared here / inherited, media type).

**Runtime (`concord`).** `BodyFormat<T>`, `encode_body(&'static BodyFormat<T>, …)`, `Parts::finish`
implementing the 8b table and 8f flag; `Form` (Encode + Decode via `serde_urlencoded`,
`application/x-www-form-urlencoded`, content-free error descriptions) behind the non-default
`form` feature; `Encode`/`Decode` docs and `on_unimplemented` texts reworded around formats
("the format `{Self}` cannot decode a `{T}` response", label naming in-place-or-inherited).

## 4. Contract rows → tests

`formats.rs::` unless stated. All pass in `scripts/verify.sh --msrv` at `6b7fca0` (§9).

| Row | Implemented | Test |
|---|---|---|
| 1 | no body ⇒ no `Content-Type` from the format | `row1_no_body_under_a_request_default` |
| 2a | `Option` body `None` ⇒ `null` + `application/json` | `row2a_required_option_body_is_json_null`, `bodies.rs::r02_*` |
| 2b | absent optional body ⇒ no body, no CT | `row2b_absent_optional_body_sends_nothing`, `forms.rs::pairs_and_form_default` |
| 2c | empty `text`/`bytes` present with CT | `row2c_empty_text_and_bytes_bodies_are_present` |
| 2d | body initializer rejected (+ one body, endpoints only) | `ui/body_errors.rs` |
| 3a | no `->` drains any 2xx under a default | `row3a_no_output_drains_under_a_response_default` |
| 3b/3c | `-> ()` / `-> json ()` decode a unit; empty 204 = decode error; rustdoc says so | `row3b_3c_typed_unit_is_decoded`, `emit::tests::format_docs_state_format_media_type_and_origin` |
| 4a/4b | JSON string vs raw text | `row4_json_string_versus_raw_text` |
| 5 | aliases (`Raw`, `Unit`, `JsonDoc`, `formats::Wire = concord::Text`) follow written format only | `row5_aliases_follow_the_written_format` |
| 6a/6d | independent directions, nearest owner, no clearing (scope inherits) | `row6a_nearest_owner_per_direction` |
| 6b | in-place exception replaces format and media type | `row6b_in_place_exception_with_media_type`, `row8a_body_media_types` (`plain`) |
| 6c | endpoint `request`/`response` → error | `ui/format_endpoint_statement.rs` |
| 7 | missing format → error at payload, both remedies | `ui/representation_missing_via.rs`, `ui/representation_alpha_spelling.rs` |
| 8a | declared (`FORMAT("…")`, `body("…")`, default's) or format's media type, sent exactly with the body | `row8a_body_media_types` |
| 8b | four runtime cases; empty native value present; empty `each` nothing; conversion failure build error | `row8b_four_runtime_cases`, `row8b_empty_value_empty_each_and_conversion_failure` |
| 8c | conflict rejected (in place, `body("…")`, default media, conditional override); `remove` resolves | `ui/format_header_media_conflict.rs` (fail), `row8c_remove_resolves_the_conflict` (pass) |
| 8d | no `Accept`; response CT unchecked | `row8d_no_accept_and_no_response_type_check` |
| 8e | output media type → error (in place and in `response` default) | `ui/format_output_media_type.rs` |
| 8f | inserted CT flagged per key's static class (marker; sticky after `remove`): flag, `Prepared` Debug `<redacted>`, sent; control unflagged | `row8f_inserted_content_type_is_classified` |
| 8g | macro-checked media types; `CONTENT_TYPE` checked at compile time; not read where replaced; `None` sends none | `ui/format_media_type_invalid.rs`, `ui/format_invalid_content_type.rs`, `representations.rs::declared_media_type_replaces_an_invalid_constant`, `row8a_body_media_types` (`Untyped`) |
| 9 | custom codec by path in place and as default; errors at word / payload; serde bound by rustc | `row9_custom_codec_in_place_and_as_default`, `ui/representation_unsupported_payload.rs`, `ui/format_inherited_unsupported.rs`, `ui_json/format_serde_bound.rs` |
| 10 | native exits unchanged | `bodies.rs::r08_raw_response_streaming`, `examples/multipart`, `examples/native` |
| 11 | status/decoder unchanged | `row11_status_and_native_decoder_unchanged`, `representations.rs::decoder_applies_status_handling` |
| 12a | one error per in-place word, one per used default, rustc's gated note | `ui_features/feature_disabled_formats.rs` (runs in the no-default lanes; 5 errors: 2 `json` + 1 `form` in place, 1 per default) |
| 12b | alias gated by consumers' cfg union; disabled-only or unused defaults never referenced | `row12b_unused_and_cfg_disabled_defaults_are_never_referenced` (compile-pass with missing types), `ui_features/…` (no error for those defaults) |
| 12c | renamed dependency, no imports | `examples/renamed::formats_through_the_renamed_crate` (+ `concord4::Binary` path) |
| 12d | leftover `via` → exact replacement; migration script | `ui/format_via_removed.rs`, `scripts/migrate_via.py` |
| 12e | rustdoc per endpoint and owner | `concord-macros` `emit::tests::format_docs_state_format_media_type_and_origin`, `format_docs_state_the_header_rule` |
| Lexical table | every compiling row on the wire; `-> json {` reserved; `-> r#json` payload | `lexical_precedence`, `lexical_raw_identifier_is_a_payload`, `ui/format_reserved_words.rs`, `ui/format_swapped_order.rs` |
| CHARTER §7 | optional native `content-type`/other header vs body presence; defaults vs header presence and inheritance (identical header maps with/without defaults); natively sensitive header under defaults | `h11_optional_native_headers_do_not_affect_body_presence`, `h12_format_defaults_do_not_affect_header_presence_or_inheritance`, `native_sensitive_header_stays_protected_under_defaults` |
| N08 form | encoding, `None` omitted, pairs order, default, form response, encode/decode errors content-free, form in/JSON out | `forms.rs` (form-only lane: 4; form+json: 5) |

`v4/TEST_MATRIX.md` gains the same mapping as rows F1–FF (status "track"). `CONTRACT.md` itself was
left as authored (its `T:` notes are not filled in; the mapping is here and in the matrix).

## 5. Deviations from the contract (and choices it left open)

1. **8g, stricter than "wherever it may be sent".** A format's `CONTENT_TYPE` is evaluated at compile
   time whenever the body has no declared media type, even if a `content-type` contribution that
   is always present would replace it at runtime. Proving "always present" needs more than slot
   membership (`each` over an empty collection, conditional fallbacks); validating an unused
   constant is harmless. Where a declared media type replaces it, it is not read (as specified).
2. **12a wording.** rustc prints "cannot find type `Json` in crate `$crate`" (the word expands
   through `$crate`); counts and the "gated behind the `json` feature" note are as specified.
3. **Invalid `CONTENT_TYPE` wording.** rustc's `E0080` ("evaluation panicked: HeaderValue::from_static
   with invalid bytes", "evaluation of `ApiOdd::prepare::__FORMAT` failed") points at the format on
   1.97.1; on 1.85 the primary span is in `http`'s source with a final note at the format. Not a
   Concord message.
4. **12e placement.** The per-endpoint format lines are on the endpoint's navigation method (where
   the request summary already was), not repeated on the request struct; owners carry their
   defaults on the struct and (scopes) on the navigation method.
5. **3b rustdoc** adds its `-> ()` sentence only when the output is spelled `()`; an alias
   (`-> Unit`) gets the ordinary output line (decoded with … ). Spelling never affects behaviour or
   acceptance.
6. **Removed runtime tests.** `representations.rs::invalid_content_type_is_a_build_error` (ADV-7)
   and `content_type_error_precedes_path_error` (C4-R-015) tested a runtime check that no longer
   exists; replaced by `ui/format_invalid_content_type.rs` and
   `declared_media_type_replaces_an_invalid_constant`; the matrix row was updated.
7. **`examples/native` keeps its downstream `Form`**, now written `repr::Form fields: …` (a path, so
   never confused with the word). Switching it to the built-in would enable `form` through workspace
   feature unification in the default `cargo test --workspace` lane; `form` is covered by its own
   lanes instead.
8. **Additions not requested explicitly:** `scripts/migrate_via.py` (the contract's 12d mentions a
   migration script; it never introduces defaults, maps `Json/Text/Binary` to words and keeps any
   other type, including a pre-formats downstream `Form`, as a path), `v4/TEST_MATRIX.md` F rows,
   MAINTAINERS notes on blessing feature-gated UI suites, and CI's MSRV list now includes
   `native_headers` (verify.sh already did).

## 6. Diagnostics (rustc 1.97.1, from the UI snapshots)

```text
error: output `Item` has no format, and no `response` default reaches this endpoint: write the format in place (`-> json Item`) or declare a default on the client or an enclosing scope (`response json;`); an endpoint without typed output omits `->`
error: body `item` has no format, and no `request` default reaches this endpoint: write the format in place (`json item: Item`) or declare a default on the client or an enclosing scope (`request json;`)
error: `-> concord::Json<Item>` is the V4-alpha spelling: write the format before the payload type, `-> json Item` (or `-> Item` under a `response json;` default on the client or an enclosing scope)
error: `via` was removed: the format comes before the type, `-> json Item` (or `-> Item` under a `response json;` default on the client or an enclosing scope)
error: `via` was removed: the format comes before the name, `text note?: String` (or `body note?: String` under a `request text;` default on the client or an enclosing scope)
error: an endpoint has no `response` statement: write the format where the output is declared (`-> text String`, `-> bytes Bytes`); `response FORMAT;` is a default on a client or scope
error: an output takes no media type: Concord sends no `Accept` and does not check the response `Content-Type`; to send an `Accept` header, declare it (`headers { "accept" = "..."; }`)
error: body `p` has the media type "application/vnd.api+json" (from the `request` default of the client `FromDefault`), and the `content-type` header set by `FromDefault` reaches this endpoint: both would set `Content-Type`; keep one: write the body's format in place without a media type (`json p: ..`), or add `remove "content-type";` to this endpoint's `headers` block
error: the output type is missing after `json`: `-> json Item` (format words are reserved in format position; a payload type named `json` is written `r#json`)
error: `csv` is not a format: the built-in formats are `json`, `text`, `bytes` or `form`; a format type is written as a path (`self::csv`)
error: `qurey` is neither a placement (`query`, `header("..")`) nor a body format: the built-in formats are `json`, `text`, `bytes` or `form`; a format type is written as a path (`self::qurey`)
error: the format comes first: `-> json Vec<Item>`
error: a body's format comes first: `json item: Item`
error: "text/plain; charset" is not a media type: a parameter is not `name=value` (expected `type/subtype` with optional `; name=value` parameters, in visible ASCII, e.g. "application/merge-patch+json")
error[E0277]: the format `Text` cannot decode a `Item` response
   |         GET item() -> Item;      (inherited `response text;`: at the payload)
   |                       ^^^^ this output's format is `Text` (written here or inherited from a `response` default); it needs `Text: Decode<Item>`
```

## 7. UI snapshots

Re-blessed existing files (wording intentionally changed): `body_errors` (.rs + .stderr; "a body",
three new cases), `representation_alpha_spelling` (.rs + .stderr; alpha hints now name formats,
body case added), `representation_missing_via` (.rs + .stderr; row 7 message, body case, direction
independence), `representation_unsupported_payload` (.rs + .stderr; in-place words, new
`on_unimplemented` text), `sensitive_body` (.rs + .stderr; span on the word),
`statement_after_child.stderr` and `unknown_method.stderr` (the statement list now includes
`request`, `response`). No other existing snapshot changed.

New: `ui/format_{via_removed, endpoint_statement, output_media_type, header_media_conflict,
media_type_invalid, invalid_content_type, inherited_unsupported, reserved_words,
swapped_order}.rs`, `ui_json/format_serde_bound.rs` (runs where `json` is on),
`ui_features/feature_disabled_formats.rs` (runs where neither `json` nor `form` is on). `ui/` stays
feature-independent: it runs, unchanged, in every lane.

## 8. Mutation log (baseline pass → mutant fails → restored pass)

Harness: `evidence/mutation/{mutate.py,run.sh}` (applies the named mutant with an exact-match
replacement, runs `cargo test --offline --locked [--no-fail-fast] -p concord …`, restores with
`git checkout`, re-runs; the tree is clean afterwards). Head `ef0de89`.

| Mutant | Change | Command (`-p concord`) | Baseline | Mutant | Restored |
|---|---|---|---|---|---|
| M1 drain under a default | no `->` decodes `()` with the nearest `response` default instead of draining | `--test bodies` | PASS | FAIL: `r04_empty_responses`, `r02_*`, `optional_body_controls` (JSON decode error on 204) | PASS |
| M1 (formats) | same | `--test formats --test bodies` | PASS | FAIL (fixtures no longer compile: `bytes` cannot decode `()`, defaults of disabled endpoints referenced) | PASS |
| M2 CT only with a body | an absent optional body becomes an empty body with the format's media type | `--test formats` | PASS | FAIL: `row2b_*`, `row8a_*`, `row8b_four_runtime_cases`, `h12_*` | PASS |
| M2 (bodies) | same | `--test formats --test bodies` | PASS | FAIL: `r02_no_body_vs_empty_vs_null` | PASS |
| M3 inserted CT sensitivity | `Parts::finish` ignores `sensitive_content_type` | `--test formats` | PASS | FAIL: `row8f_inserted_content_type_is_classified` (`value.is_sensitive()`) | PASS |
| M4 8c conflict | `check_media_conflict` returns immediately | `--test ui` | PASS | FAIL: `format_header_media_conflict.rs` "Expected test case to fail to compile, but it succeeded" | PASS |
| M5 alias cfg gating (extra) | aliases emitted without the consumers' cfg union | `--test formats` | PASS | FAIL: E0425 `DoesNotExist`/`AlsoMissing`/`Missing` (12b fixture) | PASS |

## 9. Verification

`scripts/verify.sh --msrv` on the committed tree `6b7fca0` (clean): **18/18 PASS** — fmt, clippy
`-D warnings`, test (533), test-no-default (368, includes `ui_features`), test-json-only (410),
test-reqwest-defaults-only (370), test-form-only (371), test-form-json (5), feature-isolation,
test-multipart-alone (11), doc ×5 (default, no-default, json-only, form-only, default+form),
msrv-concord (416, with `form`, incl. `formats`/`forms`), msrv-examples (95), msrv-multipart-alone
(11). Logs: `evidence/verify/`. The optional nightly lane (`cargo +nightly test -p
concord-clone-compat`) also passes.

## 10. Expansion size and check time (rough sanity, not a benchmark)

`benches/scale/gen.py api … typed --endpoints 200` (160 endpoints inheriting `request text;
response text;`, 40 in-place exceptions; `--syntax via` against a `git archive` of `c112845`) and
`rest --endpoints 200` (no bodies/outputs) as a control. Nightly `-Zunpretty=expanded`; hyperfine
`cargo check` after `touch`, deps warm, 2 warm-ups + 10 runs, load average ≈3.8 (machine in use).

| Crate | Expanded lines | Expanded bytes | `cargo check` mean ± σ |
|---|---|---|---|
| typed, `via` @c112845 | 23 126 | 859 739 | 224.7 ± 3.5 ms |
| typed, formats @HEAD | 24 095 (+4.2%) | 919 840 (+7.0%) | 226.3 ± 3.2 ms |
| rest, `via` @c112845 | 39 150 | 1 490 280 | 354.2 ± 6.3 ms |
| rest, formats @HEAD | 39 350 (+0.5%) | 1 499 680 (+0.6%) | 356.0 ± 5.4 ms |

The growth is the longer generated rustdoc (format, origin, media type) plus one `BodyFormat` static
per body and one `EndpointMeta` field per endpoint; no measurable check-time difference. An
invalid `CONTENT_TYPE` is a compile error on 1.85 too (`evidence/scale/msrv_invalid_content_type_1.85.txt`).

## 11. Migration

Every `api!` in crates/ (tests, UI sources), examples/ (shapes s1 in place, s2/s3 defaults,
quickstart and caller guide `request json; response json;`, native with defaults + `Ndjson`/
`repr::Form`/`bytes` exceptions, renamed with words and a `concord4::Binary` path, multipart),
checks/ (heap, clone-compat incl. compile_fail doctests) and benches/criterion. `gen.py` emits
formats by default; `--syntax via` reproduces the `c112845`-era control and `--syntax alpha` still
works; existing shapes are byte-identical to the previous generator in every syntax (checked with
`diff -r`). Historical records under `v4/refinement/`, `v4/evidence/`, `v4/EXAMPLES.md` untouched.

## 12. Open issues

- Under a JSON default, forgetting `-> text String` / `text note: String` for a payload both
  formats accept is a runtime error, not a compile error (accepted risk, N06); the rustdoc is the
  mitigation.
- rustc-origin wording: `$crate` in the feature-off error; internal `__FORMAT` name in the E0080 for
  an invalid `CONTENT_TYPE`.
- Pre-existing wart (unchanged): a `///` comment directly before an owner statement (now including
  `request`/`response`) is parsed as a child attribute and reported as an unexpected statement.
- `form` is not exercised by `cargo test --workspace` (no member enables it, by design); it is
  covered by the dedicated lanes. CI's Windows job was not run locally.
- Not re-measured: Criterion runtime numbers (`benches/criterion` compiles; body encoding now goes
  through a `fn` pointer held in the per-endpoint static, like `Decoder` already did for decoding)
  and the full `ab.sh` suite with the new `scale_typed` rows.
- `CONTRACT.md`'s `T:` annotations were not filled in (decision record left as authored); the
  row → test mapping is in §4 and `v4/TEST_MATRIX.md`.
