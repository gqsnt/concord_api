# Review t2-review-sol (CORRECTNESS REPAIR review, T2, on 0b4b3061)

Reviewer: Codex gpt-6.1-sol, xhigh, read-only (observed from rollout turn_context),
2026-11-08T13:54:18Z–13:59:08Z, thread 01a1bb70-438c-7743-a09e-8a8249b573d8.
**Verdict: REJECT** (2 blocking, 1 non-blocking). Confirmed: Snowflake's `case + 1` keeps exactly
one request per case; PayPal keeps its uncertainty, attempt, request-id, identical-body and
reconciliation checks, the paused clock prevents incidental timeouts, diagnostic logs show token
transport failures; stream-stress's whole-chunk measure removes read-boundary dependence and keeps
its assertions; `request_body_bytes()` unchanged (Mailgun); the held non-listening socket removes
the port-reuse window; assertions stay sensitive to the guarded regressions; the cancellation
failure predates the patch (separate item). The 21/63 inventories need tracked follow-ups, not T2
expansion; the accept-loop teardown is relevant now.

| # | Finding | Severity | Triage |
|---|---|---|---|
| 1 | Snowflake determinism unproved: the test server's accept loop still breaks on any accept error and aborts accepted connections (`examples/testserver/src/lib.rs:507`); no observed causal link | blocking | fix — round 2: demonstrate the failure with an injected accept error, fix the accept loop (skip a failed accept), keep assertions |
| 2 | Required control evidence missing (logs overwritten/deleted; no saved commands, mutations, outputs, restoration, stress summaries) | blocking | fix — round 2: an evidence record with exact commands, mutation diffs, failing outputs, restoration hashes, stress summaries, tied to the reviewed source |
| 3 | `request_chunk_bytes()` accepts any two bytes as the chunk terminator | non-blocking | fix — validate CRLF; tests for missing, split and invalid terminators |

Follow-ups recorded (not T2): 21 short-timeout tests sharing a token/retry timeout; 63 bind-and-drop
port sites; stream-stress cancellation test under load.
