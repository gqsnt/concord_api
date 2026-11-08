# Review h04-instrument-review-7-sol (INSTRUMENT re-review, H04 bank, on a3fceb08)

Reviewer: Codex gpt-6.1-sol, xhigh, read-only (observed from rollout turn_context),
2026-11-08T08:10:29Z–08:14:52Z, thread 01a1ba35-7c23-72a3-9fff-c16c5e060421.
**Verdict: REJECT** (4 blocking). Confirmed: 515 filter cases, five negative controls, 29 gate
checks, 30 decision checks; bank 636a6738; 25 render hashes and 12 B/B prompts byte-identical to
rounds 4–5. Not executed: runners, builds, sessions, bwrap.

| # | Finding | Severity | Triage |
|---|---|---|---|
| 1 | Placeholders (`{TASKS}`, `{CRATE}`) inside inserted documentation are expanded after hashing (readers receive text other than the cleared diff) | blocking | fix — round 7 (with the stale-rustdoc bug): single-pass substitution into the template only; final prompt verified against the cleared representation |
| 2 | Common prompt content (cheat sheet via unchecked `manifest.bank_commit`, template) escapes clearance; a second `prompts` run can keep a clearance after a template change | blocking | fix — pin the cheat sheet to the bank's commit; bind template, sheet, bank and final prompt digests into the clearance / the baseline exemption |
| 3 | Workers re-read prompt files after validation (time-of-check/time-of-use) | blocking | fix — pass the validated immutable prompt strings to workers |
| 4 | Rebuilding prompts can mix sessions from different prompt versions (resume skips without comparing `prompt_sha256`; the scorer does not check the manifest) | blocking | fix — freeze run inputs once sessions begin; reject mismatched prompt hashes in resume and scoring |

Also found in use by the coordinator (not this review): rendering two arms into one cargo target
reused stale rustdoc JSON (git-archive mtimes) — both arms came out byte-identical; the first
pilot-1 renders are quarantined; fixed in the same round.
