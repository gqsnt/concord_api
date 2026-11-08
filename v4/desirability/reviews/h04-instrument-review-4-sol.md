# Review h04-instrument-review-4-sol (INSTRUMENT re-review, H04 bank, on 81618089)

Reviewer: Codex gpt-6.1-sol, xhigh, read-only (observed from rollout turn_context),
2026-11-08T07:20:00Z–07:24:28Z, thread 01a1ba07-4507-7481-ade9-b7466e3eb75e.
**Verdict: REJECT** (1 blocking). Confirmed: hash 4b93f872; six B materials and 12 B/B prompts
reproduce exactly; 240 synthetic cases and rule-off controls pass; decision self-tests 30/30; the
official scorer counts control regressions only; the replaced Strava fact and all 32 control facts
have rendered public evidence; the Sentry witness fact (base/path, stated only generically at B)
is an acceptable witness opportunity; recorded validation counts. Not executed: compilation,
fixtures, sessions, isolation.

| # | Finding | Severity | Triage |
|---|---|---|---|
| 1 | Expressions still evade removal and audit (whitespace-separated members; a host-path exception admits `a.b/2`-style expressions); a full-render reproduction keeps one with `format_invariant=True` | blocking | fix — round 4: whitespace-insensitive recognition, narrowed host-path exception, detector and synthetic controls extended; **plus a structural backstop**: any broad-detector token present in the candidate arm's filtered materials and absent from the baseline arm's is listed in the pilot manifest and must be cleared (not answer-bearing) by the candidate's independent review before any reader session runs. Coordinator stop rule: after round 4, further adversarial-formatting cases are covered by this gate (the candidate implementer never sees the tasks) rather than by new filter rounds, unless a review shows answer-bearing content reaching readers in an actual arm. |
