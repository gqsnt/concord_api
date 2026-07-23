# 3. Build a client and make requests

Generated clients use a named builder. `Client::new()` exists only when the
contract needs no required values, secrets, or providers; `Client::builder()`
is always the predictable construction entry point.

```rust
let client = CatalogApi::builder()
    .tenant("north".to_owned())
    .configure_runtime(|runtime| {
        runtime.operation_timeout(std::time::Duration::from_secs(20))
    })
    .build()?;
```

Required configuration missing at `.build()` produces a focused,
secret-safe `ClientBuildError`. Repeated setters are deterministic:
replacement settings use the last value, while generated rate limits remain
lexically additive. An explicitly configured limiter wins over façade default
composition.

Calling an endpoint constructs an inert named request:

```rust
let request = client.product(42).verbose(true);
let value = request.await?;
```

Construction does not run hooks, fetch credentials, acquire a limiter,
materialize a body, poll a stream, or perform HTTP. Awaiting a valid terminal
begins the managed lifecycle. `.response().await` returns typed response
metadata plus the decoded value when that capability exists.

Use `.plan()?` when you need a sanitized preview of the public method, endpoint
identity, public URL shape, public headers, timeout selection, and
capabilities. Planning reuses real public-head preparation but has none of the
execution effects above and never returns an authenticated/native request.

The built client is an immutable runtime snapshot. Clones share its transport,
hooks, providers, and limiter safely; changing a builder after `build` is not
possible.

## Running proof

The
[request/response example](../../crates/concord-examples/src/request_response/bodies.rs)
shows named construction and terminals. The
[local runtime test](../../crates/concord-examples/tests/local_runtime.rs)
executes it against deterministic local support, and the
[construction test](../../crates/concord-examples/tests/construction.rs)
proves construction remains usable without network execution.

Next: [Authentication](04-authentication.md) or read the
[managed lifecycle](../concepts/managed-lifecycle.md).
