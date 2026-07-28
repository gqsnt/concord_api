# Managed retries

Concord separates private protocol recovery from application retry.
Reqwest's known-safe protocol recovery is enabled by default inside one
visible Concord attempt. It does not retry HTTP statuses and it never exposes
a native retry policy. `HttpConfigBuilder::protocol_recovery(false)` disables
that private recovery when a deployment requires exactly one physical send.

Application retry is absent by default. Install an explicit
`concord::retry::RetryPolicy` when an API operation is safe to repeat.

## Define and install a policy

The canonical compile-checked policy retries connection and transport
failures, `429`, and server errors:

```rust
{{#include ../../crates/concord-examples/src/runtime_policy/retries.rs:managed_retry_policy}}
```

Install it directly on a generated builder:

```rust
{{#include ../../crates/concord-examples/src/runtime_policy/retries.rs:managed_retry_client}}
```

The equivalent lower-level form is
`configure_runtime(|runtime| runtime.retry_policy(policy))`. A later
`.retry(policy)` or runtime transformation replaces the earlier selection in
call order. `disable_retry_policy()` restores the no-policy selection.

## What one policy retry does

Every admitted retry is a new visible Concord attempt under the original
operation deadline and operation ID. It reruns:

1. `before_send`;
2. managed-body reconstruction;
3. current authentication preflight;
4. limiter admission;
5. private request materialization and send;
6. limiter response observation and the response-head hook;
7. authentication challenge, declared status acceptance, and response policy.

The operation emits one completion. `AttemptInfo`, `ExecutionMeta`, and
pagination completion expose checked attempt and policy-retry totals.

Accepted statuses are never classified. For rejected statuses, limiter
observation, the response hook, authentication challenge handling, and
declared acceptance all happen before the classifier. An intermediate
response selected for retry is dropped without polling its body.

## Bounds and replayability

`RetryPolicy` requires a classifier. Its default per-operation maximum is two
policy retries, with a hard maximum of 16. That per-operation maximum is
independent of the rolling budget.

The default rolling budget uses a 20% target ratio per logical origin over ten
seconds. Concord permits `ceil(initial dispatches × ratio)` managed retry
reservations. Whole-retry upward rounding intentionally allows one retry after
a single initial dispatch. It can make the budget allowance ratio exceed the
configured ratio at low volume. The allowance ratio converges toward the
configured value as initial dispatch volume grows. Actual approved retry
traffic can be lower because a classifier may stop, a body may be
non-replayable, budget admission may deny a reservation, or the independent
per-operation maximum may intervene. The low-volume allowance lets an
intermittent request recover without waiting for five initial dispatches.
Budget denial, including bounded origin-state capacity pressure, suppresses the
retry and preserves the current result.

After a classifier returns `Retry`, Concord checks the per-operation maximum,
body replayability, then shared budget admission. Buffered bytes and encoded
codec output are reused. Factories and replayable multipart recipes rebuild
per visible retry. A one-shot stream or multipart body suppresses optional
retry without consuming budget or replacing the original result.

## Related

- [Retry recipe](../recipes/retries.md)
- [Retry classifiers](../extending/retry-classifiers.md)
- [Production HTTP](production-http.md)
- [Hooks and observability](hooks-and-observability.md)
- [Managed retry contract](../contributor/contracts/retries.md)
- [Limits](../reference/limits.md)
