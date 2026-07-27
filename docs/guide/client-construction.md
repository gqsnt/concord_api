# Build and share clients

Build a generated client once for an operational configuration, then clone and
share that built value across tasks. The builder collects required contract
inputs and deployment policy; `build()` validates them and fixes an immutable
runtime snapshot.

## Start with the smallest constructor

When a declaration has no required public values, secrets, or providers and
needs no custom deployment configuration, use its direct constructor:

```rust
let client = MinimalApi::new()?;
```

Use the named builder as soon as the client needs contract inputs or runtime
configuration. `ReferenceApi::builder()` is the predictable configured path:

## Complete example

```rust,ignore
{{#include ../../crates/concord-examples/src/contracts/full_dsl.rs:configured_client}}
```

The example supplies:

- the required public `tenant` with `set_tenant`;
- optional and defaulted public values with their generated controls;
- protected API-key material with the declared `api_key` builder method.

Required secrets are wrapped in `SecretString` before they enter the builder.
Provider-backed credentials similarly use the provider method named by the
declaration. Their methods keep declaration names rather than using the public
value `set_*` vocabulary.

`Client::new()` exists only when the declaration has no required public
values, secrets, or providers. It uses the same validation and runtime
defaults as `Client::builder().build()`.

## Configure runtime policy before `build`

Use the two handwritten configuration entry points for deployment choices:

```rust
let client = ReferenceApi::builder()
    .set_tenant("tenant-a".to_owned())
    .api_key(api_key)
    .configure_http(|http| {
        http.connect_timeout(std::time::Duration::from_secs(5))
    })
    .configure_runtime(|runtime| {
        runtime.runtime_endpoint_timeout(std::time::Duration::from_secs(30))
    })
    .build()?;
```

`configure_http` selects safe transport settings. `configure_runtime` selects
finite limits, hooks, operational timeouts, and a custom limiter. Contract
facts such as paths, credential placement, and generated endpoint timeouts
stay in the declaration.

Enabled façade features compose their documented defaults. With the ordinary
default features, HTTPS and HTTP/2 transport support are available and each
independent built client receives a bounded process-local
`LocalRateLimiter`. Default hooks do nothing until replaced. Explicit
configuration wins where the builder contract says it replaces a default.

## Handle build failures safely

`build()` checks that every required public value, protected secret, and
provider is present and valid before producing a client. Failure returns
`ClientBuildError`, with a safe kind and generated input name. The error does
not format the supplied secret or value.

Construction may also reject invalid transport material or finite
configuration. Match typed kinds where recovery differs; do not parse
`Display` or `Debug` output. A build failure occurs before any endpoint request
can execute.

## Know what clones share

The built client is immutable. Cloning it shares the same transport, hooks,
credential slots and providers, limiter object, and runtime limits. This is
the normal way to give one configured client to concurrent tasks:

```rust
let shared = std::sync::Arc::new(client);
let for_worker = std::sync::Arc::clone(&shared);
```

An `Arc` around the client is convenient for application ownership, although
the generated client itself is also cheaply cloneable.

Calling the builder again creates a separate runtime snapshot. Independent
builds receive independent default limiter state and credential slots. If
separate clients should deliberately coordinate quota or provider state,
construct the relevant implementation behind an `Arc`, clone that `Arc` into
each builder, and configure it explicitly. Sharing becomes visible application
policy instead of an accidental global.

After `build()`, mutate request snapshots rather than the client. To change
transport or runtime policy, build a new client and replace it through your
application's own configuration lifecycle.

## Related

- [Requests, planning, and errors](requests-planning-errors.md)
- [Values, defaults, and controls](values-and-controls.md)
- [Generated client construction reference](../reference/generated-api.md#client-construction)
- [Feature flags](../reference/features.md)
- [Error reference](../reference/errors.md)
