# Contract model

Concord has one product transformation:

```text
remote HTTP API contract
    -> declaration compiler
    -> ordinary predictable typed Rust
    -> one managed HTTP lifecycle
```

The declaration contains representation: stable URL structure, public values,
credential placement, media specifications, pagination bindings, lexical rate
limits, and endpoint identity. The generated Rust surface contains named
clients, builders, scopes, endpoint values, and request wrappers. The runtime
contains operational policy and effects.

Keeping those layers separate matters. A concrete rate limiter, hook object,
transport retry policy, proxy, or observer is deployment policy and belongs on
the runtime builder. A request path, response representation, or
authentication placement is remote-contract policy and belongs in the
declaration.

Within a rate-limit declaration, lexical placement selects applicability and
an optional ordered `by` clause selects bucket partitioning. Omission means
one bucket for that declaration, not a process-global bucket. The compiler
does not infer method, path, query, or URL dimensions.

The resolved compiler model is the single source of generated names and
capabilities. Direct methods and explicit endpoint values bind that same
model. Safe planning and execution share public-head preparation. This avoids
parallel interpretations that could drift.

The public extension boundary is semantic: bodies, responses, providers,
pagination controllers, limiters, and observers describe their work without
receiving native transport values. Concrete first-party integrations use the
same boundary available to downstream applications.

Read [Managed lifecycle](managed-lifecycle.md) for execution ordering and the
[DSL reference](../reference/dsl.md) for exact representation syntax.
