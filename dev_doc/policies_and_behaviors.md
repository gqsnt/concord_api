# Policies and behaviors

Concord separates declarations from attachments.

Declarations define reusable profiles:

- retry profiles
- cache profiles
- rate-limit profiles
- behavior profiles

Attachments apply those profiles through `defaults { ... }`, scopes, or endpoints. `policies { ... }` is for policy/profile declarations and observers, not default attachments.

## Policy profiles

Retry profiles describe retry attempts, methods, status codes, transport errors, retry-after behavior, and idempotency header behavior.

Cache profiles describe HTTP cache mode, TTL, revalidation, and stale-on-error behavior.

Rate-limit profiles describe bucket sets, keys, costs, and windows. Rate-limit observers translate response headers into runtime observations.

## Defaults and narrower layers

`defaults { ... }` applies client-wide attachments. Scopes add inherited attachments for nested items. Endpoints apply last.

The merge order is:

```text
client defaults
-> outer scopes
-> inner scopes
-> endpoint
```

## Behaviors

Behavior profiles are semantic labels for repeated auth/cache/retry/rate-limit combinations. They can inherit with `extends`.

Behavior merge rules:

- auth uses append
- child retry/cache replace parent retry/cache when present
- behavior rate-limit specs append and resolve at attachment site
- explicit local retry/cache override behavior retry/cache at that site
- explicit local rate-limit combines with behavior rate-limit
- `rate_limit off` clears inherited rate-limit policy

Behavior names are preserved as rustdoc labels. Labels are deduped in stable first-seen order so repeated attachments do not make docs noisy.

Behavior is not a runtime concept. By the time codegen builds request plans, behavior semantics have been lowered into ordinary policy/auth data.
