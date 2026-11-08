# Review h04-instrument-review-8-sol (INSTRUMENT re-review, H04 bank, on dcf99c21)

Reviewer: Codex gpt-6.1-sol, xhigh, read-only (observed from rollout turn_context),
2026-11-08T08:24:12Z–08:30:30Z, thread 01a1ba42-0b5d-7331-aa01-7225d13e2ba6.
**Verdict: REJECT** (2 blocking). Confirmed: 515 filter cases, five negative controls, 29 gate and
30 decision checks; single-pass substitution, pinned sheet/context digests, immutable worker
strings; bank 636a6738, prompt/material digests, unchanged B/B prompt hashes; pilot-2 uses distinct
targets with correct provenance on all 12 JSON files (wrong-arm and pre-fix JSON rejected);
round-7 evidence and scorer validation. Candidate diffs not inspected. Not executed: builds,
readers, runners, bwrap.

| # | Finding | Severity | Triage |
|---|---|---|---|
| 1 | The byte-identical exemption ignores context: a changed template with regenerated B/B prompts and manifest passes without clearance; identical replacement of both arms' stored materials also passes | blocking | fix — round 8: anchor the exemption (and every clearance) to approved digests recorded in the sealed bank's validation record: template, cheat sheet, and each arm commit's baseline materials (rendered from the commit, verified by provenance); any difference requires clearance |
| 2 | A frozen run accepts a replacement schedule (`schedule --force`; arm commits replaced; completed sessions vanish from scoring) | blocking | fix — refuse schedule replacement after the first session; bind the schedule digest into the freeze, resume and scoring; check schedule/manifest/session metadata consistency |
