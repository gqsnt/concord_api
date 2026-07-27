# Hooks and observability

Install one `RuntimeHooks` object when application policy must observe or
reject managed operations without receiving credentials or native transport
values. The built client and all its clones share that hook object.

## Implement only the callbacks you need

Every callback has a default no-op implementation. This compiled observer
counts attempts, received response heads, and fixed results:

## Complete example

```rust,ignore
{{#include ../../crates/concord-examples/src/runtime_policy/hooks.rs:hooks_implementation}}
```

Install the implementation with `configure_runtime`:

```rust,ignore
{{#include ../../crates/concord-examples/src/runtime_policy/hooks.rs:hooks_client}}
```

Calls may overlap, so shared state must be concurrency-safe. Prefer bounded
counters, fixed-cardinality metrics, or bounded queues. Avoid an unbounded map
keyed by URL, endpoint value, tenant, or operation ID.

## Before-send policy

`before_send` runs after Concord has prepared and bounded the public request
head and structural body metadata, but before body materialization,
credential placement, and native execution. It can approve with `Ok(())` or
return a safe `HookError` to stop the attempt.

The event contains the operation and optional parent operation IDs, stable
endpoint ID, application method, credential-sanitized logical URL, zero-based
page index, `AttemptInfo`, sanitized public headers, body category, and known
body length when available. `AttemptInfo::index` identifies each visible
attempt, `policy_retry_index` identifies managed policy retries, and
`authentication_recovery` marks the one authentication-recovery replay. Before
managed policy retry is configured, an initial attempt is `{0, 0, false}` and
authentication recovery is `{1, 0, true}`.

Use it for admission policy, correlation, and structural telemetry. It cannot
inspect a request body value or authenticated native request. Public URLs and
headers may still contain business identifiers or personal data, so logging
policy remains the application's responsibility.

## Response-head policy

`on_response_head` runs for every final response head after the core-selected
limiter has observed it and before authentication challenge, status
classification, or response-body handling. It receives safe operation
identity, method, sanitized URL and headers, page index, the same
`AttemptInfo`, and final status.

Returning `HookError` rejects the operation. Because this callback precedes
status and body work, its failure takes precedence over a received non-success
status or decoder result. It does not gain a native response or body stream.

Both result-determining callbacks run under the remaining operation deadline.
A timeout becomes the fixed timeout result. Synchronous work performed before
returning their futures cannot be preempted, so keep that work small.

## Completion observes a fixed result

`on_complete` runs exactly once for each direct request or pagination page
after Concord has fixed success or failure. It receives safe identity and
timing data, final status when available, `attempts_started`,
`policy_retries`, whether authentication recovered, and a structural
`CompletionOutcome`. It cannot replace the result. One ordinary result reports
one attempt and zero policy retries; authentication recovery reports two
attempts and zero policy retries.

Completion is outside the operation deadline and is awaited before that fixed
result is delivered. It is not detached background telemetry. A completion
future that remains pending can delay the caller indefinitely; add an
application-owned timeout, bounded queue, or loss policy when exporting
telemetry.

`on_pagination_complete` similarly observes the fixed aggregate result with
page and item counts plus the checked sums of page-level attempt and policy
retry totals after page-level completions. Use the parent operation ID to
correlate page attempts with the aggregate without placing raw cursor or
credential values into metrics.

Cancellation drops in-progress hook futures and prevents later Concord work,
but cannot roll back effects the hook already completed or tasks it spawned.
Custom hooks own their spawned-task, retry, I/O, CPU, memory, and metric
cardinality bounds.

## Built-in structural debugging

`DebugLevel::Basic` writes body-free structural request and response events to
standard error. `DebugLevel::Headers` additionally includes sanitized
headers. `DebugLevel::None` is the default. Built-in debugging follows the
same redaction boundary, but it does not decide whether public identifiers are
appropriate for a production log sink.

Hooks are observation and policy boundaries, not middleware with native
request mutation. Put stable public request shape in the declaration and
transport configuration on the HTTP builder.

## Related

- [The managed lifecycle](../concepts/managed-lifecycle.md)
- [Security and redaction](../concepts/security-and-redaction.md)
- [Error reference](../reference/errors.md)
- [Finite behavior](../concepts/finite-behavior.md)
- [Hooks, trusted work, and bounds](../extending/hooks-and-trusted-work.md)
