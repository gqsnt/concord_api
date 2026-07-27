# Rate limiters and observers

Implement `RateLimiter` when admission or quota state belongs to application
infrastructure rather than the first-party process-local token bucket.
Implement `RateLimitObserver` when a limiter needs service-specific,
response-head-driven cooldown policy.

The exact Rustdoc items are `concord::rate_limit::RateLimiter` and
`concord::rate_limit::RateLimitObserver`.

## Complete limiter

This compiler-owned limiter grants every attempt while recording a bounded
counter:

```rust,ignore
{{#include ../../crates/concord-examples/src/rate_limiting/custom_limiter.rs:custom_limiter_extension}}
```

It is intentionally not production quota policy. It demonstrates the complete
trait shape and is exercised by a deterministic test that proves transport
cannot begin before its permit.

Install a selected object on the client runtime:

```rust,ignore
{{#include ../../crates/concord-examples/src/rate_limiting/custom_limiter.rs:custom_limiter_install}}
```

Pass that function through the generated builder's `configure_runtime`
closure. Clones of the resulting client share the object. Give the same `Arc`
to independently built clients only when their generated rule namespaces and
quota ownership are intentionally compatible.

Generated rules may declare an exact explicit ID. Reusing one limiter then
shares state for matching ID, ordered key, and window even across separate
generated contracts. Concord validates compatible dimensions and windows
within one `api!` invocation, but the application owns that check across
invocations. Different rule costs are compatible and consume different token
amounts from the same bucket.

## Admission input

`RateLimiter::acquire` receives a borrowed `RateLimitRequest` for one visible
Concord attempt. It contains:

- stable endpoint ID and HTTP method;
- credential-sanitized logical URL and final logical host;
- zero-based page index;
- `AttemptInfo` with the visible-attempt index, policy-retry index, and
  authentication-recovery flag;
- the immutable effective `RateLimitPlan`.

The initial attempt is `{ index: 0, policy_retry_index: 0,
authentication_recovery: false }`. Policy retries increment both indices.
Authentication recovery increments the global index, keeps the current policy
retry index, and sets its flag. Use this bounded structure for correlation
instead of inferring retry state from call counts.

The plan contains applicable rules in declaration order. Each rule has its
stable ID—exactly an explicit declared value when present—ordered resolved key
parts, positive cost, and finite windows.
Dimension order is part of the key. Treat host and key values as potentially
sensitive even though credentials have been removed.

Inspect the public model through Rustdoc at
`concord::rate_limit::RateLimitPlan`,
`RateLimitRule`, `RateLimitKey`, `RateLimitKeyPart`,
`RateLimitWindow`, and `RateLimitId`. Do not parse `Debug` output or reconstruct
identity from the URL.

Return `RateLimitPermit::granted()` only after all applicable policy has
authorized the attempt. The permit is proof of admission, not a concurrency
lease held until completion. If concurrency limits are required, model and
release them in an explicitly owned layer rather than assuming permit drop
signals request completion.

`preparation_key_limit` advertises the maximum aggregate resolved-key bytes the
limiter accepts. Concord snapshots it with the selected object at client build
and validates plans before credentials, body preparation, acquisition, or
transport. Return zero only for a policy that permits global keys and rejects
all keyed rules.

## Asynchronous observation

`RateLimiter::observe` receives a `RateLimitResponse` after one final response
head and before the user response hook, authentication challenge handling,
status reduction, or body acquisition. It can update distributed state using
the sanitized head, the same request plan, and the same `AttemptInfo`.

Observation cannot replace the received response. Returning an error or
remaining pending until the operation deadline does determine the operation,
so keep the path available and finite. A remote quota store outage needs an
explicit fail-open or fail-closed policy expressed by the implementation.

Acquisition and observation futures run under the remaining operation
deadline and are dropped on cancellation. Reservations or external writes
already completed are not rolled back automatically.

## Response observers

The first-party `LocalRateLimiter` can translate a sanitized response head into
future cooldown state through `RateLimitObserver`:

```rust,ignore
{{#include ../../crates/concord-examples/src/rate_limiting/observer.rs:custom_observer_extension}}
```

An observer is synchronous. It returns either no change or a `Cooldown` with a
duration and `CooldownScope` for current buckets, the current host, or the
limiter object. Runtime configuration clamps the proposed duration.
`RateLimitResponseHead` also carries the page index and `AttemptInfo` that
produced the observed response.

Calling `LocalRateLimiter::observer` replaces its built-in
`RetryAfterObserver`. Delegate to `RetryAfterObserver` first when standard
bounded `Retry-After` behavior should remain, as the example does:

```rust,ignore
{{#include ../../crates/concord-examples/src/rate_limiting/lexical.rs:configured_rate_limiter}}
```

Do not retain arbitrary header values or create unbounded cooldown keys. The
observer gets sanitized headers, but service-defined public headers can still
contain account or operational data.

## Trusted state and failures

Concord bounds plan structure and resolved key bytes. It does not bound a
custom limiter's maps, queues, tasks, retries, database rows, metric labels, or
synchronous work performed before a future is returned. Apply explicit
capacity, eviction, TTL, and overload behavior. Preserve existing keys when
admitting a new key would exceed capacity; return recoverable
`RateLimitErrorKind::StateCapacity` when that is the chosen contract.

Calls overlap. Avoid holding a process lock across `.await`, and make
distributed updates atomic at the policy level. Decide how clock skew,
duplicate requests, cancellation, and partial remote failures affect quota
state.

Use `RateLimitError::new` with the narrowest stable
`RateLimitErrorKind`. `with_safe_message` is a trust boundary: never include
resolved keys, credentials, native URLs, headers, database diagnostics, or
response bodies.

An explicit `NoopRateLimiter` authorizes every plan. It is a deliberate policy
choice, not the same as lacking a valid limiter while nonempty rules exist.

## Verification

Test through generated requests and the actual managed ordering. Verify:

- transport cannot begin before acquisition succeeds;
- applicable rules, ordered dimensions, cost, and windows arrive unchanged;
- direct and paginated page indices;
- exact attempt identity on acquisition and response observation, including
  both authentication attempts;
- concurrent calls and atomic capacity behavior;
- exact key limit and first excess before protected work;
- acquisition and observation timeout/cancellation;
- response observation before user hooks and body polling;
- observer replacement and baseline delegation;
- shared versus independently built client state;
- error formatting without key, host, header, or backend sentinels.

Use a deterministic limiter and local HTTP support. Wall-clock sleeps and live
quota services make poor contract tests.

## Related

- [Rate limiting](../features/rate-limiting.md)
- [Per-host and per-account rate limits](../recipes/rate-limit-partitioning.md)
- [Pagination controllers](pagination.md)
- [Hooks, trusted work, and bounds](hooks-and-trusted-work.md)
- [Limits reference](../reference/limits.md)
- Rustdoc: `concord::rate_limit::RateLimiter` and
  `concord::rate_limit::RateLimitObserver`
