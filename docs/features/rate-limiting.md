# Rate limiting

Declare remote quota rules where they apply and partition them only by stable
dimensions the service actually uses. With Concord's default features, each
built client receives a bounded process-local `LocalRateLimiter`; client
clones share it.

## Declare applicability and partitioning

This compiled contract has one client rule, one host-partitioned scope rule,
and one endpoint-value rule:

## Complete example

```rust,ignore
{{#include ../../crates/concord-examples/src/rate_limiting/lexical.rs:rate_limit_contract}}
```

Lexical placement answers “which endpoints use this rule?” A client rule
applies to every endpoint. A scope rule applies to its descendants. An
endpoint rule applies only to that endpoint. Applicable rules are additive in
client-to-endpoint order.

An optional `id "service.quota";` inside a rule gives that rule one exact
stable identity. Put it before the first window; `id`, `by`, and `cost` may
otherwise appear in any order. Omitted IDs retain their existing deterministic
source-derived form. Use an explicit ID when remote quota identity must survive
client, scope, or endpoint renames or intentionally span compatible contracts.

The optional `by [...]` list answers “which requests share quota state?”
Omitting it creates one bucket for that declaration. It does not create one
process-global bucket. Supported dimensions are:

- `host` for the final shaped host;
- `endpoint` for the stable endpoint contract ID;
- required or defaulted scalar `vars.name`, `scope.name`, or `ep.name`.

Dimension order is part of the key. Concord does not infer method, path,
query, URL, account, or other service semantics. Optional values, vectors,
formatted expressions, secrets, and credentials cannot become dimensions.
Referenced application values need stable canonical `Display` output.

## Use the default local policy

With the `rate-limit` feature, generated builders install a new
`LocalRateLimiter` unless the application explicitly configures another
limiter. It implements a continuously refilling token bucket for every
resolved rule and window. A rule's `cost` is charged to all of its windows,
and an operation waits for the longest applicable deficit before transport.

This is bounded local pacing, not an exact simulation of every remote fixed-
or sliding-window implementation. Applications needing cross-process or
service-authoritative quota coordination should install a distributed
`RateLimiter`.

An explicit limiter wins over the generated default:

```rust,ignore
{{#include ../../crates/concord-examples/src/rate_limiting/lexical.rs:configured_rate_limiter}}
```

Without either a configured limiter or the feature-provided default, a client
with a nonempty generated plan fails closed during `build()`.

## Choose sharing intentionally

Clones of one built client share its limiter object and state. Two independent
builds receive separate default objects. To coordinate separately built
clients, pass the same `Arc<dyn RateLimiter>` to each
`configure_runtime` call.

Sharing one limiter across instances of the same generated contract preserves
matching declaration rule identities. Derived rule IDs are deterministic only
within one generated client contract; unrelated `api!` expansions do not
receive a global namespace. An explicit ID is exact and can intentionally
align separate contracts, but the macro cannot compare separate `api!`
invocations. Share across contracts only when ordered dimension names,
ordered windows, and remote quota meaning are compatible. Costs may differ
because operations can consume different amounts from the same bucket.

Quota and cooldown state has finite capacity. If a new quota key cannot fit
without discarding active history, acquisition fails before transport with
`RateLimitErrorKind::StateCapacity`. A newly observed server cooldown can
report the same kind during response-head observation when its finite map is
full. Existing keys remain usable and the limiter remains operational.
Respond with bounded backoff, reduce key cardinality, choose a larger finite
configured capacity, or install external policy; an immediate retry of the
same new key may still encounter capacity.

## Observe server cooldowns

`RetryAfterObserver` interprets bounded `Retry-After` information from a final
429 response and applies a capped future cooldown. It is the default observer
for `LocalRateLimiter`.

Calling `LocalRateLimiter::observer(...)` replaces that observer. A custom
observer that should retain the built-in behavior must delegate to
`RetryAfterObserver`, as the compiled example does. Server headers are
sanitized and bounded before observation.

Limiter response observation runs after each final response head and before
the user response-head hook, authentication challenge, status handling, or
body acquisition. A limiter error or timeout therefore becomes the operation
result before those later steps. Custom limiter futures are trusted
application work and must remain finite, especially when operation timeouts
are disabled.

## Pagination restriction

A `by` value cannot be controlled by pagination, including through inherited
rules. `host` also cannot be selected when final host shaping depends on a
pagination-controlled value. Static dimensions unrelated to pagination remain
valid. This preserves one coherent quota identity across the page sequence.

Limiters receive semantic bounded plans, resolved keys, and sanitized response
heads—not native requests, responses, or credentials. Exact grammar and
ceilings stay canonical in the reference.

## Related

- [Pagination](pagination.md)
- [DSL rate-limit reference](../reference/dsl.md#rate-limits)
- [Limits reference](../reference/limits.md)
- [Error reference](../reference/errors.md)
- [Rate limiters and observers](../extending/rate-limiting.md)
