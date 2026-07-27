# Dangerous capabilities

Dangerous capabilities are explicit Cargo features outside the default bundle.
They remain inside Concord's managed executor but hand the application policy
with reduced ordinary safety guarantees.

Cargo feature unification applies: if any dependency enables a dangerous
feature, its public surface is enabled for the resolved `concord` package in
the whole binary.

## Custom native retry policy

Feature: `dangerous-custom-retry`.

The feature exposes Reqwest's retry builder as
`concord::dangerous::ReqwestRetryBuilder` and adds
`HttpConfigBuilder::dangerous_custom_retry`.

The canonical example deliberately installs a no-retry native policy:

```rust,ignore
{{#include ../../crates/concord-examples/src/dangerous/custom_retry.rs:dangerous_retry_policy}}
```

```rust,ignore
{{#include ../../crates/concord-examples/src/dangerous/custom_retry.rs:dangerous_retry_client}}
```

The capability is dangerous because a native retry classifier may observe
materialized native URLs, including private query authentication. Logging or
retaining classifier inputs is outside Concord's redaction boundary.

A native policy remains inside one visible Concord attempt:

```text
one limiter admission
→ one before-send hook event
→ private authentication placement
→ one or more policy-owned physical sends
→ one response-head path
→ one completion event
```

The policy cannot create a second Concord executor or bypass final status,
authentication challenge recovery, response acquisition, or completion.
Provider HTTP uses its separate managed client and is not configured by this
application-transport policy.

Multiple physical sends share one limiter admission and are not individually
visible to hooks or completion. That can diverge from remote quotas and
observability expectations. Review:

- HTTP method and application idempotency;
- whether the body is reusable or reconstructible;
- maximum retry count and total delay;
- status/error classifier inputs;
- interaction with an outer application retry;
- service-side deduplication and rate accounting.

Runtime replayability control rejects retry of a one-shot body. It cannot prove
that a semantically replayable body is safe for the remote operation.

Prefer DSL `retry off` or the reviewed `retry reqwest_default` policy. Use a
custom native policy only when its exact classifier and finite schedule are
owned and tested by the application.

This feature lets a classifier inspect a native URL and perform hidden physical
sends. The managed attempt boundary, replayability checks, and final response
path remain enforced. It does not expose a native response, arbitrary middleware, redirect
control, a transport client, or an unbounded acquisition mode.

## Verification

Use deterministic local transport and assert physical send
count, visible hook/limiter/completion count, body replayability, cancellation,
and absence of native URL or credential sentinels from application
diagnostics. Never use live failures or wall-clock sleeps as the policy's
contract test.

## Related

- [Feature flags](features.md)
- [Production HTTP](../features/production-http.md)
- [Request and response body specifications](../extending/body-specifications.md)
- [Security and redaction](../concepts/security-and-redaction.md)
- [Unsupported forms](unsupported.md)
- Rustdoc: `concord::dangerous::ReqwestRetryBuilder`
