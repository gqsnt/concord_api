# D1-derive-debug — D secrets — Derive redacted Debug from explicit sensitivity markers
Branch / commit: spike/D-derive-debug at db49b9ad (base ba10189b)
Targets (3 files): witness `examples/salesforce-sdk/src/ids.rs`; counterexample `examples/twilio-sdk/src/models.rs`; simple control `examples/pokeapi-sdk/src/lib.rs` (byte-identical).
C1: yes — sensitivity attributes state confidentiality facts; declarations gain no controls.
C2: yes — facts move beside fields; computed summaries and behavior remain Rust.
C3: yes — Nager, PokeAPI and Frankfurter remain unchanged.
C4: yes — explicit markers and syntactic Option recognition; no name-based inference.
C5: yes — authored attributes, derives and field types receive diagnostics; a standard-library macro note remains.
C6: yes — short formatting rules; D4 adds a second default through `#[public]`.
C7: yes — native access and request escape hatches remain.
C8: yes — no I/O, retries, global state or formatting of withheld payloads.
C9: yes after fixes — unsized-field parity is covered; formatting-inventory precision remains limited.
What disappears: redaction glue 448 → 157 (−291); total classifier glue −286; 48 of 103 manual Debug impls disappear in Salesforce, Twilio, Elasticsearch, Backblaze B2, QuickBooks and Xero; net nonblank SDK source −294. Computed summaries remain handwritten.

Measurements: M1–M11, baseline → spike. Saved aggregate covers the migrated SDKs and simple controls; per-SDK results: [measurement](/home/canarit/projects/Concord_Next/work/wt/D1-derive-debug-measure.md); implementation and test evidence: [report](/home/canarit/projects/Concord_Next/work/design/D1-derive-debug/REPORT.md).

| Measure | Baseline → spike |
|---|---|
| M1 glue | 3,923 → 3,637 (−286); redaction 448 → 157; error 2,531 → 2,533; construction 213 → 215; paging 228 → 229. |
| M2 model / behavior | Model 3,112 → 3,171; behavior 6,119 → 6,087; model + behavior 9,231 → 9,258. |
| M3 concepts | Adds the public Debug derive; reuses `#[sensitive]`; optional D4 adds `#[public]`; no DSL/core trait/type. Glue lines per concept: not measured. |
| M4 declaration | DSL lines/endpoint 5.04 → 5.04; tokens/endpoint not measured. |
| M5 negations | 0 → 0. |
| M6 controls | Nager 8 → 8; PokeAPI 7 → 7; Frankfurter 11 → 11 DSL lines; no added control concepts. |
| M7 caller docs | Generated endpoint docs unchanged; item docs +121 words and crate docs +32 words. |
| M8 performance | Compile time, expanded lines and allocations: not measured. |
| M9 caller changes | 0 app call sites; 2 existing expected values change (Salesforce locator and Twilio Message Debug). |
| M10 tests | Baseline not measured → final derive tests 8/8 with and without defaults; macro tests 37/37; migrated SDK suites/canaries pass; clippy/fmt clean. New UI expectations; existing locator/Message expectations and missing-Debug snapshot change. Full workspace gate not reported. |
| M11 diagnostics | Baseline not measured → seeded std-derive marker, duplicate impl, marker order, misplaced public marker and missing-Debug cases point into authored code; std's misleading sanitize hint and macro note remain. |

Counterexample handling: Twilio Message uses D1's fail-open field list; D4 can retain its old public diagnostics and withhold future fields, but no committed SDK uses D4. Opaque records use type-level sensitivity; computed summaries remain handwritten. PokeAPI stays byte-identical.
Breaking changes: Debug labels, shape, order, variant prefixes and optional-field presence change across migrated types; wire Display is unchanged. The report's per-type inventory distinguishes these only incompletely: compact parity does not imply pretty parity; marker-only totals also include order changes.

Review:
- Round 1: [REJECT](/home/canarit/projects/Concord_Next/work/reviews/spike-D1-derive-debug-review-1.md).
- Finding, blocking: valid final unsized fields failed the advertised std-Debug compatibility.
- Finding, non-blocking: affected types and newly exposed diagnostic facts were not inventoried individually.
- Fixes: extra reference on the final shown field, unsized compact/pretty parity fixtures, updated diagnostic snapshot and per-type compatibility inventory.
- Round 2: [ACCEPT_WITH_FIXES](/home/canarit/projects/Concord_Next/work/reviews/spike-D1-derive-debug-review-2.md); no new implementation problem found; C1–C9 yes.
- Recorded limits: six compact-identical types change in pretty mode; inventory totals overstate marker-only parity. Builds/rustdoc were not independently run by reviewers; M8 not measured; external namespace/doc-link collisions not assessed.

Agent's assessment:
- The derive removes substantial repeated redaction code without a runtime surface or wire change.
- The cost is diagnostic-text churn and fail-open handling of future fields in partly private records; computed summaries stay Rust.
- Whether D4's `#[public]` earns its additional default remains an owner choice; pretty-output compatibility needs the recorded qualification.
