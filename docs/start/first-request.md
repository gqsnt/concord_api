# Make the first request

Build the generated client, construct its named request, and await it from an
async function. These two regions come from the same compiled example as the
declaration.

## Build the client

```rust,ignore
{{#include ../../crates/concord-examples/src/getting_started/minimal.rs:build_client}}
```

`MinimalApi::new()` validates the default configuration and creates an
immutable runtime snapshot. It exists because this declaration has no
required client values, secrets, or providers. Declarations that require
inputs or deployment configuration use `Api::builder()` later.

## Execute the request

```rust,ignore
{{#include ../../crates/concord-examples/src/getting_started/minimal.rs:first_request}}
```

`client.health()` only constructs `minimal_api::HealthRequest`. It does not
open a socket, run hooks, acquire rate-limit authority, or materialize a body.
Awaiting the request starts the managed lifecycle. An accepted `NoContent`
response becomes `()`.

A Tokio binary can connect the pieces like this after importing
`build_client` and `execute`:

```rust
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = build_client()?;
    execute(&client).await?;
    Ok(())
}
```

The example base uses a reserved `.test` host and is not meant to resolve on
the public network. Replace it with your API's base URL before running this
binary against a service.

## This repository executes it locally

The example test builds the same generated client with an explicit loopback
proxy, queues a real HTTP `204` response, awaits `execute`, and asserts that
one `GET` reached `/v1/health`:

```rust,ignore
{{#include ../../crates/concord-examples/tests/local_runtime.rs:first_request_loopback}}
```

That test does not synthesize an application return value or contact an
external service. The HTTP exchange passes through Concord's normal transport
and response path.

## Handle failures by category

The await terminal returns `Result<_, concord::ApiError>`. `ApiError` exposes
a stable, non-exhaustive category and safe endpoint context. It does not
expose a native response or authenticated request.

```rust
match execute(&client).await {
    Ok(()) => {}
    Err(error) => {
        eprintln!("health request failed: {:?}", error.category());
        return Err(error.into());
    }
}
```

Match typed categories or focused sub-kinds when behavior differs. Do not
parse `Display` or `Debug` text as a compatibility interface. A received
non-success status is an HTTP-status category; connection, timeout,
authentication, limiter, hook, body, and decoding failures retain their own
categories.

## Related

- [Choose your next path](next.md)
- [Build and share clients](../guide/client-construction.md)
- [Requests, planning, and errors](../guide/requests-planning-errors.md)
- [Error reference](../reference/errors.md)
- [Managed lifecycle](../concepts/managed-lifecycle.md)
