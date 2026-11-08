# Review h08-instrument-review-1-sol (INSTRUMENT review, H08 omission harness + I4 bank copy for B, on 0eaae35c)

Reviewer: Codex gpt-6.1-sol, xhigh, read-only (observed from rollout turn_context),
2026-11-08T11:10:17Z–11:15:54Z, thread 01a1bada-19c6-75b3-b957-d2d665cd972d.
**Verdict: ACCEPT_WITH_FIXES** (0 blocking, 1 non-blocking). Confirmed: the card, harness, sealed
specifications, authoring material, patches, oracles and validation records read; all three bank
hashes recomputed; patches reconstructed in memory — omissions remove only the required
classification, references and oracles fair across arms; provider sources and independent oracle
expectations reviewed; Kraken's resource-based sibling is weaker, but the other two witnesses meet
the threshold independently; all 73 records reparsed against raw diagnostics, oracle inventories
and freshness evidence. **B meets the primary detection threshold in all three witnesses, twice per
omission, at both B commits; none moves later than baseline. A fails endpoint naming.** Strava
baseline's naming match is lexical and incidental. I4 bank copy: task texts/public fields and every
oracle unchanged, scoring adaptations recorded, reference/control validation, original bank line
unchanged, unrecorded hashes rejected, other lines preserved on hash updates. Not checked: fresh
executions; the secondary agent outcome, costs, gates, CI (outside this review).

| # | Finding | Severity | Triage |
|---|---|---|---|
| 1 | Missing runs silently validate (validation checks only groups present) | non-blocking | fix — coverage check against an explicit measurement scope, with a self-test (measurement lane); the current 73-record set is complete, so the result stands |
