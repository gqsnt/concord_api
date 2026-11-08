# Desirability loop — decisions

One row per decision, applied mechanically from the mission's decision rule (A1–A9 accept, kill
conditions reject, park), each with an independent Codex `gpt-6.1-sol` xhigh read-only review.
A decision without that review verdict is not a decision.

| ID | Title | Decision | Deciding evidence | Review | Commits | CI runs |
|---|---|---|---|---|---|---|
| T1 | Charter states the fixed core; exclusions become current scope (docs truth, A2) | ACCEPTED | owner charter A2; statements checked against source by the reviewer; one wording fix applied | t1-review-sol ACCEPT_WITH_FIXES (fix applied) | 8c97e367, 32fc4312 | green on de05df8d (contains both; run 42425874766: ubuntu, windows, nightly-clone); local gate de05df8d 66/66 |
| H04 | Generated documentation states the actual contract first (docs-only pilot, design hypothesis) | **ACCEPTED (WIN)** | pilot-3 (`measurements/H04.md`): witness net +6 (≥6; Claude +3, Codex +3, 0 regressions; all on sentry/t1, base vs appended path), control lost 0; gates: docs-only scope and docs truth (candidate review 1), full-diff materials clearance (clearance review 2), local gate 66/66; pilot-2 non-qualifying (clearance unsubstantiated), result disclosed (+7) | h04-decision-review-2-sol ACCEPT_WITH_FIXES (WIN_PENDING_CI → CI green); instrument reviews 1–9 (accepted at 9) | 4145921f (candidate), d26e0aa7 (migration), merge 070ae0ae | 4145921f green (run 42461847070: ubuntu, windows, nightly-clone); merge head: pending |
