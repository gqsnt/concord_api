# Review triage

## The owner's supplied review (dispositions R1–R7 from the handoff)

| ID | Proposal | Disposition | Implemented | Evidence (fails on the old behaviour) |
|---|---|---|---|---|
| R1 | Riot's live client must not follow redirects | Done | `93f9b0e`: `http_policy` (`redirect::Policy::none()`, timeout) and `live_http_client` (HTTPS only), used by `main` | `examples/riot/tests/redirects.rs`: `send()` → `Status` 302 with the unread response; `send_raw()` → 302; the second origin receives nothing. Control: reqwest's default policy forwards `X-Riot-Token` to the second origin |
| R2 | Validate all native header names without calling endpoints | Done | `25a52f7`: `Client::validate_header_names()`; `HeaderPlan` split into a presence-free structural pass shared with `prepare()` | `crates/concord-core/tests/header_plan.rs::the_validator_and_prepare_share_one_verdict`; `crates/concord/tests/validate_names.rs` (counters: no default, `Display` or encoder runs; uninhabited body type; each problem equals `prepare()`'s error); Riot; a 480-endpoint generated SDK |
| R3 | Seal `NativeKey` | Done | `3a836e7`: private supertrait, `HeaderName` only | `ui/native_key_sealed.rs` (the same downstream impl compiles against `dfdeed8`), `ui/native_key_deref.rs` |
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
