# E1-operation-effect — E operation facts — Carry explicit read effects on operation identities
Branch / commit: spike/E-operation-effect at 3ea19eb9 (base ba10189b)
Targets (3 files): witness `examples/strava-sdk/src/auth.rs`; counterexample `examples/kraken-spot-rest-sdk/src/lib.rs`; simple control `examples/pokeapi-sdk/src/lib.rs` (byte-identical).
C1: no — `#[read]` is a provider fact, but retained `operations Name;` still selects Concord's policy-dispatch trait.
C2: yes — effect columns disappear; signing, retries, uncertainty, quotas and status classification remain Rust.
C3: yes after fixes — operation enums/name checks are opt-in again; controls gain no public enum.
C4: yes — GET/HEAD/OPTIONS default to read; endpoint/scope annotations inherit; PUT implies no replay permission.
C5: yes — errors point to authored attributes/names without internals; client-level read message remains generic.
C6: yes — the read rule is short; N3's trait-switch choice remains unresolved.
C7: yes — caller transport, prepare(), parts and raw send survive.
C8: yes — static metadata adds no actor, signing change or dispatch.
C9: no — Datadog's restored exhaustive status classification lacks a targeted regression witness.
What disappears: final classifier operation glue 33 → 32 (−1); behavior −29; net nonblank SDK source −30. Source reductions: Strava 10, Sentry 8, Zendesk 8, PayPal 5, with Kraken adding 1. Earlier −5 glue/−64 source claims are superseded.

Measurements: M1–M11, baseline → spike. Saved cohort includes migrated SDKs, unchanged comparison SDKs and controls; per-SDK changes: [measurement](/home/canarit/projects/Concord_Next/work/wt/E1-operation-effect-measure.md); evidence: [report](/home/canarit/projects/Concord_Next/work/design/E1-operation-effect/REPORT.md).

| Measure | Baseline → spike |
|---|---|
| M1 glue | 3,824 → 3,823 (−1), entirely operation 33 → 32. |
| M2 model / behavior | Model 3,410 → 3,410; behavior 3,214 → 3,185 (−29); other 1,682 → 1,681. |
| M3 concepts | `#[read]` + Operation::is_read(); 0.5 classifier glue lines removed per concept. Earlier ≈32 source lines/concept was the wrong numerator and is superseded. |
| M4 declaration | DSL lines/endpoint 6.83 → 6.84; declaration lines 437 → 438; tokens/endpoint not measured. |
| M5 negations | 0 → 0. |
| M6 controls | Nager 8 → 8; PokeAPI 7 → 7; Frankfurter 11 → 11 DSL lines; only private read metadata differs in expansion; no new control concepts/public enums. |
| M7 caller docs | Per-endpoint docs unchanged; final per-client enum-doc comparison not measured after restoring opt-in generation. |
| M8 performance | Compile time, expanded lines and allocations: not measured. |
| M9 caller changes | 0 existing call-site edits; initial SDK public-surface regressions were reversed. |
| M10 tests | Baseline not measured → final 546 library/core/macro and 408 SDK passes across 72 suites; canaries pass; H08 E0004 fixture remains rejecting. Library expected metadata/attribute/enum assertions change; no SDK expectations change. Full workspace gate not reported. |
| M11 diagnostics | Baseline not measured → authored attribute/name spans without internal paths; client-level read message generic. Missing read annotation is not a compile error; final Datadog coverage does not catch its removal. |

Counterexample handling: Kraken marks a scope covering 3 signed POST reads while signing stays exhaustive Rust. Strava/Sentry PUTs retain replay restrictions. Datadog keeps exhaustive posting() classification for HTTP 202; Square keeps its statement/forwarding impl; GitHub keeps its enum. Cfg'd endpoints/scopes and both operation identity forms are covered.
Breaking changes: Operation::is_read() is additive; hidden EndpointMeta gains read and declared construction gains a third argument; read attributes become accepted. Initial automatic-enum name collisions and Datadog/Square public impl removals are reversed; no remaining SDK public-surface break found. Expected library metadata fixtures gain read values.

Review:
- Round 1: [ACCEPT_WITH_FIXES](/home/canarit/projects/Concord_Next/work/reviews/spike-E1-operation-effect-review-1.md).
- Finding, blocking: M3 divided total source reduction rather than classifier glue by concepts.
- Finding, non-blocking: public policy-impl removals and automatic-enum namespace breaks were unlisted.
- Finding, non-blocking: Datadog's non-read default silently replaced exhaustive HTTP-202 classification.
- Finding, non-blocking: unconditional enums enlarged controls while operations still selected a trait.
- Fixes: restore opt-in enums, Datadog's exhaustive posting()/policy impl and Square's forwarding; recompute final glue/source/M4/M3 values and compatibility inventory.
- Round 2: [ACCEPT_WITH_FIXES](/home/canarit/projects/Concord_Next/work/reviews/spike-E1-operation-effect-review-2.md); C3 fixed; C1/C9 remain no.
- Recorded limits: no direct Datadog descriptor/added-write regression witness; removing its read annotation now passes tests. N3's blanket adapter demonstrably overlaps downstream impls, but this does not prove all stable alternatives impossible. Reviewers inspected logs without rerunning tests; M8 not measured.

Agent's assessment:
- E1 carries a useful read fact on both identity forms with modest classifier savings.
- N3's blanket adapter fails coherence; the final tree retains the operations trait switch and opt-in enum generation.
- Keeping that statement versus a runtime from_operation lookup remains an owner choice; Datadog needs targeted regression coverage before stronger completeness claims.
