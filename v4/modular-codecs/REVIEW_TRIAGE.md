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
