# Retry

Retry is **off** unless the application installs a `RetryPolicy`. A policy
expresses appetite; it never grants authority. Every replay still passes the
endpoint's effect, body, budget, cap and deadline gates (§13.4-§13.6).

## Installing a policy

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

<!-- concord-source: crates/concord-examples/src/runtime_policy/retries.rs#managed_retry_client -->
```rust
pub fn construct() -> Result<RetryApi, concord::prelude::ClientBuildError> {
    RetryApi::builder().retry(managed_retry_policy()).build()
}
```

The builder's `retry(policy)` and `configure_runtime(|r| r.retry_policy(policy))`
install a policy; `disable_retry_policy()` removes it. There is no retry DSL
clause: retry is runtime policy, not contract meaning.

| `RetryPolicyBuilder` method | Default | Meaning |
|---|---|---|
| `max_retries_per_operation(n)` | 2 | additional policy attempts per direct request or page; hard maximum 16 |
| `max_extra_load(ratio)` | 0.20 | shared load budget ratio, finite, `0.0..=1000.0` |
| `without_budget()` | budget on | trusted opt-out of the load budget, disclosed |
| `budget_scope_capacity(n)` | 4,096 origins | budget state capacity |
| `budget_scope_ttl(d)` | 15 minutes | idle TTL of per-origin budget state |
| `backoff(RetryBackoff)` | immediate | `RetryBackoff::fixed(d)` or `RetryBackoff::exponential(initial, maximum, jitter)?` |
| `honor_server_delay_up_to(d)` | zero | the longest server-requested delay the policy is willing to wait |
| `classify(classifier)` / `classify_fn(f)` | required | the `RetryClassifier` |

`build()` validates the configuration (a classifier is required) and returns a
`RetryPolicyBuildError` with a value-free kind otherwise.

## The classifier

A classifier receives a sanitized `RetryAttempt`: the endpoint ID, method, host
only when public, page index, `attempt()` facts, `retries_performed()`, the
last attempt's `dispatch_knowledge()` and a semantic `failure()`:

- `RetryFailure::Connect`
- `RetryFailure::Transport`
- `RetryFailure::RejectedStatus { status, server_delay }`

It returns `RetryDecision::Retry` or `RetryDecision::Stop`. It never sees raw
headers, bodies or credentials and cannot declare dispatch certainty or bypass
any gate. The executor separately retains aggregate dispatch knowledge across
attempts and uses it when deciding whether replay is safe. A later attempt
that conclusively did not send does not erase an earlier observed or ambiguous
attempt.

## When a policy replay happens

A policy replay requires **all** of (RETRY-01):

1. an eligible failure (connect, transport or rejected status) and a classifier
   `Retry`;
2. a mechanically replayable or reconstructible body;
3. either settled no-dispatch, or an endpoint effect of `safe` or `idempotent`;
4. remaining per-operation policy retries;
5. remaining aggregate send and replay caps (`CallOptions`);
6. shared retry-load budget;
7. a valid backoff and server minimum within the policy's willingness and the
   remaining deadline;
8. a new rate admission.

Accepted-head validation failures, accepted-body conversion failures,
pagination controller failures and failures after a stream is handed over are
terminal. A `non_idempotent` operation that possibly or observably reached the
server is never resubmitted automatically.

## Backoff and the load budget

Backoff is immediate, fixed, or capped exponential: retry number `n` waits
`min(initial * 2^(n-1), maximum)` with checked arithmetic. `RetryJitter::Full`
selects a value between zero and the delay, `Equal` between half the delay and
the delay, and `None` uses the delay itself; production entropy belongs to core
and tests inject it deterministically. Semantic and count
gates are checked before sleeping; the retry-load reservation is taken after the
backoff, immediately before committing to the next visible attempt. Cancelling
while sleeping consumes no new reservation.

The load budget is shared by clones of one policy and not by independently
built policies. It is scoped by normalized logical origin (using the protected
actual origin even when telemetry redacts it) and tracks ten one-second buckets
of initial dispatches and approved retries: at most
`ceil(ratio * initial_dispatches)` approved retries in the active window, with
no startup credits. Exhausted, full or poisoned budget state denies *extra*
retries, never initial requests.

## Server delay

Core captures the wall time once when a response head arrives and parses one
`Retry-After` value once: nonnegative delta seconds or a valid HTTP date.
Malformed, negative or past dates give no hint; zero is a valid zero minimum;
multiple occurrences are ignored as malformed; an overlarge value forbids retry
rather than wrapping.

A retry waits `max(backoff, remaining server minimum)`, never their sum
(RETRY-03). If the untruncated minimum exceeds the policy's willingness
(`honor_server_delay_up_to`) or the remaining deadline, the operation stops
instead of retrying early. Limiter cooldowns and status-body diagnostics may
retain a clamped duration (60-second default caps), but the retry gate never
uses a clamped value to violate a larger observed minimum. `ApiError::retry_after()`
exposes the parsed hint.

## Three separate replay authorities

| Authority | Scope | Limit |
|---|---|---|
| Policy retry | `RetryPolicy`, gates above | installed default 2, hard 16 per direct request or page |
| Auth recovery | an attributed challenge, a provider refresh yielding usable material, a replayable body, a `safe`/`idempotent` effect, remaining caps and deadline, a new admission | at most 1 per direct request or page |
| Transport-private recovery | a protocol-proven unprocessed try with a natively replayable body, inside one visible attempt | at most 2 physical tries per visible attempt |

Auth recovery uses no policy backoff or classifier but counts toward the send
and replay caps. Static and endpoint-backed sources never recover
automatically. Transport-private recovery repeats no preparation, credential,
admission or public attempt event; an observed rejected response is not a
no-processing signal. Deployments that require one physical dispatch per
visible attempt disable it:

<!-- concord-source: crates/concord-examples/src/runtime_policy/production_http.rs#protocol_recovery_toggle -->
```rust
/// Disables private known-safe protocol recovery when deployment policy
/// requires one physical dispatch for each visible Concord attempt.
pub fn build_single_dispatch_client() -> Result<ProductionApi, concord::ClientBuildError> {
    ProductionApi::builder()
        .configure_http(|http| http.protocol_recovery(false))
        .build()
}
```

Pagination continuation is not a retry: each next page is a new page identity
with its own admission under the traversal's aggregate budgets, and no retry
path resets a cursor loop.

## Counting

`ExecutionMeta::attempts_started()` and `policy_retries()`, the traversal
summary's `application_attempts()` and `automatic_replays()`, and
`ApiError::operation_counts()` keep policy retries, auth recoveries, physical
tries and page continuations separate. Caps intersect across terminal wrappers
and never reset.
