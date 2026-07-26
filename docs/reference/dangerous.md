# Dangerous capabilities

Dangerous capabilities are explicit, independent Cargo features. Neither is in
the default bundle. Both remain inside Concord's managed executor, but each
hands the application data or policy with reduced ordinary safety guarantees.

Cargo feature unification applies: if any dependency enables one of these
features, its public surface is enabled for the resolved `concord` package in
the whole binary.

## Bounded raw successful response

Feature: `dangerous-raw-response`.

The feature generates `.raw_response()` on named requests. The selector
type-checks only when the declared response mode implements the sealed
`RawResponseMode`; Concord provides that capability for `Buffered` and
`Discard`, not `Streaming`.

This compiled example selects the terminal:

```rust,ignore
{{#include ../../crates/concord-examples/src/dangerous/raw_response.rs:dangerous_raw_client}}
```

Awaiting it:

- retains ordinary public shaping, hooks, authentication, limiter admission,
  status classification, operation deadline, completion, and redaction;
- skips the declared successful-response validation and decoder;
- collects one complete successful post-decompression body under the ordinary
  buffered-response limit;
- returns `concord::dangerous::RawResponse`, never `reqwest::Response`.

Non-success status still becomes the ordinary typed `ApiError` and bounded
terminal-status capture. The raw terminal does not bypass status policy or
create an unbounded body.

`RawResponse` exposes the credential-sanitized logical URL, successful status,
HTTP version, safe execution metadata, actual successful headers, and complete
bounded body bytes. The actual headers and body may contain secrets.
`Debug` omits their values, but explicit `headers()`, `body()`, and
`into_body()` access makes them application-owned.

Use raw success for one-off inspection or migration when no reusable response
specification is yet appropriate. For stable behavior, implement
`DecodeBody` or `ResponseBodySpec` so media validation, conversion, and output
typing remain part of the declaration.

Raw success is not available for byte streams or record streams. Selecting a
streaming declaration and then buffering it through a dangerous terminal would
erase the declared liveness and memory contract, so the sealed mode bound
rejects it.

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

## Capability comparison

| Feature | Reduced guarantee | Still enforced |
|---|---|---|
| `dangerous-raw-response` | caller receives actual successful headers and bounded body bytes without declared decoding | status, byte limit, lifecycle, private transport |
| `dangerous-custom-retry` | classifier may inspect native URL and perform hidden physical sends | managed attempt boundary, replayability, final response path |

Neither feature exposes a native response, arbitrary middleware, redirect
control, a transport client, or an unbounded acquisition mode.

## Verification

For raw success, test exact response limits, first excess, post-decompression
counting, non-success status, actual header/body access, and redacted
formatting.

For custom retry, use deterministic local transport and assert physical send
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
- Rustdoc: `concord::dangerous::RawResponse` and
  `concord::dangerous::ReqwestRetryBuilder`
