# Review pilot-harness-review-1-sol (INSTRUMENT review round 1 of ≤ 2, A3; pilot harness on 54e27660)

Reviewer: Codex gpt-6.1-sol, xhigh, read-only (observed from rollout turn_context),
2026-11-08T17:19:15Z–17:24:32Z, thread 01a1bc2b-e7cb-77a2-96a5-35466ea8cef6.
**Verdict: ACCEPT** (6 non-blocking; none changes a verdict on supplied data). Independently
verified: both frozen pilot hashes, the original H04 bank hash, unchanged ported keys/text/fixtures;
all 288 H04 pilot-3 task/fact/check verdicts identical (278 passes); H04 WIN recomputed (witness net
+6, +3 per provider, 0 control regressions); failed gates give LOSS, omitted facts stay pending;
archive hashes, target markers, run bindings, public summaries, transcript audits, candidate diffs;
82/82 selftest with wrong-request, non-compiling and undetected-omission controls; the `Batch`
change is behaviour-neutral without the new arguments.

| # | Finding | Severity | Triage |
|---|---|---|---|
| 1 | Reader scores not bound to set/run hash and arm commits (`pilot.py:148`) | non-blocking | known limit (README) |
| 2 | Missing reader sessions omitted instead of zero-scored (`readers.py:188`) | non-blocking | known limit; cards relying on reader rates require completeness |
| 3 | `compile_at: arm` for reader items unavailable (`readers.py:200`) | non-blocking | known limit |
| 4 | Provenance checked before `Batch`, not before each cargo command in it (`h04/harness.py:368`); README overstated | non-blocking | known limit; README corrected |
| 5 | Header exemptions can hide substantive differences (`kinds.py:245`) | non-blocking | known limit; exemptions must be justified with an independent check |
| 6 | Clearance report location not restricted to `work/sealed` (`clearance.py:327`) | non-blocking | known limit |
Instrument accepted after round 1 (A3 cap 2 rounds not needed).
