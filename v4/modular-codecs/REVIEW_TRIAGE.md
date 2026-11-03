# Review triage

## The owner's supplied review (dispositions R1–R7 from the handoff)

| ID | Proposal | Disposition | Implemented | Evidence (fails on the old behaviour) |
|---|---|---|---|---|
| R1 | Riot's live client must not follow redirects | Done | `93f9b0e`: `http_policy` (`redirect::Policy::none()`, timeout) and `live_http_client` (HTTPS only), used by `main` | `examples/riot/tests/redirects.rs`: `send()` → `Status` 302 with the unread response; `send_raw()` → 302; the second origin receives nothing. Control: reqwest's default policy forwards `X-Riot-Token` to the second origin |
| R2 | Validate all native header names without calling endpoints | Done | `25a52f7`: `Client::validate_header_names()`; `HeaderPlan` split into a presence-free structural pass shared with `prepare()` | `crates/concord-core/tests/header_plan.rs::the_validator_and_prepare_share_one_verdict`; `crates/concord/tests/validate_names.rs` (counters: no default, `Display` or encoder runs; uninhabited body type; each problem equals `prepare()`'s header-composition error; an earlier build step can fail first: `earlier_build_steps_fail_first`); Riot; a 480-endpoint generated SDK |
| R3 | Seal `NativeKey` | Done | `3a836e7`: private supertrait, `HeaderName` only | `ui/native_key_sealed.rs` (the same downstream impl compiles against `dfdeed8`; its snapshot degenerated to an import error at the crate split `7ec1f4b` and was repaired after the final semantic review, S1), `ui/native_key_deref.rs` |
| R4 | Static codec-error labels, numeric context, explicit source | Done | `ca545f0`: `CodecError::new(&'static str)`, `.at(name, u64)` (3 positions), `.with_source`, `source_unredacted()`; every creation site migrated (a dynamic label no longer compiles) | `concord-core` `compile_fail,E0308` doctest; `crates/concord-extras/tests/media.rs::decode_errors_show_positions_not_content`; `examples/native/tests/bodies.rs` (NDJSON record/line/column); `d15_surfaces.rs` S09 shows a deliberate `leak` is still possible (trust boundary) |
| R5 | Index native-key occurrence provenance | Done | `a4b465a`: each `KeyPath` records its owner when its contribution is recorded | `emit::tests::key_occurrence_owner_matches_a_scan_of_every_owner`; expansion timings in PERF |
| R6 | Typed hierarchical path segments | **Deferred** (optional gate) | `1f26e91`: maintained native recipe `write_resource_segments` | Terra 6.1 XHigh test-only prototype: a static-index mutant mis-redacts; frozen report and model in the work directory |
| R7 | Local/CI MSRV selections drifted | Done | `013de15`: one inventory (`scripts/verify.sh`) called by CI; minimum = pin = 1.99.1 | `scripts/check-ci.py` (fails on a cargo command in `ci.yml` or a stale toolchain selector; checked by adding a `cargo +1.85 test` line) |

## Final executable audit (fresh Sol 6.1 XHigh, candidate `9f8939f`)

Observed settings `gpt-6.1-sol`, `xhigh`; report [`final-exec-sol-REPORT.md`](reports/final-exec-sol-REPORT.md).
No blocking finding. All 33 gate steps passed in the auditor's run; its independent downstream
format, validator/`prepare()` presence comparisons, confidentiality probes, `NativeKey` and
collision negatives, isolated consumers, package contents and Riot redirect tests held.

| ID | Severity | Finding | Disposition | Evidence |
|---|---|---|---|---|
| X-F1 | should fix | A quoted `charset="utf-8"` followed by another parameter was refused by `Json`, `Text`, `Form`: the pinned `mime` 0.3.17 reads the value as `"utf-` | Fixed: extras read `charset` parameters with an RFC 9110 parameter scanner (tokens, quoted strings, quoted pairs); `mime` still parses type and subtype | `crates/concord-extras/tests/media.rs` (charset module), `metadata.rs::charset_parameters_are_read_exactly`; the auditor's `audit/probe` suite passes against the fix (5/5, previously 3/5) |
| X-F2 | should fix | `charset=utf-8; charset=iso-8859-1` was accepted (only the first parameter read) | Fixed: a repeated `charset` is refused as ambiguous by every format that checks it | same tests |

## Fresh-use trials (R07)

Both trials completed the two tasks (a vendor format with a version header and an NDJSON decoder;
validating header names and breaking one endpoint) from the documentation alone, with no
unprovoked compiler error. Reports: [`fresh-terra-REPORT.md`](reports/fresh-terra-REPORT.md),
[`fresh-sonnet-REPORT.md`](reports/fresh-sonnet-REPORT.md).

| Source | Note | Disposition |
|---|---|---|
| Terra | no example narrows to one exact vendor media type | Added: `crates/concord-extras/tests/media.rs` (`vendor` module) quoted in `docs/CUSTOM_FORMATS.md` §5 |
| Sonnet | a payload-generic format's methods need `<F as Decode<T>>::` (E0283) | Documented (`docs/CUSTOM_FORMATS.md` §9) |
| Sonnet | `validate_header_names().unwrap()` printed a struct dump | Fixed: `HeaderNameErrors`'s `Debug` lists the problems as lines; author guide shows the text and `reason()` |
| Sonnet | `with_base_url` replaces the base path too | Documented (QUICKSTART) |
| Sonnet | exposing head state to the caller | Documented: a wrapper output type (`docs/CUSTOM_FORMATS.md` §5) |
| Both | naming (`ErrorKind::Metadata`, `Decode::context`, `validate_header_names`, `CodecError`) | Kept: chosen with both architecture challengers and both journey probes; Terra found them reasonable once read, Sonnet would prefer other names. Recorded as feedback, not changed in this increment |

## Final semantic review (side Opus, candidate `9f8939f`)

Observed model `claude-opus-5-5` (164 transcript turns); report
[`final-sem-opus-REPORT.md`](reports/final-sem-opus-REPORT.md). The reviewer proposed (did not
implement) the contract before implementation and did not read the executable audit. No blocking
finding.

| ID | Severity | Finding | Disposition | Evidence |
|---|---|---|---|---|
| S1 | should fix | `ui/native_key_sealed.rs` imported `concord::__private`, gone since the crate split: its snapshot became an import error and no longer tested the seal | Fixed: imports through `concord::__macro::concord_core::__private`; snapshot shows E0277 on `sealed::Sealed`; a mutant without the supertrait fails the test | `crates/concord/tests/ui/native_key_sealed.rs`; work directory evidence/mutants/S1_unsealed.txt |
| S2 | should fix | `validate_header_names()` docs claimed its problem is the error `prepare()` returns for every request; `prepare()` encodes the body and formats the URL first | Fixed (wording, MC19): the claim is narrowed in rustdoc (core and generated), AUTHOR_GUIDE §4.9, DECISIONS MC14 | `validate_names.rs::earlier_build_steps_fail_first` |
| S3 | should fix | docs said a head-based decision cannot be made after the body is read; a `Context` can carry head data into `decode_body` | Fixed (wording): refusals belong in the first two steps; a refusal in `decode_body` is `Decode`, after the read | `Decode` rustdoc, CUSTOM_FORMATS §3, AUTHOR_GUIDE §5.7, MIGRATION |
| S4 | should fix | MIGRATION did not say a reqwest client's default `Accept` is now replaced on typed `Json`/`Form` endpoints | Fixed: MIGRATION and AUTHOR_GUIDE §5.8 state it with the remedies | `accept.rs::the_wire_and_the_client_defaults` |
| M1 | minor | error docs said output is purely static; numeric positions are format-chosen numbers | Fixed: `Error` rustdoc, CALLER_GUIDE §6 | — |
| M2 | minor | `IgnoreContentType` is only as narrow as documented if `context` does not refuse on `Content-Type` | Fixed: `Decode::context` docs say it may read it for state, not refuse on it | — |
| M3 | minor | RECIPES error-document text was stale (the example already guards with `status()`) | Fixed | `examples/native/src/errors.rs` |
| M4 | minor | strict `Json` refused RFC 9110 forms (`application/json ; charset=utf-8`, `;;`) because `mime` does | Fixed (MC17): `mime` parses type and subtype only; the scanner reads parameters | `crates/concord-extras/tests/media.rs` (`charset::the_parameter_grammar_is_rfc_9110`) |
| M5 | minor | after `into_parts`, `.header(ACCEPT, v)` appends to the suggestion | Documented (MIGRATION, AUTHOR_GUIDE §5.8): `headers_mut().insert` replaces | — |
| M6 | minor | stale small docs (macro first token, MAINTAINERS current campaign, core dependencies, `extras` feature list); `CODEC_ERROR_CONTEXT` not re-exported | Fixed; the facade re-exports `CODEC_ERROR_CONTEXT` | — |
| M7 | minor | a stray empty file `A` tracked at the root since `df37383` | Removed | — |
| M8 | minor | `concord-macros` had no README | Added, with the `readme` field | `scripts/package-check.sh` lists packaged files |
| notes | note | `each` over an empty `accept` list sends the suggestion; strict `Json`'s charset policy is stricter than RFC 8259 requires; `ContentTypeField` `Debug` prints its value; the path deferral had no decision row | Row added to AUTHOR_GUIDE §5.8 and `accept.rs`; the charset policy is deliberate (owner: strict by default, `IgnoreContentType` as the explicit escape); `Content-Type` is not a secret; MC16 | — |

## Targeted re-review of the audit corrections (fresh Sol 6.1 XHigh, `4ce3d01`)

Observed settings `gpt-6.1-sol`, `xhigh`; report [`rereview-sol-REPORT.md`](reports/rereview-sol-REPORT.md).
X-F1 and X-F2: resolved (the auditor's original reproducers pass; 58 cases × 3 formats on the
wire, 24,576 stress inputs, no panic or hang; refused responses unread, no header content in
errors; `HeaderNameErrors` `Debug`/`Display` agree). The re-review ran on `4ce3d01`, before the
semantic-review corrections, while `mime` still parsed the parameters.

| ID | Severity | Finding | Disposition | Evidence |
|---|---|---|---|---|
| RR-1 | should fix | `Text` accepted a readable foreign charset when `mime` refused valid parameter syntax (`text/plain;\tcharset=iso-8859-1`, `charset=""`) | Fixed by `d98e4cf` (MC17: `mime` parses type and subtype only) | the re-reviewer's probe, re-pointed at `530b6c1`: 7/7 (was 5/7); `charset::the_parameter_grammar_is_rfc_9110` |
| RR-2 | should fix | `Json`/`Form` refused valid UTF-8 metadata (`;\tcharset=utf-8`, empty parameters, `audit=""`) | Fixed by `d98e4cf` (same) | same |
| RR-3 | minor | `Text` docs did not mention the repeated-charset refusal | Fixed by `d98e4cf` | `Text` rustdoc, CUSTOM_FORMATS |
| matrix | note | after the fix, the only cases where the scanner's verdict and a format's decision differ are two `Text` cases whose field has a non-ASCII byte (`audit="é"`): `Text` reads such a field as opaque and accepts it, as it does any malformed field | Kept; documented (`Text` rustdoc, CUSTOM_FORMATS) | the probe's matrix on `530b6c1` (work directory evidence/rereview-sol-probe_530b6c1.log) |

The vendor example (`AcmeV2`, CUSTOM_FORMATS §5) re-parsed the whole field with `mime`, inheriting
its parameter limits; it now compares the text before the first `;`, which `Json` has already
validated.

## Re-review of the semantic-review corrections (side Opus, `530b6c1`)

Observed model `claude-opus-5-5`; report [`rereview-opus-REPORT.md`](reports/rereview-opus-REPORT.md).
S1–S4 and M1–M8: all closed (S1 checked with the real UI suite on a pristine copy, 77/77, and on an
unsealed mutant, 76/77 with `native_key_sealed` failing; P1–P6 rerun; a 55-input comparison of the
parameter scanner with `mime` found the scanner right wherever they differ). No blocking finding.

| ID | Severity | Finding | Disposition | Evidence |
|---|---|---|---|---|
| S2 residual | minor | four places still said the problem equals `prepare()`'s error without the qualifier | Fixed (AUTHOR_GUIDE §4.9, `NameCheck::endpoint` docs, the test comment, row R2) | — |
| MC19 | note | outcome accepted; the rationale understated the cost (values decide which error an unbuildable endpoint reports; an encoder runs first) | MC19 rewritten to state the trade-off and the disagreement | — |
| MC17 | note | the scanner is a second, hand-written parser; say so | MC17 says so | — |
| N1 | minor | AUTHOR_GUIDE §5.1, MIGRATION, SEMANTICS §7 and the extras crate table did not mention the repeated-charset rule | Fixed (`Json`, `Form`: charset absent or once and UTF-8) | — |
| N2 | minor | `Text` refused a repeated equal UTF-8 charset while accepting more ambiguous fields it cannot read | Fixed: `Text` refuses a readable non-UTF-8 charset, repeated or not, and nothing else (MC17) | `crates/concord-extras/tests/media.rs` (`charset::a_repeated_charset_is_refused`) |
| N3 | minor | CUSTOM_FORMATS §5 suggested a wrapper named like the format (`Versioned<T>`) and split an excerpt from its follow-up | Fixed: `WithVersion<T>`, paragraph moved after the follow-up | — |

## Second targeted re-review of all corrections (fresh Sol 6.1 XHigh, `3fa14b7`)

Observed settings `gpt-6.1-sol`, `xhigh`; report [`rereview2-sol-REPORT.md`](reports/rereview2-sol-REPORT.md).
X-F1, X-F2, RR-1..RR-3: resolved. The gate passed (650 tests); the sealing mutation discriminates
(exit 0 / 101 / 0); 231 further wire requests and 6,550 essence-split checks admitted no non-JSON
type through `Json`; refused responses unread, no header content in errors; the unpacked
`concord-macros` package carries its README. It ran before `abdad04` (the side Opus re-review's
`Text` change), so its matrix still expected a repeated UTF-8 charset to be refused by `Text`.

| ID | Severity | Finding | Disposition | Evidence |
|---|---|---|---|---|
| RR2-1 | should fix | `Text` treated a field with an empty subtype (`text/; charset=latin1`) as readable, because `mime` accepts it, and refused it, contradicting "malformed is accepted" | Fixed: the type and subtype must be non-empty RFC 9110 tokens before `mime` parses them (MC17) | `charset::the_parameter_grammar_is_rfc_9110`; the reviewer's extended probe on the fix: all 24 empty-subtype rows as documented (work directory evidence/rereview2-probe-extended-final.log; its remaining differences are `abdad04`'s deliberate `Text` change) |
| RR2-2 | minor | `Form`'s docs omitted the accepted `us-ascii` | Fixed (`Form` rustdoc, CUSTOM_FORMATS, the summary tables: UTF-8 or US-ASCII) | — |
| RR2-3 | minor | MIGRATION said typed `Form` endpoints send `application/json` | Fixed: each format's own suggestion | `accept.rs` |
| RR2-4 | minor | DIAGNOSTICS still said a duplicate native header fails `prepare()` on every call unconditionally | Fixed: unless an earlier build step fails first | `validate_names.rs::earlier_build_steps_fail_first` |

## Third targeted re-review of the last delta (fresh Sol 6.1 XHigh, `d28ad34`, narrow)

Observed settings `gpt-6.1-sol`, `xhigh`; report [`rereview3-sol-REPORT.md`](reports/rereview3-sol-REPORT.md).
Scope: `3fa14b7..d28ad34` (`Text`'s charset rule, the token check of type and subtype, the
documentation delta). No blocking, should-fix or minor finding. RR2-1..RR2-4 resolved. 46 tests
and probes passed (extras, the facade's `metadata`, `accept` and `validate_names`, the previous
re-review's probes with two expectations adjusted to the deliberate `Text` change, a new delta
probe: 2,025 wire requests, 0 mismatches); 55 withheld-body refusals stayed unread; no header
content in errors. It did not run the full gate (the coordinator's gate on `d28ad34`: 34/34, 650).
