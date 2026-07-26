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

Rate-limit declarations form an ordered lexical plan:

```rust
rate_limit {
    cost 2;
    100 / 1m;
    1_000 / 1h;
}
```

Client, outer-scope, inner-scope, and endpoint rules are additive. With the
default façade features, Concord installs the downstream local limiter
implementation. Without a configured/default limiter, a non-empty plan fails
closed during client construction, before a request lifecycle can begin. A
user-supplied limiter always wins.

Limiters and observers receive safe semantic plans, keys, and response-head
context—not native requests, responses, or credentials. Key preparation and
server-directed cooldowns are bounded. Exact syntax is in the
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
cover built-in and custom contracts. The
[local runtime test](../../crates/concord-examples/tests/local_runtime.rs)
executes deterministic pages and limits; the
[construction test](../../crates/concord-examples/tests/construction.rs)
checks the generated capability surface.

Next: [Explicit and runtime endpoints](07-explicit-and-runtime-endpoints.md).
