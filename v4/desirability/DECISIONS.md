# Desirability loop — decisions

One row per decision, applied mechanically from the mission's decision rule (A1–A9 accept, kill
conditions reject, park), each with an independent Codex `gpt-6.1-sol` xhigh read-only review.
A decision without that review verdict is not a decision.

| ID | Title | Decision | Deciding evidence | Review | Commits | CI runs |
|---|---|---|---|---|---|---|
| T1 | Charter states the fixed core; exclusions become current scope (docs truth, A2) | ACCEPTED | owner charter A2; statements checked against source by the reviewer; one wording fix applied | t1-review-sol ACCEPT_WITH_FIXES (fix applied) | 8c97e367, 32fc4312 | green on de05df8d (contains both; run 42425874766: ubuntu, windows, nightly-clone); local gate de05df8d 66/66 |
