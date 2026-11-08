# Review h04-instrument-review-2-sol (INSTRUMENT re-review, H04 bank, on c8c82534)

Reviewer: Codex gpt-6.1-sol, xhigh, read-only (observed from rollout turn_context),
2026-11-08T06:23:49Z–06:29:57Z, thread 01a1b9d3-d263-7922-b58a-c931040fc678.
**Verdict: REJECT** (6 blocking). Confirmed addressed: review-1 #1, #2, #4, #6, #8 (replacement
control facts found in B's materials; 17 constant and 7 input controls fail for recorded reasons;
explicit-default positives; README capture; byte-identical re-renders; B/B prompt identity; both
providers' isolation records). Bank hash e68bee14 recomputed. Not executed: builds, runners,
providers, bwrap.

| # | Finding (task content not quoted) | Severity | Triage |
|---|---|---|---|
| 1 | Worked examples left after chain removal still supply scored answers (construction / result-use) | blocking | fix — round 2: remove worked examples in every format (code blocks of any fence or indentation, inline spans holding an expression) in both arms; keep contract prose |
| 2 | Inline vs block formatting changes evidence unnoticed by the audits | blocking | fix — one removal boundary for all forms; audits for inline/block/prose conversions, fence removal, construction/result-use evidence |
| 3 | Paired control-fact recurrence counting removed with the new single-failure rule | blocking | fix — restore recurrence counting alongside the single-failure rule (both apply) |
| 4 | Timeout scenarios admit a broad interval (a wrong timeout passes) | blocking | fix — coordinator decision: drop the timeout from the critical facts (black-box timing cannot pin 30 s without flaky margins; timeouts are not H04's contract question); record the change |
| 5 | Construction runs use identical API and OAuth origins (swapped/reused origins undetected) | blocking | fix — distinct receivers, destination-ownership assertions, swapped/reused-origin controls |
| 6 | Session working-directory names reveal the arm (cwd is reader-visible) | blocking | fix — opaque directory names with no arm, commit or run identifier |
