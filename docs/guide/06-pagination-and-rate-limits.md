# 6. Pagination and rate limits

Pagination is a capability of a bodyless, buffered page response:

```rust
GET list(offset: u64) -> Json<Page<Item>> {
    paginate OffsetPagination {
        offset: bind ep.offset,
    }
}
```

The controller type is ordinary Rust. Built-in offset, page-number, cursor,
and next-cursor controllers are reexported by the façade, but the declaration
compiler does not recognize their names. The generated named request gains
`.paginate(PageLimit)`.

Pagination is sequential, finitely bounded, identity-cycle detecting, and
all-or-error. Every page is a fresh request through the same hooks,
authentication, limiter, status, acquisition, and completion protocol. No
partial result is returned under the existing terminal.

The retained pagination-identity byte limit charges the complete canonical
encoded identity vector for every page kept by cycle detection. The separate
runtime page ceiling bounds the number of retained identities; allocator and
container overhead is not included in the byte total. Construction withholds
the first encoded byte that would exceed the configured identity limit.

## Applicability and bucket partitioning

A rule's lexical location determines where it applies. Its optional `by`
clause determines how applicable requests are partitioned into local buckets.

For example, omission means one declaration-global bucket shared by every
request to which that declaration applies:

```rust
rate_limit {
    1_000 / 1m;
}
```

This is declaration-global, not process-global: a different source declaration
has a different deterministic declaration rule ID within its generated client
contract. Client, outer-scope, inner-scope, and endpoint rules are additive in
that order. Descendants inherit applicable ancestor rules, but each rule
retains the dimensions resolved where it was declared.

Add only the dimensions that define the intended partition:

```rust
// The same client declaration has one bucket per endpoint contract ID.
rate_limit {
    by [endpoint];
    100 / 1m;
}

// Requests are separated by the final logical host after host shaping.
rate_limit {
    by [host];
    100 / 1m;
}

// A scope declaration has one bucket per required/defaulted account.
rate_limit {
    by [scope.account];
    100 / 1m;
}

// An endpoint declaration has one bucket per required/defaulted channel.
rate_limit {
    by [ep.channel];
    20 / 1s;
}

// Dimension order is part of the key.
rate_limit {
    by [host, scope.region, ep.account];
    cost 2;
    100 / 1m;
}
```

Concord does not infer `method`, path, query, URL, or other dimensions.
Supported value dimensions must be required or defaulted scalars with stable,
canonical `Display` output. Optional values, vectors, formatted expressions,
secrets, and credentials are rejected. Pagination-controlled values are also
rejected across the final effective plan: this includes inherited or local
`vars.*`, `scope.*`, and `ep.*` dimensions. A `host` dimension is rejected
when final host shaping recursively reads a pagination-controlled value.
Unrelated pagination and static or non-pagination host dependencies remain
valid. Only referenced value types gain a `Display` bound.

A resolved bucket is identified by the selected limiter object, the
deterministic declaration rule ID, the ordered resolved dimensions, and the
window. `host` is the final logical host; `endpoint` is the stable endpoint
contract ID. Resolved values are bounded operational identifiers supplied
only to the selected limiter. Concord does not include them in errors,
`Debug`, `Display`, hooks, completion, endpoint descriptions, generated
Rustdoc, or safe plans.

Declaration IDs are deterministic within one generated client contract.
Renaming a source client/scope/endpoint identity changes the derived ID, and
the ID is not globally unique across unrelated `api!` expansions. Default
client builds receive separate limiter objects. Sharing one limiter across
instances of the same generated client intentionally shares matching rule
state. Sharing one limiter across unrelated generated client contracts
requires compatible rule namespaces; Concord adds no global namespace or
runtime rewriting.

## Default local policy

With the default façade features, each built client receives a
`LocalRateLimiter` from `concord-extras`. It is a continuously refilling token
bucket for every resolved rule/window, not an exact emulation of arbitrary
remote fixed- or sliding-window servers. A rule's cost is charged to every one
of its windows, and admission waits for the longest applicable deficit without
partially reserving the others.

The limiter is process-local and limiter-object-local. Client clones share the
built client's limiter object. Independently built clients receive independent
objects unless the application supplies the same `Arc<dyn RateLimiter>` to
both:

```rust
let limiter: std::sync::Arc<dyn concord::rate_limit::RateLimiter> =
    std::sync::Arc::new(concord::rate_limit::LocalRateLimiter::new());

let first = ExampleApi::builder()
    .configure_runtime({
        let limiter = std::sync::Arc::clone(&limiter);
        move |runtime| runtime.rate_limiter(limiter)
    })
    .build()?;

let second = ExampleApi::builder()
    .configure_runtime(move |runtime| runtime.rate_limiter(limiter))
    .build()?;
```

Quota and cooldown maps are finite. When a request needs new state that does
not fit, acquisition fails before transport with recoverable
`RateLimitErrorKind::StateCapacity`. Existing buckets remain operational; the
limiter is not poisoned. Quota entries are eligible for pruning only when
fully replenished and idle for the configured TTL. Expired cooldowns may be
pruned, while active quota history and unexpired cooldowns are never evicted.
Applications can reduce key cardinality, select a larger finite capacity, wait
for safely prunable state, or install a custom distributed limiter.

```rust
if error.rate_limit_kind()
    == Some(concord::rate_limit::RateLimitErrorKind::StateCapacity)
{
    // Apply bounded backoff or capacity/cardinality policy. Existing keys
    // remain usable; an immediate retry of the new key may still be full.
}
```

`RetryAfterObserver` is the built-in final-429 observer. Calling
`LocalRateLimiter::observer(...)` replaces it. A custom observer must delegate
to `RetryAfterObserver` if built-in bounded `Retry-After` handling should
remain.

Without a configured or feature-provided limiter, a non-empty plan fails
closed during client construction, before a request lifecycle can begin. An
explicit user-supplied limiter always wins.

Limiters and observers receive bounded semantic plans and sanitized
response-head context—not native requests, responses, or credentials. Key
preparation and server-directed cooldowns are bounded. Exact syntax is in the
[DSL reference](../reference/dsl.md#rate-limits); defaults are in
[Limits](../reference/limits.md).

Limiter acquisition and response observation are correctness authority. After
each native response, Concord sanitizes the head, awaits limiter observation
under the remaining operation deadline, and only then runs the user
response-head hook. A limiter error or timeout prevents that hook and any
challenge, status, or body handling. Disabling the operation timeout therefore
trusts a custom limiter future not to remain pending indefinitely; custom
limiter state and synchronous work are also application-owned bounds.

`RateLimitPermit` is a zero-sized proof that one visible attempt was admitted.
It is not an in-flight concurrency lease: Concord does not retain or drop it to
signal request completion. A limiter that enforces concurrency must own that
state and its release protocol itself.

## Running proof

The importable
[pagination modules](../../crates/concord-examples/src/pagination/mod.rs) and
[rate-limit modules](../../crates/concord-examples/src/rate_limiting/mod.rs)
cover global, host, endpoint, scope-value, endpoint-value, composite, shared,
capacity-pressure, observer, and custom-limiter contracts. The
[local runtime test](../../crates/concord-examples/tests/local_runtime.rs)
executes deterministic pages and limits; the
[construction test](../../crates/concord-examples/tests/construction.rs)
checks the generated capability surface.

Next: [Explicit and runtime endpoints](07-explicit-and-runtime-endpoints.md).
