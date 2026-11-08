# Review h04-instrument-review-5-sol (INSTRUMENT re-review, H04 bank, on c383065a)

Reviewer: Codex gpt-6.1-sol, xhigh, read-only (observed from rollout turn_context),
2026-11-08T07:48:51Z–07:52:42Z, thread 01a1ba21-acbb-7ed2-a03e-cb44f70f182a.
**Verdict: REJECT** (1 blocking, 1 non-blocking). Confirmed: review-4 cases removed; bank hash
636a6738 (137 files, 24 tasks, 66 facts); six B renders and 12 B/B prompts reproduce; 515
synthetic cases, five rule-off controls, eight backstop checks, 30 decision cases pass; regression
-only control-loss counting; recorded validation; clearance-refusal evidence; isolation code and
both provider transcripts. Not executed: builds, fixtures, runners, sessions, bwrap.

| # | Finding | Severity | Triage |
|---|---|---|---|
| 1 | The structural backstop misses added expressions written with legal Rust comments or blank lines between tokens: an added answer expression stays, `new_code_like_tokens=[]`, prompts would build | blocking | fix — round 5, by construction: the gate lists **all** text the candidate arm's filtered materials add relative to the baseline arm (normalised added lines per SDK), writes it to a file with a digest, and `prompts`/`sessions` refuse unless a clearance file carries that exact digest; the candidate's independent review reads the added text (and may read the sealed bank) and clears it only if no added text supplies an answer; the code-like token list stays as a highlight inside that file. Formatting no longer matters: anything added must be reviewed. |
| 2 | The committed card names the superseded bank hash | non-blocking | fix — card amended with the final bank hash and instrument provenance before any outcome is read |
