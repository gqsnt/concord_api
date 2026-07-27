# Hooks, trusted work, and bounds

Implement runtime hooks for bounded lifecycle observation or application
policy at Concord's stable event boundaries. Hooks receive sanitized semantic
metadata, not bodies, credentials, a native request, or a transport handle.

This page also owns the cross-cutting liveness rules for all extension
callbacks. Read it before an extension performs I/O, shares mutable state, or
creates background work.

The exact hook Rustdoc item is `concord::hooks::RuntimeHooks`.

## Complete example

This compiled implementation counts attempts, received heads, and completions,
then installs the shared object on one immutable client:

```rust,ignore
{{#include ../../crates/concord-examples/src/runtime_policy/hooks.rs:hooks_extension}}
```

The counter state is finite and every callback completes immediately. Real
telemetry exporters need an equally explicit overload and shutdown policy.

## Lifecycle callbacks

`RuntimeHooks` has four callbacks:

| Callback | Timing | Can determine the result? |
|---|---|---|
| `before_send` | after public preparation and structural body metadata, before credential acquisition, body preparation, and transport | yes, by returning `HookError` |
| `on_response_head` | after limiter observation, before authentication challenge, status handling, and body acquisition | yes, by returning `HookError` |
| `on_complete` | after one direct request or page result is fixed | no; it is awaited before delivery |
| `on_pagination_complete` | after the aggregate pagination result is fixed | no; it is awaited before delivery |

The first two futures run under the remaining operation deadline. Completion
callbacks run outside that deadline. They cannot change an already-fixed
outcome, but a future that never resolves prevents the caller from receiving
it. Put a timeout, bounded queue, or nonblocking handoff around fallible
external telemetry.

Completion is dispatched exactly once for each visible direct/page operation
that reaches the completion owner. Pagination has page completions plus one
aggregate completion. Buffered record collection fixes `Success` or a decode
failure before completion. Opening a byte or record stream instead fixes a
`StreamOpened` outcome; later stream polling does not emit a second completion
event.

## Event identity and redaction

Events expose stable endpoint and operation IDs, method, credential-sanitized
logical URL, page index, `AttemptInfo` where relevant, structural body facts,
status, elapsed duration, and a bounded outcome category. `AttemptInfo`
separates the zero-based visible-attempt index, managed policy-retry index, and
authentication-recovery flag. The initial attempt is `{0, 0, false}` and the
current authentication replay is `{1, 0, true}`.

Direct/page completion events report `attempts_started`, `policy_retries`, and
the authentication-recovery summary once the result is fixed. Aggregate
pagination completion reports checked sums of the page totals. These counters
are correlation facts, not a license to use operation or URL values as metric
labels.

Request and response headers use `SanitizedHeaders`; private authentication
placements are absent. The logical URL is also credential-sanitized.
Nevertheless, public path segments, query values, headers, hostnames, and
service response metadata may contain account IDs or personal information.
Apply the application's data classification before logging or labeling them.

`DebugLevel::Basic` emits body-free structural events. `Headers` also emits
sanitized headers. Neither level makes arbitrary public values safe or turns
debug output into a stable machine-readable schema.

## Safe vetoes

`before_send` can reject before body materialization and native execution.
`on_response_head` can reject after the service has already received the
request, so it must not be described as preventing the remote side effect.

Construct `HookError` only from application-approved safe text. Its message is
shown by `Display` and `Debug`; never insert a URL, header, credential, body,
provider response, or backend diagnostic. Completion callbacks return `()` and
cannot replace the fixed result with a hook error.

## Trust and liveness map

Concord enforces limits around extension boundaries, but application callbacks
remain trusted:

| Extension work | Execution boundary | What Concord bounds | What the extension must bound |
|---|---|---|---|
| `EncodeBody`, `DecodeBody`, body-spec callbacks | synchronous; deadline checked after return | managed request/response bytes and structural validation | CPU, output expansion, state, blocking I/O |
| request body factories and producers | factory is synchronous; stream is polled under managed execution | total bytes and successful empty chunks | producer state, source I/O, tasks, internal retries |
| `RecordDecoder` | synchronous during buffered conversion or caller polling | pending source bytes, staging bytes, response bytes, net progress | decoded allocation, parsing CPU, retained records |
| `CredentialProvider` | future under operation deadline | credential slots and provider HTTP bodies | provider caches, queues, retries, tasks, external effects |
| `PaginationController` | synchronous between pages, outside page deadline | page/item/identity ceilings and repeated request identity | controller state and callback time |
| `RateLimiter` | acquisition/observation futures under operation deadline | plan/key structure before callbacks | quota state, backend calls, overload behavior |
| `RateLimitObserver` | synchronous response policy | cooldown clamping after return | callback time and observer state |
| result-determining hooks | future under operation deadline | event shape and ordinary deadline | external work, queues, labels |
| completion hooks | awaited outside operation deadline | event shape and exactly-once dispatch | their own timeout and delivery policy |
| returned byte/record streams | caller-driven after opening | wire bytes, empty chunks, pending source, and record staging | consumption deadline, downstream queues, decoded objects |

Synchronous work cannot be preempted. A timeout can be observed only after the
callback returns. Avoid filesystem access, DNS, blocking locks, sleeps, and
unbounded parsing in those callbacks.

Dropping an asynchronous future on timeout or cancellation prevents later
polls. It does not undo a sent provider request, a quota reservation, a log
write, or a spawned task. Design external operations to tolerate cancellation
and duplicate attempts.

## Concurrency and shared state

Hook calls for independent operations overlap, and one hook object is shared by
client clones. Prefer atomics for small counters and bounded channels for
telemetry handoff. Do not hold a mutex across `.await`.

If a queue is full, choose and test a policy: drop an observation, aggregate
locally, backpressure within a finite deadline, or fail a result-determining
hook. Completion hooks cannot report failure, so they need an explicit
best-effort or application-owned durable handoff contract.

Keep metric names and label sets finite. Operation IDs are correlation values,
not sensible metric dimensions. Page cursors, URLs, account IDs, error text,
and header values should not become dynamic labels.

## Verification

Use deterministic gates and paused time instead of sleeps. Verify:

- hook order relative to body preparation, limiter, challenge, status, and
  decoding;
- veto before transport and response-hook rejection after a received head;
- one completion for success and every stable error category;
- exact attempt identity for initial and authentication-recovery callbacks,
  with one completion carrying two attempts and zero policy retries;
- page and aggregate pagination completion;
- stream-open completion without a second event on stream failure or drop;
- cancellation drops pending futures and does not claim rollback;
- finite behavior when queues are full or exporters are unavailable;
- all credential and sentinel values absent from event formatting.

Run the implementation through a generated client and share clones
concurrently. Direct callback unit tests cannot prove lifecycle ordering.

## Related

- [Hooks and observability](../features/hooks-and-observability.md)
- [Testing API clients](../recipes/testing-api-clients.md)
- [Production configuration](../recipes/production-configuration.md)
- [Extension model](README.md)
- [Finite behavior](../concepts/finite-behavior.md)
- Rustdoc: `concord::hooks::RuntimeHooks` and
  `concord::hooks::CompletionEvent`
