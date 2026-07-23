# 7. Explicit and runtime endpoints

Generated direct calls and explicit endpoint values are equivalent:

```rust
let direct = client.lookup("42".to_owned());

let endpoint = catalog_api::Lookup::new("42".to_owned());
let explicit = client.request(endpoint);
```

Both produce the same named request wrapper, safe endpoint description, and
managed execution. Explicit values are useful for storage, generic
composition, and applying optional setters before binding a client.

Use `concord::endpoint::RuntimeEndpoint` when method, public path, query, or
headers are chosen at runtime:

```rust
use concord::endpoint::{EndpointId, RuntimeEndpoint};
use concord::http::Method;

let endpoint = RuntimeEndpoint::builder(
    EndpointId::new("catalog.lookup")?,
    Method::GET,
)
.path_segment("records")
.path_segment("42")
.build()?;

let request = client.request(endpoint);
```

The endpoint ID is bounded and validated. Timeout selection is a real
tri-state: inherit the client default, use an explicit duration, or disable
the timeout. Runtime endpoints support only coherent typed contracts; there is
no partial runtime pagination API.

`.plan()` returns the same sanitized description shape for generated and
runtime endpoints. It constructs the real public request head without hooks,
credentials, limiter work, body encoding/factories, stream polling, transport,
or completion. It never exposes an exact native/authenticated preview.

## Running proof

The
[generated/runtime module](../../crates/concord-examples/src/runtime_endpoints/generated_and_runtime.rs)
constructs both forms, and the
[explicit endpoint module](../../crates/concord-examples/src/contracts/explicit_endpoints.rs)
shows reusable generated values. The
[construction test](../../crates/concord-examples/tests/construction.rs) and
[local runtime test](../../crates/concord-examples/tests/local_runtime.rs)
prove inert construction and lifecycle convergence.

Continue with the [contract model](../concepts/contract-model.md) or a
[production recipe](../recipes/production-configuration.md).
