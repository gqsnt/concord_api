# Shape the public request

Build the public URL and headers from declared values rather than assembling
strings at each call site. Concord combines client, scope, and endpoint
clauses into one final public request head, validates its finite shape, and
keeps credential material out of that surface.

## Shape from broad to specific

The regional endpoint in the guide contract shows every public shaping area:

## Complete example

```rust,ignore
{{#include ../../crates/concord-examples/src/contracts/full_dsl.rs:regional_scope}}
```

Request shaping follows the declaration hierarchy:

- `host [...]` selects generated DNS labels placed before the fixed base host;
- `path [...]` appends encoded path segments by default;
- `query { ... }` adds named query values;
- `headers { ... }` adds validated public headers.

Client clauses provide common shape. A scope or endpoint can inherit, modify,
replace, disable, or remove the relevant parts according to that clause's
rules. For example, normal path clauses append, while `path replace [...]`
starts a new generated path and `path off` removes the inherited generated
path. Query and header blocks can remove individual inherited names or replace
the complete inherited collection.

Host shaping is intentionally different: a local `host [...]` replaces
inherited generated host labels. In the example, the regional scope selects
labels from `scope.region` and the literal `"api"`.

## Use public expressions

A shaping atom is a string or unsuffixed decimal integer literal, or a direct
reference to `vars.name`, `scope.name`, or `ep.name`. Use `fmt[...]` when one
host label, path segment, query value, or header value must concatenate
several public atoms:

```rust
path [fmt["resource-", ep.id]];
```

Each normal path expression becomes one percent-encoded URL segment; `/`
inside a value does not create an extra path level. Optional values omit the
corresponding operation when absent. Repeated values retain their declared
iteration order.

The restricted expression language is a feature, not a general Rust
interpreter. Calls, indexing, methods, macros, chained fields, secrets, and
credentials are rejected. Convert application data before calling the
generated endpoint when shaping needs computation outside these forms.

## Keep private data private

Public query entries and headers are available to safe planning, hooks, and
diagnostics, so they must not contain credentials. Declare authentication
material under `credentials` and place it with an `auth` clause. Concord adds
private authentication only during managed execution, after public planning
and other safe observation points.

Transport-owned headers and authentication-owned headers cannot be declared
as ordinary public headers. This prevents collisions and keeps the
credential-sanitized URL, endpoint description, errors, and hooks from
becoming accidental secret channels.

## Inspect the result safely

For an explicit endpoint value, ask the client for a request plan:

```rust
let endpoint = reference_api::Status::new();
let plan = client.plan(&endpoint)?;

assert_eq!(plan.endpoint_id(), "reference_api.status");
```

The plan reports safe method, endpoint identity, public URL shape, public
headers, timeout selection, and capabilities. It uses the real public-head
preparation path, but it does not acquire credentials, build an authenticated
native request, consume a limiter permit, run hooks, or perform HTTP.

Planning can still fail if a public value cannot be formatted or the resulting
shape exceeds a finite bound. Treat it as a diagnostic and testing tool, not
as a substitute for executing against deterministic local support.

## Know the host limitation

The fixed base may use a DNS domain, IPv4 literal, or IPv6 literal. A non-empty
`host [...]` list prepends DNS labels and therefore requires a DNS-domain
base. The compiler rejects a final host prefix over an IP literal. An IP base
remains valid when host shaping is absent or when `host off` removes inherited
labels.

The complete inheritance, removal, validation, and formatting grammar belongs
to the [DSL reference](../reference/dsl.md#public-expressions-and-request-shaping).

## Related

- [Requests, planning, and errors](requests-planning-errors.md)
- [Authentication](../features/authentication.md)
- [Complete DSL request-shaping reference](../reference/dsl.md#public-expressions-and-request-shaping)
- [Limits reference](../reference/limits.md)
- [Security and redaction](../concepts/security-and-redaction.md)
