# Retry classifiers

Implement a retry classifier when application semantics decide which eligible
failures may repeat. The classifier requests another visible managed attempt;
Concord retains replayability checks, attempt limits, shared budget admission,
the operation deadline, and the executor.

Use the exact Rustdoc contracts `concord::retry::RetryClassifier`,
`concord::retry::RetryAttempt`, `concord::retry::RetryFailure`, and
`concord::retry::RetryDecision`.

## Complete example

For ordinary policy, a closure is the smallest extension:

```rust
{{#include ../../crates/concord-examples/src/runtime_policy/retries.rs:managed_retry_policy}}
```

`classify_fn` adapts the closure to `RetryClassifier`. Implement the trait
directly when classification needs a named reusable type or deliberately
shared bounded state. In either form, `RetryPolicy::builder()` requires a
classifier and installs no policy by default.

## What can be classified

Concord calls the classifier only for:

- connection establishment failure;
- another transport failure;
- a response status rejected by the declared response specification.

A rejected status reaches the classifier only after limiter observation, the
response-head hook, authentication challenge handling, and
`ResponseBodySpec::accepts_status`. An accepted status, including an explicitly
accepted `404` or other non-2xx status, is never a retry candidate. Timeout,
hook, limiter, authentication, body, accepted-response, decoding, pagination,
and internal failures are not classifier inputs.

`RetryAttempt` exposes the stable endpoint ID, application method, final
logical host when available, page index, structural `AttemptInfo`, retries
already performed, and one `RetryFailure`. Rejected-status headers are
`SanitizedHeaders`; no native request, response, URI, body, credential,
Reqwest error, resolved rate key, or provider state crosses this boundary.

## A retry decision is not authority

`RetryDecision::Retry` asks Concord to try again. Concord then checks, in
order:

1. the configured per-operation maximum;
2. non-consuming prepared-body replayability;
3. the shared origin-scoped rolling budget.

Denial preserves the current transport failure or performs ordinary bounded
terminal status capture. A one-shot body therefore suppresses optional retry
without replacing the useful original result. A successful reservation is
conservative and is not returned if a later hook, reconstruction,
authentication, limiter, deadline, or transport stage fails.

Each admitted retry reruns before-send, complete attempt-body reconstruction,
current authentication preflight, limiter admission, private materialization,
transport, and response observation under the original operation deadline.
The operation still produces one completion. Authentication recovery remains
globally limited to one across all policy attempts.

## Idempotency and shared state

The classifier, not Concord, knows whether repeating an operation is safe for
the remote API. Consider method semantics, idempotency keys, endpoint identity,
and the effects of an uncertain transport result. Do not classify every
transport error merely because the body can be reconstructed.

Classification is synchronous trusted application work. Independent
operations may call one classifier concurrently, and policy clones share it
with the same bounded rolling-budget state. A custom classifier must bound
CPU, locks, allocation, caches, logging, metrics, and any application state.
Concord checks the deadline immediately after the callback returns but cannot
preempt it.

The logical host and sanitized response headers can still contain customer or
operational data. They are explicit application inputs, not safe metric labels
by default. Classifier formatting omits application-owned state, but the
implementation remains responsible for every value it logs or retains.

## Verification

Use deterministic scripted transport and paused time. Cover:

- every intended eligible failure and a nearby stop case;
- accepted non-2xx status exclusion;
- per-operation maximum and shared-budget denial;
- one-shot suppression and replayable body reconstruction;
- hook, authentication, limiter, and response-observation order on retries;
- one operation deadline and completion event;
- one global authentication recovery across policy attempts;
- concurrent calls when the classifier owns shared state;
- host/header sentinels absent from automatic formatting.

## Related

- [Managed retries](../features/retries.md)
- [Retry transient failures](../recipes/retries.md)
- [Request and response body specifications](body-specifications.md)
- [Hooks, trusted work, and bounds](hooks-and-trusted-work.md)
- [Managed retry contract](../contributor/contracts/retries.md)
- Rustdoc: `concord::retry::RetryClassifier` and
  `concord::retry::RetryPolicy`
