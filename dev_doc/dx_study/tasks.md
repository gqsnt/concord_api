# Gate 3 participant tasks

**Status:** Part of the v2.1 Gate 3 packet described in [`README.md`](README.md); not revised for v3.

Use only public/generated documentation, supplied normal examples, compiler
feedback, and ordinary editor/repository search. Do not inspect macro or core
implementation source. Record an answer and confidence from 1 (guessing) to 5
(certain) before checking or compiling it.

1. For declarations `x?: T` and `x?: Option<T>`, predict the generated stored
   type and the exact argument type accepted by `set_x`.
2. For `x?: Option<T>`, classify these calls as valid or invalid and state the
   meaning of each valid call: `set_x(None)`, `set_x(Some(value))`,
   `set_x(Some(None))`, `omit_x()`, and `reset_x()`. Distinguish a
   Concord-present Rust `None` from Concord-owned absence.
3. Add repeated query values using `append "tag" = each endpoint.tags;`
   without changing the declared Rust value type. Predict zero-, one-, and
   two-element wire cardinality.
4. Given inherited query/header patches, distinguish ordinary assignment,
   `when_present`, `append`, `remove`, `replace`, and `off`; state what survives
   for present and absent optional values.
5. For three supplied endpoints, identify the effective request and response
   representations independently and name the owning client, named scope, or
   endpoint declaration for each.
6. Explain a local request/response representation override and an `explicit`
   fence. Add one valid endpoint below each and predict the focused error for a
   missing representation below the fence.
7. Predict the endpoint-owned accepted status, effect, body replayability, and
   whether a policy retry is permitted for a safe, idempotent, and
   non-idempotent example.
8. Choose `PageLimit::Prefix` for a bounded preview and
   `PageLimit::CompleteWithin` for an all-or-error synchronization. Explain the
   result when each bound is reached before natural completion.
9. Distinguish `ByteStream` from `ItemStream<T>` for acquisition success,
   polling output, terminal transport/framing/decode failure, cancellation, and
   early drop.
10. Diagnose a supplied generated compiler error: identify the authored
    declaration, the wrong mental model, and a valid generated method without
    editing generated output.
11. Locate what `OperationObserver`, built-in debug, and `RateLimitRequest` may
    see, then contrast that value-free/narrow authority with explicit
    `RequestPlan` and `ApiResponse` URL/header access.
12. Make one local endpoint representation or effect change, predict which
    endpoints and caller/wire/security/runtime classifications its semantic
    diff will report, then compare the actual diff.

For every task, the facilitator records elapsed time, pages/types opened,
search behavior, compiler cycles, corrections, recovery, abandonment, and
confidence before feedback.
