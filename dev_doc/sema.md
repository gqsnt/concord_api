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

## Public expression closure

Public request-shaping expressions are closed during sema. Header/query policy
values, timeout expressions, route atoms, public `fmt[...]` pieces, and
pagination assignment values must not resolve through auth material, secret
variables, or generated implementation locals.

Route atoms split into trusted string literals and dynamic data: static path
string atoms stay as raw route fragments, while dynamic path pieces and
`fmt[...]` path pieces are checked as single encoded segments and reject `/`,
`\`, `.` and `..`. Dynamic host pieces are checked as host labels, not as
full URL hosts.

The parser may keep raw Rust expressions so diagnostics can point at the user
token, but sema must reject forbidden roots before resolved IR reaches codegen.
Forbidden roots include `auth`, `secret`, `secrets`, `ctx`, `cx`, `ep`, `vars`,
`client`, `runtime`, `policy`, `req`, `request`, `headers`, `url`, `cache`,
`transport`, and `self` when they appear inside arbitrary public expressions.
Raw identifiers are normalized for this check, so `r#auth` and `r#secret` are
equivalent to `auth` and `secret`.
Direct safe public references, such as endpoint arguments and declared client
variables in supported value positions, are represented as resolved value IR
rather than relying on user access to generated locals.

The validator also scans macro token streams recursively because `syn::Visit`
does not interpret every token inside a macro body. Secret exposure methods
such as `.expose()` and `.expose_secret()` are rejected in these public
contexts. Auth credential declarations remain the only DSL surface that may
refer to `secret.*`.

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
- `retry off` and `cache off` clear inherited policy.
- Rate-limit profiles combine.
- `rate_limit off` clears inherited rate-limit policy.
- `only` replaces the local inherited profile set where applicable.
- Auth uses append in inherited/source order.
- Query and header operations preserve order after resolution.

Cache sizing units are resolved in sema. The parser records `capacity N entries`, `max_body N unit`, and `shared`; sema validates positive values, converts max body units to bytes, and stores numeric capacity/max-body/shared fields in the resolved cache config.

## Behavior expansion

Behavior profiles are resolved in sema and lowered into normal auth/cache/retry/rate-limit policy data. Behavior is not emitted as a runtime concept.

Behavior rate-limit specs are intentionally resolved at the attachment site, not at declaration time. This is required because `rate_limit key name = arg` bindings are contextual and may be visible only at a scope or endpoint.

Sema rejects duplicate behavior names across multiple `behavior` clauses at one attachment site: one client defaults block, one scope body, or one endpoint body. The parser already rejects duplicates inside a single `behavior [...]` list. Cross-layer reuse remains valid. Behavior clauses at one site apply in source order, and behavior names are preserved only for rustdoc labels.

Behavior use order is:

```text
client default behavior names
-> outer scope behavior names
-> inner scope behavior names
-> endpoint behavior names
```

Behavior rustdoc labels are deduped in stable first-seen order. Deduping affects documentation metadata only, not policy semantics.
