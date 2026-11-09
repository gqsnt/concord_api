# I1-wire-display — I values — Derive Display from a value's scalar serde spelling

Historical experiment: owner decision 7 (P4.1) replaces the scalar-display derive described here
with `serde_plain::derive_display_from_serialize!`. The implementation and hidden serde bridge
have been removed; see `docs/MIGRATION.md#sdk-owned-formatting` for the current contract.

Branch / commit: spike/I-wire-display at bf05f387 (base ba10189b)
Targets (3 files): witness `examples/github-sdk/src/params.rs`; counterexample `examples/stripe-sdk/src/lib.rs`; simple control `examples/frankfurter-sdk/src/lib.rs` (byte-identical).
C1: yes — declarations remain unchanged; no generator controls added.
C2: yes — serde spelling supplies scalar text; validation, joins and exceptional formatting remain Rust.
C3: yes — Nager, PokeAPI and Frankfurter remain byte-identical.
C4: yes — explicit serde rename/transparent/untagged rules determine spelling.
C5: no — missing Serialize exposes an internal bridge; unsupported composite shapes fail at prepare(), not compilation.
C6: yes — a small derive; declarations and endpoint docs do not grow.
C7: yes after fixes — an explicit crate path supports renamed dependencies/facades; native exits remain.
C8: yes — deterministic formatting adds no I/O, retry or state.
C9: yes — literal/wire/unsupported-shape/partial-write tests discriminate the covered behavior; existing tests remain unchanged.
What disappears: 243 net nonblank SDK source lines across GitHub, Stripe, GitLab, Kubernetes, Cloudflare Zones and Elasticsearch. Classifier glue stays unchanged: the reduction is model −205 and other −38. Compound joiners, forms and differing spellings stay handwritten.

Measurements: M1–M11, baseline → spike. Saved aggregate covers migrated SDKs and simple controls; per-SDK results: [measurement](/home/canarit/projects/Concord_Next/work/wt/I1-wire-display-measure.md); evidence: [report](/home/canarit/projects/Concord_Next/work/design/I1-wire-display/REPORT.md).

| Measure | Baseline → spike |
|---|---|
| M1 glue | 4,539 → 4,539 (0); no glue category changes. |
| M2 model / behavior | Model 5,087 → 4,882 (−205); behavior 3,594 → 3,594; other 2,679 → 2,641 (−38). |
| M3 concepts | One public WireDisplay derive, with optional `#[wire_display(crate = path)]` after fixes; no DSL/core trait/type; zero classifier glue lines removed per derive. |
| M4 declaration | DSL lines/endpoint 7.00 → 7.00; tokens/endpoint not measured. |
| M5 negations | 0 → 0. |
| M6 controls | Nager 8 → 8; PokeAPI 7 → 7; Frankfurter 11 → 11 DSL lines; no added control concepts. |
| M7 caller docs | Generated api! docs unchanged. |
| M8 performance | Compile time, expanded lines and allocations: not measured. |
| M9 caller changes | 0 existing call-site edits; formatter flags change observably despite this count. |
| M10 tests | Baseline not measured → final core 530 passed, SDKs 345 passed including canaries, renamed consumer 4 + 3 passed; clippy clean. Lane reports feature matrix/fmt/rustdoc clean; no existing expected values changed. Full workspace gate not reported. |
| M11 diagnostics | Baseline not measured → missing Serialize at the authored type with internal `concord::__macro::Serialize` help; duplicate Display at derive; serde typo uses serde diagnostics; composite placement fails at prepare(); disabled features report configured-out; crate-path option errors have authored spans. |

Counterexample handling: Stripe forms retain nesting, omission and unset semantics; IdempotencyKey/SecretKey stay manual. GitHub/GitLab Labels and GitHub RepoRef stay manual; Elasticsearch IndexName/DocumentId retain manual Serialize. S3 cannot adopt without adding serde. Frankfurter gains no wrapper or derive.
Breaking changes: Serialize becomes required for adopters; ≈40 query-only types gain a public Serialize impl. Numeric newtypes and numeric enum branches stop forwarding outer width/fill/sign/zero flags, including GitHub WorkflowRef::Id, GitLab ProjectRef::Id and Cloudflare page sizes; default wire text is preserved. The original renamed-dependency failure is fixed through the optional crate path.

Review:
- Round 1: [ACCEPT_WITH_FIXES](/home/canarit/projects/Concord_Next/work/reviews/spike-I1-wire-display-review-1.md).
- Finding, blocking: generated paths hard-coded `::concord`, breaking renamed dependencies.
- Finding, non-blocking: missing-Serialize help exposed internals and had no committed diagnostic fixture.
- Finding, non-blocking: formatter-change inventory omitted numeric enum branches.
- Fixes: optional crate/facade path and real renamed-dependency compile/run fixture; option-error and missing-Serialize UI fixtures; inventory and width assertions cover numeric branches.
- Round 2: [ACCEPT_WITH_FIXES](/home/canarit/projects/Concord_Next/work/reviews/spike-I1-wire-display-review-2.md); renamed-dependency fix accepted; C5 remains no.
- Recorded limits: the new UI fixture preserves the internal Serialize bridge; unsupported composites still fail at prepare(). Reviewers did not run builds/tests/rustdoc/features; deleted temporary equivalence assertions could not be inspected; M8 not measured.

Agent's assessment:
- The derive is small at use sites and removes 243 source lines while leaving declarations unchanged.
- It removes no classifier glue and adds Serialize/formatter-flag compatibility costs; compound values still need Rust.
- A serde_plain recipe could remove the same lines without a Concord concept; whether Concord should own this derive remains an owner judgment.
