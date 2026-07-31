# Modeling APIs

- **Audience:** API authors and reviewers
- **Purpose:** guide
- **Normative reference:** [DSL specification](spec.md)

## Start from the domain, not from URL strings

Identify stable ownership and relationships first:

```text
client
├── platform(platform_route)
│   ├── summoner_v4
│   └── league_v4
└── regional(regional_route)
    ├── account_v1
    └── match_v5
```

Then state how each node contributes to the host, path, public query or headers,
authentication, timeout, and rate policy. Do not create a scope for every path
segment. Create one when it represents meaningful domain ownership, selected
context, shared policy, or generated navigation.

## Put values at their narrowest stable owner

- Client values apply to the whole built client.
- Scope values select a reusable domain context for descendants.
- Endpoint values belong to one operation.
- Request-level controls override only the selected request.

Moving a value higher than necessary pollutes unrelated operations. Moving a
shared value down repeats arguments and wire mapping.

## Make signatures domain-first

Prefer domain names and types:

```text
challenge_id: ChallengeId
level: ChallengeLevel
count: NonZeroU32
```

over transport-shaped primitives such as unrelated `i64`, `String`, or magic
sentinels. Concord treats Rust types as opaque and lets Rust enforce their
semantics through normal type checking and traits.

The signature should answer what the operation means. Path/query/header/body
clauses should answer how it maps to HTTP.

## Declare once, attach where used

Use separate declaration and attachment when binding environments differ:

- secrets and credential sources are declared at the client;
- authentication placement attaches at a client, scope, or endpoint;
- reusable rate profiles declare formal dimensions;
- `rate_limit use` binds them to values visible at the attachment site;
- pagination controllers bind live fields at the endpoint.

Avoid lexical capture in reusable policy. Explicit formals make dependencies,
compatibility, and generated trait obligations reviewable.

## Attach shared behavior at the narrowest common ancestor

If every endpoint under `match_v5` shares a path prefix, timeout, authentication
placement, or rate policy, attach it there. If one endpoint differs, state the
local delta with the family-specific merge operation.

Each family has its own algebra because its safe meaning differs:

| Family | Operations |
|---|---|
| host | replace or off |
| path | append, replace, or off |
| query and public headers | modify, replace, remove, or off |
| authentication | ordered append, replace, or off |
| timeout | set, off, or inherit by omission |
| rate limits | additive |

## Review the generated usage before freezing the declaration

Ask:

- Does autocomplete follow the domain?
- Are required values arguments at the right level?
- Are optional/defaulted controls understandable?
- Does the endpoint name describe a domain action rather than a route string?
- Are invalid terminals absent?
- Can an explicit endpoint be constructed and planned without surprising
  hidden inputs?

The generated API is part of the design, not an implementation afterthought.

## Identity

Derived endpoint IDs are convenient but change when a source component is
renamed. Use explicit IDs when metrics, completion events, policy, or external
tooling require rename-stable identity.

## Local endpoint-authoring checklist

When adding an endpoint, determine:

1. Correct owner scope.
2. Domain action name.
3. Required, optional, and defaulted typed values.
4. Body role and response specification.
5. Local path/query/header mapping.
6. Exceptional auth, timeout, or rate policy.
7. Pagination bindings when relevant.
8. Explicit ID when rename stability matters.

The author normally should not implement route propagation, request types,
encoding, limiter/retry loops, response acquisition, cancellation, or
completion.

## Anti-patterns

- One flat scope containing unrelated domains.
- Scopes used only to mirror every literal path segment.
- Long endpoint names that re-encode the entire parent tree.
- Primitive identifiers where a domain newtype is available.
- Repeated auth, route, or rate clauses that belong on an ancestor.
- Runtime deployment choices encoded in the contract.
- Generic callbacks that hide value dependencies from the compiler.

## Limits to locality

A local endpoint still depends on inherited semantics. Review the parent when
using `replace` or `off`, attaching a profile, changing credential ownership,
or binding pagination values that also influence host or rate policy. Concord
makes global context small, named, inspectable, and centrally validated; it
does not make it irrelevant.
