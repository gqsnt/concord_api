# Review h04-decision-review-sol (DECISION review, H04, pilot-2)

Reviewer: Codex gpt-6.1-sol, xhigh, read-only (observed from rollout turn_context),
2026-11-08T08:47:55Z–08:54:16Z, thread 01a1ba57-c31c-7ff1-8d29-528eb40262b4.
**Verdict: REJECT; decision INCONCLUSIVE** (1 blocking, 1 non-blocking). Confirmed: all 288
answer rows and 144 pairs reconstructed (witnesses 65→72, 7 corrections, 0 regressions; controls
70→70, 1 correction, 1 regression; provider witness nets +4/+3); scalar checks recomputed against
the bank keys, answers spot-checked in all six SDKs; all 12 prompts rebuilt; bank, materials,
clearance and frozen hashes, schedule balance, provenance, session metadata, observed models/effort,
zero tool use verified; 64/64 APIs and 10/10 expansions identical without docs; local gate 66/66 on
the exact candidate. "The answer-based rule was applied correctly: concentration in sentry/t1 does
not independently require INCONCLUSIVE"; ceilings disclosed; the Xero byte-identical reading is
supported.

| # | Finding | Severity | Triage |
|---|---|---|---|
| 1 | The required pre-session clearance is unsubstantiated: the candidate review inspected only additions, and only for Cloudflare, Sentry and Strava; the complete diffs the card requires were not read; coordinator-issued digests matching do not establish that review | blocking | accepted — confirmed from the candidate reviewer's rollout (added-line printouts for three SDKs, partial `cat` of two others; no full-diff read for each SDK). **pilot-2 does not qualify for the H04 decision** (results disclosed in `measurements/H04.md` as non-qualifying). Remedy chosen (the reviewer's second option): a qualifying clearance review that reads every changed SDK's complete diff, verified from its rollout, then a **fresh run (pilot-3, seed 5)** whose result decides H04 whatever it is; pre-registered in the card before pilot-3 starts. |
| 2 | Decision record unfinished (CI and verdict placeholders) | non-blocking | fixed in the record |

Coordinator note: this is a procedural defect in the clearance step, not a measurement defect;
the frozen thresholds are unchanged.
