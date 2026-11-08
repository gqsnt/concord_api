# Review h04-instrument-review-3-sol (INSTRUMENT re-review, H04 bank, on e50a22ae)

Reviewer: Codex gpt-6.1-sol, xhigh, read-only (observed from rollout turn_context),
2026-11-08T06:45:47Z–06:50:58Z, thread 01a1b9e7-f128-7ad3-af11-690ffa257ae1.
**Verdict: REJECT** (3 blocking). Confirmed: bank f21f432e recomputed; all six materials
re-rendered identically in memory; no surviving SDK worked examples in B's materials; decision
self-tests 30/30 incl. both control-fact rules; timeout scoring removed; distinct-origin
assertions and swapped/reused-origin failures; opaque paths and transcript scans; recorded
validation counts. Not executed: cargo, sessions, bwrap.

| # | Finding (task content not quoted) | Severity | Triage |
|---|---|---|---|
| 1 | Field-only expressions written as plain prose evade removal; conversions start only from fenced blocks; evidence detection shares the filter's blind spot | blocking | fix — round 3: detect field/call expressions in prose too; conversions from prose and inline as well as blocks; independent positive checks |
| 2 | A Strava control fact needs a construction-time rule B's public docs do not state | blocking | fix — replace with a B-documented control fact; reseal |
| 3 | `--control-loss-rule net` lets the official scorer use a rule the card forbids | blocking | fix — remove the option for official verdicts (regression counting only) |
