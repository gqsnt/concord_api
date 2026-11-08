# Review h04-instrument-review-1-sol (INSTRUMENT review, H04 docs-pilot bank, on c0839cad)

Reviewer: Codex gpt-6.1-sol, xhigh, read-only (observed from rollout turn_context),
2026-11-08T05:43:07Z–05:49:19Z, thread 01a1b9ae-91d3-7e11-81f0-2b5cf14db75c.
**Verdict: REJECT** (8 blocking). Confirmed: all 24 tasks / 66 critical facts true against SDK
source at B (no false key); bank hash 2c9cae59 reproduced (137 files); rustdoc captures re-render
byte-identically; Kraken signing verified independently (three fixtures recomputed); stored
validation consistent; 23 decision cases reproduced; control-losses-as-regressions conservative;
recurrence ≥2 and gates as inputs reasonable if frozen. Not executed: builds, sandboxes, sessions.

| # | Finding (task content not quoted) | Severity | Triage |
|---|---|---|---|
| 1 | A control task's destination/tenant facts are absent from both arms' materials (undiscoverable at B) | blocking | fix — H04 fix round 1: replace with control facts discoverable from B's public docs (controls are preservation facts) |
| 2 | Constant answers can pass metadata / result-use checks (one disclosed response) | blocking | fix — exercise submitted expressions against several undisclosed varying responses; constant-answer negative controls |
| 3 | Construction inputs untested (OAuth credentials with unexpired tokens; a requested timeout) | blocking | fix — scenarios that exercise submitted credentials/transport settings; wrong-input controls |
| 4 | A valid explicit-default call is rejected (exact query comparison) | blocking | fix — accept both permitted forms; positive validation case |
| 5 | Example filtering depends on fence formatting (retagging a block changes visible evidence) | blocking | fix — format-independent filtering (call-chain detection as in I3 `chain_hits`, in any fence/indent/prose); coverage audit across arms |
| 6 | Package README route omitted (a candidate could gain by inlining existing docs) | blocking | fix — capture each arm's package README consistently, deterministic dedup + the same filtering |
| 7 | Decision rule weakens "no critical control fact newly misrepresented" (needs ≥2 regressions in one fact) | blocking | fix — coordinator decision: a control critical fact that passes in every baseline-arm session of its SDK and fails in ≥1 candidate session blocks WIN (INCONCLUSIVE, or LOSS if attributed to the docs); plus the docs-truth gate input; card amended before any candidate outcome |
| 8 | Codex isolation not enforced (runs inside the checkout, tools enabled, read-only; post-hoc filtering only) | blocking | fix — readers run in bubblewrap with the home and repository hidden (per-run provider home with only the credential file bound, as I4's runner does), cwd an empty directory outside the repository; observed configuration verified from the rollout; tool-call discard kept |

Note (coordinator): the same ambient-context point applies to the I3 runner (empty dir inside the
checkout, read-only, tool calls discard; 0 tool calls observed in 256 baseline sessions). Recorded
as an I3 limit; the I3 runner can adopt the H04 sandbox later.
