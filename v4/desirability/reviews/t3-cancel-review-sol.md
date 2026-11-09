# Review t3-cancel-review-sol (CORRECTNESS REPAIR review, T3 cancellation, on 7f29045e)

Reviewer: Codex gpt-6.1-sol, xhigh, read-only (observed from rollout turn_context),
2026-11-08T17:08:22Z–17:14:14Z, thread 01a1bc21-fb56-7352-bbe4-0cf74ae5aece.
**Verdict: ACCEPT_WITH_FIXES** (2 blocking). Confirmed: a real defect, not a measurement race
(locked probe counters, post-drop backtraces through hyper's write loop, instrumented hyper);
Windows' 14,336 plausibly the same mechanism (inference); flag-before-mutex ordering prevents later
inner polls and waits out the in-progress one; poison recovery sound; no new user Send/Sync/Unpin
bounds; frames, trailers, hints and errors forwarded; `UploadError` still reachable via `source()`;
receive/retry routing, response-completion disarming, `wrap_stream` coverage, buffered bodies
replayable; lockfiles add only the `http-body` edge (no package); the direct wrapper covers cases an
`EncodedBody` token would miss; smoke test and harness unchanged; per-frame cost reasonable. Noted:
drop synchronously occupies an executor thread until the active poll returns (a blocking or
reentrant source can make that wait unbounded or deadlock).

| # | Finding | Severity | Triage |
|---|---|---|---|
| 1 | The "receiver never sees a complete body" promise exceeds the guarantee: an in-progress poll can return `None`, final data or trailers after the flag is set and is forwarded; `is_end_stream()` ignores cancellation; exact-length bodies can complete without another poll (`cancel.rs:101`) | blocking | fix — state the boundary (frames already handed to the transport), or recheck cancellation after the inner poll to suppress an ending result; cover final frames, trailers, known lengths |
| 2 | `upload_cancel.rs` is timing-dependent (200 ms sleep; C3 can pass if its polls land between the drop and the counter load; server-side close ≠ body destruction; zero live connections can precede acceptance) | blocking | fix — barrier-driven guard tests with explicit cancellation/drop synchronization and a body-destruction signal; keep the integration case as transport coverage, not an exactness claim |
