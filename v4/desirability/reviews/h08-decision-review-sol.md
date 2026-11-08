# Review h08-decision-review-sol (DECISION review, H08 pilot)

Reviewer: Codex gpt-6.1-sol, xhigh, read-only (observed from rollout turn_context),
2026-11-08T11:49:20Z–11:55:09Z, thread 01a1bafd-d874-7563-9b44-49b263faa384.
**Verdict: ACCEPT_WITH_FIXES; decision WIN_PENDING_CI.** Bank hashes recomputed; coverage
reconstructed (73 records, 53/53 groups, commits and patch/oracle hashes match); raw diagnostics,
freshness, naming and oracle inventories checked; all omission pairs reproduce, 14 references plus
6 final-commit references pass; **B catches all six omissions at SDK build with endpoint-naming
E0004**; Strava and Sentry satisfy the threshold independently of Kraken (ii); A fails naming (55
public items, 33 changed signatures), so the card selects B. All 18 agent results, oracle logs,
classification sites, observed model/effort, isolation probes, unique sessions and alternating
pairing verified: 3/3 per witness in both arms (ceiling, no demonstrated gain). Public JSON of 52
non-adopters unchanged apart from docs (independently compared); concept budget, A7 (three pilot
replacements), compile-time enforcement, guardrails, Kraken test changes, allocations, costs < 5%,
both local gates 66/66 confirmed. Migration plan consistent with the card as a provisional plan
(no completed-work credit). Outstanding before ACCEPT: CI, full migration, impact table, migration
docs, post-migration gates.

| # | Finding | Severity | Triage |
|---|---|---|---|
| 1 | Unreported existing-suite failure (baseline Sentry r3 doctest E0463; oracle passed) | non-blocking | fixed in the record (exception reported; "no regression" scoped to oracle and classification) |
| 2 | Missing adoption-cost diagnostic (per-SDK lines) | non-blocking | fixed in the record |
