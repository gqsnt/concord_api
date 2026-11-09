# H1-request-line — H documentation — Render prepared request facts compactly and explain shared rules once
Branch / commit: spike/H-request-line at ea8103e3 (base ba10189b)
Targets (3 files): witness `examples/fred-sdk/src/lib.rs`; counterexample `examples/clickhouse-sdk/src/wire.rs`; simple control `examples/nager-holidays-sdk/src/lib.rs` (documentation edits only).
C1: yes — executable declarations are unchanged; no provider syntax added.
C2: yes — policies, codecs, wrappers and behavior stay Rust; no replacement SDK trait.
C3: yes — FRED/Nager changes are documentation only; control declarations/call shapes remain.
C4: yes after fixes — literal notation, ownership, presence and provenance ambiguities are corrected.
C5: yes by inspection — validation, signatures, bounds and diagnostic spans unchanged; M11 not measured.
C6: yes for samples — generated endpoint words fit the budget; universal enforcement is absent.
C7: yes — native exits survive and shared Requests text describes them; the page is named, not linked.
C8: yes — text rendering introduces no runtime actor or evaluation.
C9: no — sticky sensitivity still leaks in mixed header chains; budget/general composition checks remain absent.
What disappears: classifier glue remains 1,418; declaration-mechanics doc lines 81 → 76. FRED removes 9 doc-only lines; Nager deletes 10 and adds 3; ClickHouse unchanged. No runtime module disappears. Repeated generated prose moves to a shared Requests section.

Measurements: M1–M11, baseline → spike. Classifier cohort: FRED, Nager, ClickHouse and simple controls; M7 separately samples public endpoints across SDKs. Sources: [measurement](/home/canarit/projects/Concord_Next/work/wt/H1-request-line-measure.md), [report](/home/canarit/projects/Concord_Next/work/design/H1-request-line/REPORT.md).

| Measure | Baseline → spike |
|---|---|
| M1 glue | 1,418 → 1,418 (0); decl_doc_mech 81 → 76, outside classifier glue. |
| M2 model / behavior | Model 553 → 553; behavior 1,138 → 1,138; aggregate hand source delta not measured (doc edits listed above). |
| M3 concepts | No DSL keyword/public type; shared Requests section and compact notation. Proposed CI word gate is not implemented; glue lines per concept not measured. |
| M4 declaration | DSL lines/endpoint 6.75 → 6.75; tokens/endpoint not measured. |
| M5 negations | 0 → 0. |
| M6 controls | Nager 8 → 8; PokeAPI 7 → 7; Frankfurter 11 → 11 DSL lines; no added runtime/declaration concepts. |
| M7 caller docs | 91 public endpoints in 9 SDKs; navigation + request type + send(), excluding author prose: mean 785.5 → 84.5, median 810 → 83, p90 972 → 107, max 1,073 → 111; all 91 above 150 → none. Final figures supersede mean 81.2/max 106; actual rustdoc statistics remain lane-reported. |
| M8 performance | Compile time, expanded lines and allocations: not measured. |
| M9 caller changes | 0 call-site edits; no public API/signature/runtime Debug/Display changes found. |
| M10 tests | Baseline not measured → final 515 macro/concord passes (macros 39/39), FRED/Nager/ClickHouse 133; clippy/rustdoc clean. Macro prose expectations and shared-rule assertions change; SDK suites do not assert generated prose. Full workspace gate not reported. |
| M11 diagnostics | not measured; sensitive-source canaries do not measure compile-error quality. |

Counterexample handling: ClickHouse's generated client is private, so its public wrappers/header notes remain SDK-local and no public generated-doc gain is claimed. Nager keeps its endpoint and KeepHead behavior. Runtime bases render as {base}; shared rules are named once rather than linked. Native-header literals are conservatively withheld, including harmless ones whose resolved name is unknown.
Breaking changes: generated documentation text/notation changes to Prepared request:, symbolic bases, quoted names/keys, escaped literal braces, owner-qualified holes, sensitivity/presence markers and body-limit wording. No caller/API/runtime behavior change; secrecy remains incomplete for sticky mixed header chains.

Review:
- Round 1: [REJECT](/home/canarit/projects/Concord_Next/work/reviews/spike-H1-request-line-review-1.md).
- Finding, blocking: native header aliases and sensitive initializers could print secrets in docs.
- Finding, blocking: literal braces and separator-bearing keys masqueraded as request structure.
- Finding, blocking: owner qualification and optional/repeated sensitivity facts were lost.
- Finding, blocking: native Content-Type provenance was stated incorrectly.
- Finding, blocking: “at most N bytes read” overstated the chunked read bound.
- Finding, non-blocking: selected-base/prepared-request wording and unlinked-page reachability differed from the design.
- Finding, non-blocking: no CI word gate/general prepared-output oracle; source canaries did not measure M11.
- Fixes: conservative native-literal/initializer withholding; escaped/quoted literals with paired prepare assertions; resolved owners/presence; symbolic native Content-Type precedence; truthful body-limit text; {base} and Prepared request: labels; revised word counts.
- Round 2: [REJECT](/home/canarit/projects/Concord_Next/work/reviews/spike-H1-request-line-review-2.md); blocking sticky-sensitivity leak remains at `crates/concord-macros/src/emit.rs`; C9 no.
- Recorded limits: a sensitive literal header overridden in a mixed native/literal chain can print its secret because rendering loses the slot's sensitivity history. Per-endpoint CI budget/general rendered-line-versus-prepare oracle absent; M11 and reader accuracy not measured; rules page unlinked. Historical sample/word figures are superseded; reviewers did not run tests or reproduce revised rustdoc statistics.

Agent's assessment:
- Compact facts and shared rules greatly reduce sampled generated prose; final maximum is 111 words against the 150-word budget.
- This is partly working: the remaining documentation secrecy leak blocks acceptance despite the measured reduction.
- The rules page is named, not one click away; budget/composition enforcement and diagnostic/reader measurements remain open.
