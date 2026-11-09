# Review i3g-instrument-review-sol (INSTRUMENT review round 1 of ≤ 2, A3; I3 --generated-docs on 7a34bbbb)

Reviewer: Codex gpt-6.1-sol, xhigh, read-only (observed from rollout turn_context),
2026-11-09T02:29:46Z–02:35:22Z, thread 01a1be23-e7e8-71d0-8e89-ac55b2ca27a8.
**Verdict: ACCEPT_WITH_FIXES** (1 non-blocking). Verified: 97 default outputs byte-for-byte (64 prompts, 32
metadata, pack log) against HEAD^; 66 bank/truth inputs unchanged; both arms' archive hashes, isolated target
markers, JSON commit markers; all 128 prompts and 720 appendix blocks (endpoint identities, hashes, question
coverage); Kubernetes watch exposes the candidate's composition changes; additions are identity headings and
raw docs, no truth-derived answers; conservative example flags on every block; selftest comparisons reproduced;
negative checks reject missing/hidden docs, stale JSON and wrong provenance.

| # | Finding | Severity | Triage |
|---|---|---|---|
| 1 | Candidate wrapper notes (ClickHouse ping `Text`, B2 download `Binary`: no `Accept`) are absent from the scored prompts; could affect H16r R1.exact on those SDKs | non-blocking | H16r's claim is prospectively limited to endpoint documentation (card); README known limit added |
Instrument accepted after round 1.
