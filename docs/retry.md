# Retry and repeatability

Retry policy answers whether a failure looks worth retrying. Endpoint
repeatability and body replayability answer whether another send is permitted
and mechanically possible. Core combines all three with stage, budgets, attempts,
and deadline.

## Configure policy outside the DSL

<!-- concord-source: crates/concord-examples/src/runtime_policy/retries.rs#managed_retry_policy -->
```rust
pub fn managed_retry_policy() -> RetryPolicy {
    RetryPolicy::builder()
        .max_retries_per_operation(2)
        .max_extra_load(0.20)
        .classify_fn(|attempt| match attempt.failure() {
            RetryFailure::Connect | RetryFailure::Transport => RetryDecision::Retry,
            RetryFailure::RejectedStatus { status, .. }
                if status == concord::http::StatusCode::TOO_MANY_REQUESTS
                    || status.is_server_error() =>
            {
                RetryDecision::Retry
            }
            _ => RetryDecision::Stop,
        })
        .build()
        .expect("the documented retry policy is structurally valid")
}
```

The classifier receives bounded, credential-free failure context. It recommends
retry or stop; it cannot bypass core gates. Attempt maximum, extra-load/shared
budget, backoff, jitter, and classification remain runtime policy rather than
endpoint syntax.

Generated builders use one spelling: `.retry(policy)`.

## Declare semantic permission

Methods derive a default: GET, HEAD, PUT, DELETE, OPTIONS, and exact TRACE are
idempotent; POST, PATCH, and unknown custom methods are never-repeat by default.
An endpoint can explicitly select either result. “Idempotent” grants only
permission to resend after an eligible failure; it does not promise safety,
server correctness, deduplication, cacheability, or that retry is desirable.

Body replayability is independent. A one-shot stream blocks resend even for an
idempotent endpoint. A replayable encoding does not permit resend for a
nonrepeatable endpoint after dispatch may have occurred.

## Failure stages

Preparation and conclusive pre-dispatch failures report `NotDispatched`.
Transport loss after dispatch begins without a response is
`PossiblyDispatched`. Any response head gives `ResponseReceived`, even when
status, acquisition, or conversion fails later. This is transport knowledge,
not remote commit knowledge.

Conversion failures do not policy-retry. Acquisition failures and transient
statuses may retry only with repeatable/replayable state. Nonrepeatable endpoints
may retry only when core conclusively knows no dispatch occurred and any consumed
body can be reconstructed.

Every retry receives a new attempt ID, reacquires rate admission, consumes
attempt/shared budget, and remains under the original operation deadline.
Authentication recovery is separate and does not count as an application retry,
but shares total bounds.

See [runtime retry gates](runtime.md#retry-gates) and the
[DSL repeatability contract](spec.md#methods-and-repeatability).
