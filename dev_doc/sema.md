# Semantic analysis

Semantic analysis turns the raw AST into the resolved model consumed by codegen. It is the main boundary between user syntax and generated Rust.

## Responsibilities

Sema resolves:

- client variables and auth variables
- secrets and credential declarations
- credential material shapes
- endpoint and scope arguments
- route atoms in `host`, `path`, and `fmt[...]`
- query and header values
- retry, cache, rate-limit, and behavior profile references
- pagination controller fields
- auth endpoint references
- endpoint aliases and facade names

## Profiles and inheritance

Retry, cache, rate-limit, and behavior profiles can use `extends`. Sema resolves parents before children, detects self-extension and cycles, and reports unknown parent names.

Unknown profile references are semantic errors. Keep messages explicit, for example `unknown retry profile`, `unknown cache profile`, or `unknown rate_limit profile`.

## Policy inheritance

Resolved endpoint policy follows this order:

```text
client defaults
-> outer scopes
-> inner scopes
-> endpoint
```

At each attachment site, behavior is applied before explicit local clauses. That lets local clauses override or refine behavior-provided policy.

## Merge rules

- Explicit `retry` and `cache` override behavior-provided `retry` and `cache` at the same attachment site.
- Rate-limit profiles combine.
- `rate_limit off` clears inherited rate-limit policy.
- `only` replaces the local inherited profile set where applicable.
- Auth uses append in inherited/source order.
- Query and header operations preserve order after resolution.

Cache sizing units are resolved in sema. The parser records `capacity N entries`, `max_body N unit`, and `shared`; sema validates positive values, converts max body units to bytes, and stores numeric capacity/max-body/shared fields in the resolved cache config.

## Behavior expansion

Behavior profiles are resolved in sema and lowered into normal auth/cache/retry/rate-limit policy data. Behavior is not emitted as a runtime concept.

Behavior rate-limit specs are intentionally resolved at the attachment site, not at declaration time. This is required because `rate_limit key name = arg` bindings are contextual and may be visible only at a scope or endpoint.

Behavior use order is:

```text
client default behavior names
-> outer scope behavior names
-> inner scope behavior names
-> endpoint behavior names
```

Behavior rustdoc labels are deduped in stable first-seen order. Deduping affects documentation metadata only, not policy semantics.
