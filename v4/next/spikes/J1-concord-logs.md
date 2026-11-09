# J1-concord-logs — J observability — Emit a static call event and provide redacted summaries
Branch / commit: spike/J-concord-logs at f51bd08d (base ba10189b)
Targets (3 files): witness `examples/github-app/src/bin/log_one.rs`; counterexample `examples/amazon-s3-sdk/src/client.rs`; simple control `examples/nager-holidays-sdk/src/lib.rs` (byte-identical; opt-in demonstrated in a separate caller).
C1: yes — declarations gain no logging syntax or generator controls.
C2: yes — metadata is compiler-derived; subscriber setup, signing and workflow reports remain Rust.
C3: yes — GitHub/Nager SDK declarations and manifests remain unchanged; caller opt-in is separate.
C4: yes after fixes — route composition preserves meaningful base-only trailing slashes.
C5: yes — existing declaration checks remain; accessors use public Rust types.
C6: yes — a fixed event and summary view; endpoint docs unchanged.
C7: yes — caller transport/native exits remain; S3 still signs and executes through its decoder.
C8: yes — one call, no retries/middleware/global subscriber; logging feature off by default.
C9: no — feature-off route representation/literal absence lacks a committed regression guard.
What disappears: no classifier glue; S3 SignedRequest Debug shrinks 17 → 5 lines (−12, classified behavior), with model +1 in the measured cohort. GitHub SDK needs no edit. J3 supplies this formatting reduction; J4 value-bearing spans were not built.

Measurements: M1–M11, baseline → spike. Saved cohort includes S3, GitHub, Nager, ClickHouse, Discord and simple controls; per-SDK changes: [measurement](/home/canarit/projects/Concord_Next/work/wt/J1-concord-logs-measure.md); evidence: [report](/home/canarit/projects/Concord_Next/work/design/J1-concord-logs/REPORT.md).

| Measure | Baseline → spike |
|---|---|
| M1 glue | 4,057 → 4,057 (0); no glue category changes. |
| M2 model / behavior | Model 2,168 → 2,169; behavior 3,181 → 3,169 (−12); full hand-counted net source delta not measured. |
| M3 concepts | Adds tracing feature, Summary type, four summary() methods and fixed event schema; no DSL/SDK trait; glue lines per concept not measured. |
| M4 declaration | DSL lines/endpoint 8.69 → 8.69; tokens/endpoint not measured. |
| M5 negations | 0 → 0. |
| M6 controls | Nager 8 → 8; PokeAPI 7 → 7; Frankfurter 11 → 11 DSL lines; no added control declaration concepts. |
| M7 caller docs | Per-endpoint docs unchanged; crate docs +≈70 words. |
| M8 performance | Compile time, expanded lines and allocations: not measured. Feature-off route literal measured absent; final interest gate can read the clock for INFO subscribers that reject this event. |
| M9 caller changes | 7 test/example sites + 1 bench; includes stronger Debug expectations and hidden metadata initializers. |
| M10 tests | Baseline not measured → core 29 off/on (+4 ignored), macros 37, concord 474 off/on; SDKs 151 off; final app/callers/ClickHouse 103 on; canaries, clippy/fmt/rustdoc/graph checks pass per lane. Query-Debug expectations change; round-1 fixes change none. Full workspace gate not reported. |
| M11 diagnostics | Baseline not measured → seeded cases point into user code; rustc's moved-value note reaches core response code and one error names tracing::Value. Forgetting the feature silently produces no events. |

Counterexample handling: S3 decoder summary withholds the native signed target after edits; signing stays Rust. Events use static route templates and exclude runtime header/path/query values; cancellations emit nothing. Prepared Summary/Debug still show ordinary unmarked path/keyed-query values; query(..) is wholly withheld. The guarantee covers target concord: permissive dependency tracing can expose sensitive host labels, so caller filtering matters.
Breaking changes: J3 changes Prepared Debug for query(..) even with tracing off and changes S3 SignedRequest Debug; ClickHouse, core-concepts and Discord expectations now require ?<withheld> while retaining wire/secrecy assertions. Hidden metadata initializers change; Summary/accessors/feature are additive. Caller manifests/lockfile gain tracing-subscriber dependencies and separate caller crates.

Review:
- Round 1: [ACCEPT_WITH_FIXES](/home/canarit/projects/Concord_Next/work/reviews/spike-J1-concord-logs-review-1.md).
- Finding, blocking: mismatched enabled-probe metadata suppressed events for interested subscribers.
- Finding, blocking: base-only route rendering lost the sent path's trailing slash.
- Finding, non-blocking: “off = nothing compiled in” ignored unconditional metadata and J3 additions.
- Fixes: level-enabled clock gate plus actual event callsite filtering/lazy elapsed field; selective-subscriber tests; wire-compared route composition tests; unit-valued Route and absent literal with tracing off.
- Round 2: [ACCEPT_WITH_FIXES](/home/canarit/projects/Concord_Next/work/reviews/spike-J1-concord-logs-review-2.md); blockers fixed; C9 remains no.
- Recorded limits: off-footprint lacks a regression guard. Instrumentation/dependency/clock/route literal are gated; query_pairs/redaction/Summary/accessors remain unconditional. Some INFO subscribers incur a clock read without an event; mid-call subscriber installation yields no event. Reviewers did not rerun tests/footprint checks; M8 not measured; unrelated query(..) SDK assertions still need the full gate.

Agent's assessment:
- Concord can log its static call facts through an opt-in feature without SDK declaration words, with strong covered-placement canaries.
- Events are intentionally sparse; safe automatic events do not imply blanket secrecy for application/dependency logs or ordinary unmarked summary values.
- S3's saving belongs to J3, which changes query Debug even when tracing is off; the corrected footprint claim and missing guard remain explicit limits.
