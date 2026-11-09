# Review t5-review-sol (DOCS-TRUTH review, T5, on 4235233b; plus the H01 disposition)

Reviewer: Codex gpt-6.1-sol, xhigh, read-only (observed from rollout turn_context),
2026-11-09T00:56:31Z–01:00:13Z, thread 01a1bdce-88d5-7f71-aa97-5074b778c765.
(T5 authored by a Codex lane: independent session, same provider.)
**Verdict: ACCEPT_WITH_FIXES** (1 blocking, 1 non-blocking). **H01 disposition: SOUND** — the survey
identifies unresolved visibility concerns but no measured caller-task deficit requiring a design card;
H04's ceiling results support this only for its tested materials; routing false prose to T5 and
existing-template adoption to H11 follows A1; A3's preference for static evidence does not establish an
additional deficit. Confirmed: all 33 corrections against generator signatures, policy outputs, stream
readers and error definitions; all 25 files reconstructed from the saved fragments (patch = commit);
non-documentation Rust, signatures, assertions and README fences unchanged; 66 doctests passed (4
ignored), Adyen/ntfy recheck 7 passed, quickstart 13 checks + 44 doctests passed, 396 record references.

| # | Finding | Severity | Triage |
|---|---|---|---|
| 1 | Anthropic stream doc: "without reading the body" covered the whole `Result` although non-2xx error bodies are read; pre-stream failures were all called `Error::Api` though build/transport/head-check failures are `Error::Concord` (`lib.rs:107`) | blocking | fixed in a8a548c4 (coordinator; doc wording against `SendPolicy::send` and `From<concord::Error>`); confirmation review below |
| 2 | Evidence claimed no fenced example changed, but a comment inside the ignored fence at `prepared.rs:226` changed | non-blocking | evidence corrected (comment-only exception disclosed) |
