# Per-host and per-account rate limits

## Goal

Compose a client-wide rule with per-account and endpoint-local rules, include
one composite host/region/account key, and deliberately share limiter state
between built clients when the deployment needs coordinated local quota.

## Complete code

This compiled composition contains client, scope, endpoint, and composite
partitioning, shared limiter state, and a finite-capacity case:

```rust,ignore
{{#include ../../crates/concord-examples/src/rate_limiting/partitioning.rs:rate_partitioning_recipe}}
```

All generated declarations and requests are imported by the construction
test; local runtime tests exercise shared state and recoverable capacity
pressure.

## Why these declarations

The client rule uses `by [endpoint]`, so each stable endpoint contract ID has
separate state while the rule applies throughout the client.

The `accounts` scope adds `by [scope.account]`. Every child request for the
same account shares that scope rule even when its endpoint differs. The
`channel` endpoint then adds `by [ep.channel]`, separating its own local
window by required channel.

The `composite` endpoint demonstrates ordered dimensions:

```text
host -> scope.region -> ep.account
```

`host` is the final shaped host, not the fixed base text. Dimension order is
part of the quota key. `cost 2` charges two tokens to every window in that
rule.

Applicable rules are additive. A composite request must receive authority
from the client rule, account rule, and endpoint rule before transport.
Concord does not infer account identity from a path or header; every
partition is explicit.

The shared-limiter region passes the same object to two independently built
clients. Ordinary clones already share the original client's limiter, so an
explicit `Arc` is needed only when separate builds should coordinate.

## Adaptations

Omit `by` for one bucket shared by every request to which that declaration
applies. Use `by [host]` for remote policy scoped to the final routing host or
`by [endpoint]` for endpoint-contract isolation.

Use only required or defaulted scalar value dimensions with stable canonical
`Display`. Optional values, vectors, secrets, credentials, and formatted
expressions are not valid dimensions. Avoid including a dimension merely
because it is available; cardinality consumes finite local state.

Share a limiter across processes only through an implementation designed for
distributed coordination. An in-process `Arc<LocalRateLimiter>` cannot
coordinate separate processes or machines.

If unrelated generated client contracts share one limiter, verify their
declaration rule namespaces are compatible. Concord does not rewrite IDs into
a global namespace.

## Production notes

The default `LocalRateLimiter` is a continuously refilling token bucket. It
provides bounded local pacing, not an exact emulation of every remote fixed or
sliding window.

Handle `RateLimitErrorKind::StateCapacity` as recoverable pressure. Existing
keys remain operational, but immediate retries for a new high-cardinality key
may still fail. Reduce dimensions, select an evidence-based finite capacity,
use bounded backoff, or install external policy.

Keep `RetryAfterObserver` behavior when a custom local observer should still
honor bounded final-429 cooldowns. Limiter acquisition and response
observation are correctness work under the operation deadline; custom futures
and state must remain finite.

Pagination-controlled values cannot be rate dimensions, and host partitioning
cannot depend on pagination-controlled host shaping. Design the quota key to
remain coherent across a page sequence.

## Related

- [Rate limiting](../features/rate-limiting.md)
- [Pagination](../features/pagination.md)
- [Regional APIs](regional-apis.md)
- [DSL rate-limit reference](../reference/dsl.md#rate-limits)
- [Limits reference](../reference/limits.md)
