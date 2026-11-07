# Desirability loop — decisions

One row per decision, applied mechanically from the mission's decision rule (A1–A9 accept, kill
conditions reject, park), each with an independent Codex `gpt-6.1-sol` xhigh read-only review.
A decision without that review verdict is not a decision.

| ID | Title | Decision | Deciding evidence | Review | Commits | CI runs |
|---|---|---|---|---|---|---|
| T1 | Charter states the fixed core; exclusions become current scope (docs truth, A2) | ACCEPTED_PENDING_CI | owner charter A2; statements checked against source by the reviewer; one wording fix applied | t1-review-sol ACCEPT_WITH_FIXES (fix applied) | 8c97e367, 32fc4312 | pending on 32fc4312 |
