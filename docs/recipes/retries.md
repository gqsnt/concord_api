# Retry transient failures

## Goal

Retry a small, explicit set of transient failures while keeping application
load, attempts, bodies, authentication, rate limits, and completion under
Concord's managed lifecycle.

## Complete policy

```rust
{{#include ../../crates/concord-examples/src/runtime_policy/retries.rs:managed_retry_policy}}
```

The classifier receives only safe semantic context. This example retries
connection and transport failures plus rejected `429` and `5xx` statuses. Add
an operation-specific idempotency condition when the same client contains
non-repeatable remote operations.

The policy keeps the default bounded ten-second origin budget and explicitly
retains its 20% extra-load ratio. It permits at most two policy retries for
one request or pagination page.

## Install it

```rust
{{#include ../../crates/concord-examples/src/runtime_policy/retries.rs:managed_retry_client}}
```

Generated `.retry(policy)` and
`.configure_runtime(|runtime| runtime.retry_policy(policy))` select the same
runtime policy. Prefer `.retry` for the ordinary generated-client path. Use
the lower-level form when composing several runtime settings in one
transformation.

## Validate the result

Test the policy offline with scripted connection/transport failures and
responses. Assert:

- the classifier sees only intended failure categories;
- accepted non-2xx statuses are not candidates;
- each admitted retry reruns hooks, authentication preflight, and limiter
  admission;
- buffered encoders run once while factories run once per visible attempt;
- one-shot bodies preserve the original failure;
- completion occurs once with the expected attempt and retry totals.

Private protocol recovery is independent and enabled by default. It can
recover a known-safe protocol event inside one visible attempt, but it does
not classify statuses or consume managed retry budget.

## Related

- [Managed retries](../features/retries.md)
- [Retry classifiers](../extending/retry-classifiers.md)
- [Testing API clients](testing-api-clients.md)
- [Rate limiting](../features/rate-limiting.md)
- [Managed retry contract](../contributor/contracts/retries.md)
