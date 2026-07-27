# Requests, planning, and errors

Construct a named request, adjust its endpoint-owned values, and await it when
the application is ready to perform HTTP. Use an explicit endpoint and a safe
plan when code needs to inspect or store an operation before execution.

## Construct without side effects

Both direct navigation and explicit endpoint binding produce the same request
type:

## Complete example

```rust,ignore
{{#include ../../crates/concord-examples/src/contracts/full_dsl.rs:direct_and_explicit}}
```

These statements do not open a connection. They retain bounded public values
and construct immutable snapshots. Consuming controls such as
`set_language`, `clear_scope_label`, and `reset_scope_label` return a new
snapshot, which makes a prepared request safe to move between tasks according
to its ordinary Rust bounds.

Awaiting the request starts execution:

```rust
let text = request.await?;
```

The ordinary terminal returns the declared application value. If the endpoint
supports typed response metadata, select it before awaiting:

```rust
let response = client.status().response().await?;
println!("status: {}", response.status());
let bytes = response.into_value();
```

`ApiResponse<T>` also exposes the accepted HTTP version, actual accepted
headers, credential-sanitized logical URL, and safe execution metadata.
Those public URL values and headers may still contain identifiers chosen by
your application, so decide separately what is appropriate to log.

For complete bounded bytes without validation or decoding, select
`response_bytes()`:

```rust
let response: concord::ApiResponse<concord::Bytes> =
    client.status().response_bytes().await?;
let bytes = response.into_value();
```

This terminal preserves the response specification's declared status
acceptance and `Accept` header. It is available for buffered and discard
declarations, including runtime endpoints, but not streaming modes. Actual
headers and returned bytes are explicitly accessed application data;
`ApiResponse` formatting omits their values.

## Plan an explicit endpoint

Planning answers “what public operation would this become?” without executing
it:

```rust
let endpoint = reference_api::regions::Fetch::new(
    "eu-west".to_owned(),
    "42".to_owned(),
    vec!["stable".to_owned()],
);
let plan = client.plan(&endpoint)?;
```

A plan contains the safe method, endpoint ID, credential-sanitized URL,
public headers, timeout selection, and declared capabilities. It performs the
same bounded public-head preparation used by execution and may return a
configuration error when shaping fails.

It does not fetch or place credentials, acquire rate authority, run hooks,
materialize a body, poll a stream, perform transport, or return a native
request. Use plans for diagnostics and assertions about public shape. Use a
local HTTP test when behavior across the actual lifecycle matters.

## Handle failures by category

Execution returns `ApiError`. Its stable, non-exhaustive `ErrorCategory`
separates configuration, request encoding/body, authentication, rate-limit,
hook, timeout, connection, transport, HTTP status, response body/decoding,
response contract, pagination, and internal failures.

Match a category or focused typed kind only when application behavior changes:

```rust
match client.status().await {
    Ok(bytes) => use_bytes(bytes),
    Err(error) if error.category() == concord::error::ErrorCategory::Timeout => {
        schedule_retry();
    }
    Err(error) => return Err(error),
}
```

Do not parse formatted error text. `ApiError` keeps endpoint context safe and
does not expose an authenticated URL, native request/response, credential
material, or unbounded response body.

Statuses rejected by the declared response specification use the HTTP-status
category. When available, the typed status failure exposes the status,
sanitized headers, bounded status body state, and parsed `Retry-After`
metadata. Redirects are not followed by default, so a received 3xx is final;
it becomes an accepted response only when the response specification declares
that acceptance.

## Choose explicit endpoints deliberately

Use an explicit endpoint when you need to:

- inspect a plan before binding or execution;
- choose among compatible clients later;
- store or pass an unbound operation;
- write bounded generic composition against Concord's sealed endpoint trait.

Use direct generated methods for ordinary service calls. Both paths share the
same endpoint identity, request controls, error model, hooks, authentication,
rate limiting, and completion behavior.

Completion observation runs once after Concord has fixed the result. It can
record the safe outcome but cannot change it. The next page explains where
that observation sits in the complete lifecycle.

## Related

- [The managed lifecycle](../concepts/managed-lifecycle.md)
- [Build and share clients](client-construction.md)
- [Error reference](../reference/errors.md)
- [Generated request controls and terminals](../reference/generated-api.md#request-controls-and-terminals)
- [Security and redaction](../concepts/security-and-redaction.md)
