# Review h04-instrument-review-9-sol (INSTRUMENT re-review, H04 bank, on 6ede0339)

Reviewer: Codex gpt-6.1-sol, xhigh, read-only (observed from rollout turn_context),
2026-11-08T08:37:24Z–08:41:15Z, thread 01a1ba4e-230e-7cf3-91df-bac51824dea3.
**Verdict: ACCEPT** (no findings). Both review-8 fixes close the reported bypasses (approved
context and fresh-material checks, schedule refusal after start, frozen schedule/arm binding,
resume/scoring metadata checks; 44 read-only in-memory guard checks passed). Bank 06be1a2b
recomputed (137 files); approved template/sheet hashes and the 12 B/B prompts unchanged from round
7; recorded evidence verified (32/32 e2e, 515 filter cases, five negative controls, 29 gate, 30
decision, 24 references, 94 negatives, 29 equivalents, 32 claims, expected synthetic verdict);
scoring rules, harness, reader template and worker isolation unchanged; pilot-2 has correct
provenance on 12 rustdoc files, refuses uncleared prompts, has no prompts or sessions. "No
remaining pilot-validity threat found." Not executed: re-renders, live runner/providers.

Decision: the H04 instrument (tooling `6ede0339` on local `dz/h04-bank`, sealed bank `06be1a2b`)
is **accepted** for the pilot.
