# Gate 3 participant tasks

**Status:** v3 participant tasks; use the exact candidate selected by the facilitator.

Use only public/generated documentation, supplied normal examples, compiler
feedback, and ordinary editor/repository search. Do not inspect macro or core
implementation source. Record an answer and confidence from 1 (guessing) to 5
(certain) before checking or compiling it.

Use the concrete declarations, error fixture and edit scenario in
[the exercise fixtures](fixtures.md). Record the selected candidate's revision
before starting. The fixture sheet supplies inputs, not a scoring answer key;
open compiler-output fixtures only after the relevant prediction is recorded.

1. For declarations `x?: T` and `x?: Option<T>`, predict the generated stored
   type and the argument types accepted by `set_x`, `replace_x` and
   `set_x_if_some`.
2. For `x?: Option<T>`, classify these calls as valid or invalid and state the
   meaning of each valid call: `set_x(None)`, `set_x(Some(7))`,
   `replace_x(None)`, `replace_x(Some(None))`, `set_x_if_some(None)`,
   `omit_x()`, and `reset_x()`. Compare an uninitialized slot with one that
   retains an initializer; distinguish inner Rust `None` from outer absence.
3. Add repeated query values using `append "tag" = each tags;`
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
8. Choose `collect_prefix(u32)` for a bounded preview,
   `collect_all(NonZeroU32)` for complete synchronization, and `pages(u32)`
   for an application-controlled traversal. Explain each result when its
   bound is reached before natural completion and when the bound is zero.
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
    `concord-xtask contract diff` will report, then compare the actual diff.

For every task, the facilitator records elapsed time, pages/types opened,
search behavior, compiler cycles, corrections, recovery, abandonment, and
confidence before feedback.
