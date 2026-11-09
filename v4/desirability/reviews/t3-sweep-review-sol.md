# Review t3-sweep-review-sol (CORRECTNESS REPAIR review, T3 flake sweep, on f0f229e6)

Reviewer: Codex gpt-6.1-sol, xhigh, read-only (observed from rollout turn_context),
2026-11-08T18:00:06Z–18:05:31Z, thread 01a1bc51-4c05-78f3-b41c-81099d30796f.
(Author: a Claude subagent — this review is cross-provider.)
**Verdict: ACCEPT_WITH_FIXES** (1 non-blocking). Confirmed: complete 147-file diff, shared helpers,
assertion and timeout inventories, EVIDENCE.md; complete examples of each pattern and the rest
hunk by hunk; the five changed assertions keep their meaning; no test or function removed, no skip
or Windows exclusion added; helpers return the call's real outcome; `frozen` keeps a real-time
watchdog; OnceLock lifetime, socket options and refusal assertions (ConnectionRefused, not a
timeout); 22 dev-dependency changes and Cargo.lock's single line; saved hunks match the commit;
controls, restoration hashes, failure logs and stress summaries support the claims and separate
heavy-load from capped runs; unrepaired cases reasonably scoped as proposals.
Not verified by the reviewer: native Windows RST semantics for this commit (next CI run);
the stalled-body helper's 50 ms / 1000-poll settling is a heuristic.

| # | Finding | Severity | Triage |
|---|---|---|---|
| 1 | `time_out_attempts` tracks currently held replies, not per-attempt state; a second attempt could stall after a delayed reply is released during backoff (`paused.rs:146`); only one-attempt callers exist | non-blocking | fixed in 30cb4325 (coordinator, trivial: the unused multi-attempt API removed; `time_out` implements the single-attempt case; testserver, clippy, HubSpot/Zendesk pass) — trivial fix, no re-review needed (A9) |
