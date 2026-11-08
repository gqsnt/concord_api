# Review l0-baseline-review-sol (BASELINE review, on 7ac81a4d)

Reviewer: Codex gpt-6.1-sol, xhigh, read-only (observed from rollout turn_context),
2026-11-08T06:01:17Z–06:10:19Z, thread 01a1b9bf-322f-7ad1-a2d5-90b74a0972df.
**Verdict: REJECT** (2 blocking, 4 non-blocking). Confirmed: I1 totals and per-SDK rows, rustdoc
hashes, D1c's 13 inaccessible generated clients, S2's classifier difference; S3 `quota_key` is
request context, not a credential (S3 = 0 after triage); I5 scores; I3 aggregates, ceiling
evidence, prompt hashes, transcripts; requested/observed metadata across 256 reader and 40 editor
sessions; I4 results, validation counts, timings; gate 66 passes, 443 s, 2,727 example tests, 55
canary targets; 38 changed files contain no sealed quotes; `eb46bf2b` matches B's runner, changes
only `omission_stage`, preserves `oracle_verdict`, its control discriminates (12 framing controls,
7 omission logs, 1,112-log corpus reproduced). Not checked: CI run (GitHub unreachable from the
sandbox); no fresh executions.

| # | Finding | Severity | Triage |
|---|---|---|---|
| 1 | Noise bands rounded down (Codex spreads 4.545 / 4.545 / 3.409 pts) | blocking | fixed: bands from item counts, exact values |
| 2 | I4 propagation reading exceeds the evidence (7 test-expectation flags + 1 docs flag; not production paths, not D2a) | blocking | fixed: BASELINE I4 bullet rewritten; QUEUE H12 note corrected |
| 3 | Reader isolation overstated | non-blocking | fixed: observed absence of tool use + isolation limit stated |
| 4 | R1 explanation incomplete (Accept/header/body misses; Codex run 2 Nager) | non-blocking | fixed: critical vs other exact misses, field rates; QUEUE H04/H16 notes |
| 5 | 74→73 mechanics-line difference unexplained | non-blocking | fixed: seed counted a commented NASA NeoWs example |
| 6 | I4 model ledger row unfinished | non-blocking | fixed: observed row appended |
