# Policies And Behaviors

Concord separates declarations from attachments.

Declarations define reusable profiles:

- retry profiles
- rate-limit profiles
- behavior profiles

Attachments apply those profiles through `defaults { ... }`, scopes, or endpoints. `policies { ... }` is for policy declarations and observers, not default attachments.

## Policy Profiles

Retry profiles describe retry attempts, methods, status codes, transport errors, `Retry-After` behavior, and idempotency header behavior.

Rate-limit profiles describe bucket sets, keys, costs, and windows. Rate-limit observers translate response headers into runtime observations.

## Defaults And Narrower Layers

`defaults { ... }` applies client-wide attachments. Scopes add inherited attachments for nested items. Endpoints apply last.

The merge order is:

```text
client defaults
-> outer scopes
-> inner scopes
-> endpoint
```

Behavior clauses at one site apply in source order. The same behavior name may be reused across different layers, but sema rejects attaching it more than once at the same defaults, scope, or endpoint site.

## Behaviors

Behavior profiles are semantic labels for repeated auth, retry, and rate-limit combinations. They can inherit with `extends`.

Behavior merge rules:

- auth uses append in source order
- child retry replaces parent retry when present
- `retry off` clears inherited policy
- behavior rate-limit specs append and resolve at attachment site
- explicit local retry overrides behavior retry at that site
- explicit local rate-limit combines with behavior rate-limit
- `rate_limit off` clears inherited rate-limit policy

Parser diagnostics reject duplicate names inside one `behavior [...]` list. Sema also rejects attaching the same behavior more than once at one defaults, scope, or endpoint site, while still allowing the same behavior to be reused across different layers.

Same-layer auth header and query declarations are non-lossy. Distinct inline and block declarations merge in declaration order. Duplicate auth headers in the same layer are rejected with case-insensitive header-name comparison. Duplicate auth query parameters in the same layer are rejected by exact query key. Cross-layer inheritance is separate and is not collapsed by these same-layer checks.

An empty inline `rate_limit {}` block is rejected as a no-effect declaration. Clearing inherited rate-limit policy must use `rate_limit off`.

Behavior names are preserved as rustdoc labels. Labels are deduped in stable first-seen order so repeated attachments do not make docs noisy.

Behavior is not a runtime concept. By the time codegen builds request plans, behavior semantics have been lowered into ordinary policy and auth data.
