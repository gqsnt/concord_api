# Requests and values

## Exact presence

Concord never infers semantics from a Rust type name. `x?: T` adds exactly one
outer `Option`; `x: Option<T>` remains a required payload; `x?: Option<T>` keeps
all three omit/clear/set states. Aliases and qualified paths behave identically.

`#[default]` and stable initializers are independent from presence. The value is
evaluated once when its owner snapshot is made and retained for reset, retries,
recovery, and pagination.

## Public shaping

Restricted public atoms are strings, supported integers, owner-qualified
references, and bounded `concat`. An optional slot can appear directly only in
omission-aware query/header assignment. Redacted dependencies redact a complete
derived atom in observed structures while preserving real wire bytes.

Subdomain labels compose in front of the fixed DNS host. Path segments append to
the fixed base path. Replace/off operate only on generated portions. Each value
is validated as one label/segment and encoded once; raw authority and encoded
segment escape hatches do not exist.

## Ordered query patch

Query assignment removes all exact-key occurrences before emitting. Append
retains prior occurrences. Remove deletes a key, replace starts empty, and off
clears the complete public query. Key comparison is case-sensitive and source
order is observable. `each` is the only collection-cardinality operator.

## Header patch

Header assignment/removal uses case-insensitive identity. Optional absence on
assignment removes inherited values. Header append is intentionally absent.
Transport/private/cookie/media-owned fields are reserved. Body and response
specifications own `Content-Type` and default `Accept` respectively.

## Timeout selection

Every root declares a duration or `timeout off`. Descendants inherit by omission
and may select their own duration/off. Request controls can explicitly override,
disable, or restore the endpoint's inherited selection. The resulting deadline
covers the complete terminal, including every pagination page.

## Planning

`request.plan()` borrows the inert request and may run bounded public formatting
and validation. It never inspects body values, resolves credentials, acquires a
limiter, invokes execution gates/observers, allocates operation identity, or
performs I/O. `observed_url()` and sanitized headers may differ from real public
wire values when redaction is present.

See the exact [DSL rules](spec.md) and [planning model](planning_and_observation.md).
