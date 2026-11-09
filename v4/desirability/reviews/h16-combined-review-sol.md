# Review h16-combined-review-sol (COMBINED review, H16: candidate + measurement + decision; A3 §5)

Reviewer: Codex gpt-6.1-sol, xhigh, read-only (observed from rollout turn_context),
2026-11-09T02:06:45Z–02:13:27Z, thread 01a1be0e-d381-7dd3-8634-691b3cccec98.
(Candidate authored by a Codex lane: independent session, same provider.)
**Verdict: ACCEPT_WITH_FIXES. DECISION: INCONCLUSIVE** — the instrument did not expose A's generated-doc
mechanism, so pooled +3 cannot decide its effectiveness. **Deciding item:** failed intervention exposure
makes the intended comparison unscorable (KEEP_CURRENT would apply to +3 only from a valid comparison;
variability alone does not override the frozen threshold).
Confirmed: candidate docs true against emitter/core/SDK sources (four samples; Kubernetes matches the real
request); docs-only (33 expansions, 116 rustdoc targets); testserver edits remove link markup only; gate 66;
all 324 wire answers rescored; wire truths unchanged; 120 prompt hashes; alternating arm order; no tool calls;
totals Codex 53→53, Claude 40→43; controls match.

| # | Finding | Severity | Triage |
|---|---|---|---|
| 1 | Record INCONCLUSIVE as an instrument-validity failure (generated rustdoc absent from every wire prompt; 8 SDKs identical); neither accepts A nor closes the family | blocking | applied in measurements/H16.md and DECISIONS |
| 2 | "Attributable effect" overstates causality: FRED/OpenAI observed +8 (Codex), +11 (Claude), pooled +19; unchanged-prompt witnesses −16 | non-blocking | applied |
| 3 | Metadata/variability wording: complete metadata of ClickHouse/B2 differs via rustdoc hashes; Kubernetes swing is observed (pooled −15), not a calibrated band | non-blocking | applied |
| 4 | Specify a valid follow-up (materials with each arm's generated docs; exposure audit; frozen scoring; fresh paired interleaved reps) | non-blocking | applied (record §"What a valid re-pilot needs") |
| 5 | The card's secondary 55-SDK coverage diagnostic is missing | non-blocking | marked "not measured" |
