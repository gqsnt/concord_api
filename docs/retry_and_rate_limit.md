# Retry and rate limiting

The [DSL specification](spec.md#rate-limit-profiles-and-policy) owns
rate-limit profile and attachment grammar. Retry policy is configured in
Rust.

## Retry policy

```rust
let policy = concord::retry::RetryPolicy::builder()
    .max_retries_per_operation(2)
    .max_extra_load(0.20)
    .classify_fn(|attempt| {
        use concord::retry::{RetryDecision, RetryFailure};
        match attempt.failure() {
            RetryFailure::Connect | RetryFailure::Transport => RetryDecision::Retry,
            _ => RetryDecision::Stop,
        }
    })
    .build()?;

let client = ServiceApi::builder().retry(policy).build()?;
```

The per-operation retry count is a hard bound. The rolling extra-load budget
is a second bound. A retry occurs only when the classifier admits it and both
bounds allow it.

The classifier receives safe attempt identity, attempt number, elapsed time,
and a reduced failure kind. It does not receive body bytes, credentials,
native errors, or the authenticated request.

Every admitted retry reconstructs the visible attempt under the original
operation timeout and reacquires the same effective rate-limit policies.

Retry policy clones share rolling-budget state. Independently built policies
own independent state.

## Resolved rate-limit policy

The compiler resolves each attachment into an ordered policy containing:

- an explicit or derived `RateLimitId`;
- ordered logical key-part names and formatted values;
- a positive cost;
- one or more ordered finite windows.

The selected `RateLimiter` receives the complete ordered plan before private
authentication placement.

## Default limiter

The façade `rate-limit` feature installs a new bounded `LocalRateLimiter` in
each built client. An explicitly configured limiter takes precedence.

Without a selected limiter, an empty plan executes and a nonempty plan fails
closed.

Default-built clients do not share limiter state. Supplying the same limiter
object to multiple clients shares compatible rule, key, and window state.

## Local limiter

`LocalRateLimiter` implements bounded process-local token buckets. It owns
finite quota entries, keyed cooldown state, pruning, and deterministic
eviction.

It is not a distributed strict-quota implementation. A distributed limiter
implements `RateLimiter` and owns cross-process consistency.

## Response observation

`RateLimitObserver` receives a sanitized response head and the resolved policy
plan before the user response hook.

An observer may update cooldown or quota state and may reject the operation
with a safe rate-limit error. It does not receive a response body or native
response.

## Identity and sharing

Bucket identity includes the limiter object, effective rule ID, ordered key
values, and window. Key order is significant.

An explicit ID permits intentional sharing across source renames or separate
clients. The application owns schema and remote-quota compatibility when one
limiter is shared across separate `api!` invocations.

Resolved key values may contain tenant, account, region, host, or endpoint
data. Built-in formatting and errors suppress them. Custom limiters and
observers own equivalent protection in storage and logs.

## Related

- [DSL rate-limit profiles and policy](spec.md#rate-limit-profiles-and-policy)
- [Runtime model](runtime.md)
- [Customization](customization.md)
