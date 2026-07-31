# Retry

- **Audience:** application runtime authors
- **Purpose:** runtime-policy guide
- **Normative reference:** [runtime model](runtime.md)

Retry is application policy, not DSL contract syntax. The same remote operation
may be retried differently by an interactive client, a batch process, or an
administrative tool. Concord therefore installs retry through the generated
client builder.

## Build a finite policy

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

The per-operation retry count is a hard bound. The rolling extra-load budget is
a second bound intended to prevent aggregate retry amplification.

The classifier receives safe attempt identity, attempt number, elapsed time,
and a reduced finite failure kind. It does not receive body bytes, credentials,
native errors, or the authenticated request.

## Install it

<!-- concord-source: crates/concord-examples/src/runtime_policy/retries.rs#managed_retry_client -->
```rust
pub fn construct() -> Result<RetryApi, concord::prelude::ClientBuildError> {
    RetryApi::builder().retry(managed_retry_policy()).build()
}
```

Policy clones share rolling-budget state. Independently built policies own
independent state unless the application explicitly shares an object.

## Attempt semantics

An admitted retry repeats:

- before-send hook;
- body reconstruction;
- credential preflight;
- limiter admission;
- private authentication placement;
- transport;
- response-head observation.

It uses the original operation deadline rather than resetting timeout. A retry
is possible only when the classifier, hard bound, extra-load budget, remaining
deadline, body replay capability, and limiter all permit it.

## Retry versus authentication recovery

Retry says that the same attempt may succeed under application failure policy.
Authentication recovery says that one attributed credential was challenged and
its still-current material may need replacement.

Recovery has a separate one-per-operation bound, may update credential state,
and is reported separately in completion. Both still use the same executor,
deadline, body replay rules, and limiter admission.

## Retry and response handling

The classifier observes managed connection, transport, or rejected-status
failure categories. It does not receive arbitrary decode callbacks or native
response bodies. Authentication challenge is handled before ordinary status
acceptance and retry classification can finalize the result.

## Why retry is not endpoint syntax

Putting retry in the DSL would mix the remote contract with deployment and
application tolerance, create another inheritance algebra, and make the same
contract less reusable across applications. The builder is the correct owner
for policy that selects runtime behavior rather than remote API structure.

## Related

- [Rate limits](rate_limits.md)
- [Execution policy](execution_policy.md)
- [Authentication](auth.md)
- [Request bodies](request_bodies.md)
