# Review h04-instrument-review-6-sol (INSTRUMENT re-review, H04 bank, on 9ce64a00)

Reviewer: Codex gpt-6.1-sol, xhigh, read-only (observed from rollout turn_context),
2026-11-08T07:59:23Z–08:04:31Z, thread 01a1ba2b-51ad-7612-839d-d8be85a58dbd.
**Verdict: REJECT** (2 blocking). Confirmed: six B renders and 12 B/B prompt hashes reproduce;
bank 636a6738 unchanged (137 files, 24 tasks, 66 facts); 515 filter cases, five negative
controls, 11 gate checks, 30 decision cases pass; per-SDK/overall digests and `sessions`
re-hashing bind the added-text files; decision/scoring and isolation code unchanged. Not
executed: cargo, fixtures, runners, sessions, bwrap.

| # | Finding | Severity | Triage |
|---|---|---|---|
| 1 | Moves and deletions escape review (moved prose listed only by its new type line; a deleted heading gives empty added text; an old clearance survives later deletions) | blocking | fix — round 6: clearance binds the **complete** candidate materials (per-SDK SHA-256 of the exact rendered materials incl. public types, and an overall digest); the review evidence is a full unified diff of baseline vs candidate materials per SDK (removals, both ends of moves, whitespace and structure preserved) with the added-text highlights; any change to the materials requires a new clearance |
| 2 | Normalisation hides reader-visible structure (list indentation, table boundaries) | blocking | fix — same: the digest and the diff use the exact text, no normalisation |
