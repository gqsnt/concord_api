# A2-derive-error — A errors — Derive mechanical error formatting and conversions
Branch / commit: spike/A-derive-error at 5b173acb (base ba10189b)
Targets (3 files): witness `examples/salesforce-sdk/src/error.rs`; counterexample `examples/discord-rest-sdk/src/error.rs`; simple control `examples/frankfurter-sdk/src/error.rs` (byte-identical).
C1: yes — declarations are unchanged; conversion skips live outside `api!`.
C2: yes — mechanical impls derive; classification, recovery and custom formatting remain Rust.
C3: yes after fixes — controls unchanged; moving the derive removes Frankfurter's new rustdoc ambiguity.
C4: yes after fixes — hygienic bindings and rejected dynamic format counts remove ambient capture.
C5: yes after fixes — unsupported helper placements and formatting forms diagnose authored code.
C6: yes — a derive and short helper contract; Discord exposes its limited savings.
C7: yes — native exits and unread-response handling remain.
C8: yes — generated impls add no I/O, retry or state.
C9: no — the final placeholder validator rejects valid Unicode field names; import compatibility lacks a dedicated regression test.
What disappears: error glue 2,313 → 2,245 (−68) across Salesforce, Backblaze B2, GitHub and Discord; net source follows the error-module reductions. Salesforce's −55 includes 15 independently replaceable std-Debug lines: A2's own share is −40 there and −53 across the migrated SDKs. Discord grows +3.

Measurements: M1–M11, baseline → spike. Saved aggregate includes migrated SDKs and simple controls; per-SDK results: [measurement](/home/canarit/projects/Concord_Next/work/wt/A2-derive-error-measure.md); evidence: [report](/home/canarit/projects/Concord_Next/work/design/A2-derive-error/REPORT.md).

| Measure | Baseline → spike |
|---|---|
| M1 glue | 3,885 → 3,817 (−68), entirely error glue 2,313 → 2,245. |
| M2 model / behavior | Model 2,649 → 2,649; behavior 4,347 → 4,347. |
| M3 concepts | One derive + 2 helper attributes (`display`, `from`); reported ≈23 glue lines/concept includes the unrelated std-Debug saving. Final path `concord::derive::Error`; macro source 398 → 427 nonblank lines after fixes. |
| M4 declaration | DSL lines/endpoint 5.33 → 5.33; tokens/endpoint not measured. |
| M5 negations | 0 → 0 in declarations; `#[from(skip)]` is an opt-out outside `api!`. |
| M6 controls | Nager 8 → 8; PokeAPI 7 → 7; Frankfurter 11 → 11 DSL lines; no added concepts; rustdoc ambiguity fixed. |
| M7 caller docs | Per-endpoint comparison not measured; reported 70 + 14 added item-doc words; endpoint generation unchanged. |
| M8 performance | Compile time, expanded lines and allocations: not measured. |
| M9 caller changes | 0 existing call sites changed; no existing expected values changed. |
| M10 tests | SDK baseline 242 → 242 passes; final core tests 518; canaries, clippy, fmt and rustdoc clean; added runtime/UI fixtures. Full workspace gate not reported. |
| M11 diagnostics | Baseline not measured → duplicate sources, missing/invalid display, wrong hook, kept impl, skipped conversion, missing Debug and misplaced helpers diagnose authored code without internals; final Unicode rejection is a correctness regression. |

Counterexample handling: Discord retains an enum-wide formatter hook and grows +3; B2/GitHub derive conversions and empty Error impls but keep custom formatting. Frankfurter, Datadog and GitLab cannot adopt because the derive supplies no source chain.
Breaking changes: no migrated caller or formatting changes found; the root `concord::Error` macro import/doc-link collision was fixed by moving the derive to `concord::derive::Error`. Dynamic width/precision is rejected; valid non-ASCII placeholders are still incorrectly rejected. Source-chain errors remain excluded.

Review:
- Round 1: [ACCEPT_WITH_FIXES](/home/canarit/projects/Concord_Next/work/reviews/spike-A2-derive-error-review-1.md).
- Finding, blocking: a field named `__formatter` shadowed the generated formatter.
- Finding, blocking: dynamic width/precision references escaped validation and could capture ambient values.
- Finding, blocking: field-level helper attributes were silently ignored.
- Finding, blocking: Salesforce's A2-only saving was misreported; −40 is Salesforce, −53 is the adjusted total.
- Finding, non-blocking: the root macro export also broke previously compatible Error imports.
- Fixes: mixed-site bindings/aliases, count-parameter rejection, field-attribute diagnostics, corrected attribution and derive namespace move, with regression fixtures.
- Round 2: [ACCEPT_WITH_FIXES](/home/canarit/projects/Concord_Next/work/reviews/spike-A2-derive-error-review-2.md), with a remaining blocking finding: valid Unicode identifiers are rejected; C9 no.
- Recorded limits: validate placeholders using Rust identifier rules and cover delegation/composite Unicode cases; add an import-compatibility compile-pass fixture. Reviewers did not execute builds/tests; M7 comparison and M8 remain not measured.

Agent's assessment:
- The mechanism works for delegation-only errors without source chains and changes no existing caller.
- Savings concentrate in Salesforce; custom hooks save little or grow, and source-chain errors cannot adopt.
- The namespace fix is in place, but the Unicode blocker remains; opt-in conversions and less restrictive formatting/source policies remain design proposals.
