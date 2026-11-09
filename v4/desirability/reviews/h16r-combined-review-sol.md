# h16r-combined-review-sol (Codex gpt-6.1-sol xhigh, read-only; observed gpt-6.1-sol xhigh; 2026-11-09T03:01Z–03:08Z; independent session, same provider as the Codex-authored candidate lane)

VERDICT: ACCEPT_WITH_FIXES

DECISION: INCONCLUSIVE — A1 §3 supports an instrument-validity outcome here; failed WIN does not imply KEEP_CURRENT, and superseded DECISION_RULE A4 does not imply LOSS.

DECIDING ITEM: Codex `openai/w1`: OpenAI falls 13→10, exceeding the frozen −2 guard; candidate answers give `accept: "*/*"`, true at transport execution, while the key requires `null`.

FINDINGS:

- [non-blocking] Clarify the decision basis — [measurements/H16r.md:62](/home/canarit/projects/Concord_Desirability_Loop/work/wt/cd/v4/desirability/measurements/H16r.md:62) — Ambiguity is called “not a decision input,” then used to justify INCONCLUSIVE. A1 provides broader inconclusive categories, not an explicit catch-all for every unmatched result. State that the verdict-relevant question/oracle ambiguity justifies INCONCLUSIVE under A1/A3; retain the frozen scores and failed guard.

- [non-blocking] Document a valid follow-up — [measurements/H16r.md:77](/home/canarit/projects/Concord_Desirability_Loop/work/wt/cd/v4/desirability/measurements/H16r.md:77) — The record ends without a prospective remedy. Specify an instrument repair that explicitly defines the boundary for `headers`, `accept`, and `content_type`; validate known-good/bad cases, then freeze a new card with exhaustive outcome mapping and fresh paired, alternating runs for both providers and arms. Preserve H16r’s scores and INCONCLUSIVE outcome; no acceptance, family remains open, and it counts as “not accepted.”

- [non-blocking] Complete variability reporting — [measurements/H16r.md:41](/home/canarit/projects/Concord_Desirability_Loop/work/wt/cd/v4/desirability/measurements/H16r.md:41) — The table promises per-repetition spread but mostly gives pooled totals. Add five counts or ranges for every SDK/provider/arm, as the card requires. State that human preference and adoption remain unmeasured.

- [non-blocking] Correct run chronology — [measurements/H16r.md:3](/home/canarit/projects/Concord_Desirability_Loop/work/wt/cd/v4/desirability/measurements/H16r.md:3) — “First reader run 03:41” conflicts with raw attempt timestamps: first start was **03:44:25 +01:00**. Correct it; the stored widened exposure check predates that start.

CHECKED: Candidate remains `02bc13b6`; all 11 changed-source hashes match H16’s evidence, product worktrees have no additional changes, and the diff remains documentation rendering/comments and doc assertions. Reviewed H16’s docs-only evidence and exact-commit gate log. Both disputed notes are true against Binary/Decode and pinned reqwest source: default Accept is `*/*`, inserted only when missing at execution. Verified 108 appendix blocks against arm-specific rustdoc JSON, archive/provenance hashes, question coverage, and all 20 sealed prompt hashes. All appendices differ; declaration portions differ only for FRED/OpenAI. No complete scored request found in leakage checks; marker widening accommodates queryless endpoints without changing card criteria. Verified 200 fresh sessions, no H16 session reuse, alternating order, requested settings, observed models, and no tool calls. Independently summed score files and rescored all 540 wire item-runs without discrepancy: Codex **94→100**, Claude **95→108**, controls **20→20 per provider**, Codex OpenAI **13→10**. The packed question limits `headers` explicitly but leaves the separate `accept` field ambiguous; `score.py:165` compares it directly with filtered truth.

NOT CHECKED: No builds, runners, agents, or new gate/CI executions were performed. Existing docs-only/gate evidence was inspected. Claude’s actual effort is not exposed; medium was requested. The original narrow marker-check version was not retained, so its exact historical contents could not be independently verified.