# The contract model

Use Concord by describing the remote API once, then work with the Rust types
generated from that description. The shortest useful mental model has four
steps:

```text
declaration -> generated Rust -> inert request -> managed execution
```

Each step has a different job. Keeping those jobs separate makes a client
predictable to call, safe to inspect, and consistent across features.

## Declare the remote contract

The `api!` declaration records facts that belong to the service: its fixed
base, public values, scopes and endpoints, request shaping, authentication
placement, response representations, timeouts, pagination bindings, and
declared rate limits. It is the place to express what an operation means, not
how one deployment should observe or transport it.

Here is the complete contract used throughout this guide:

## Complete example

```rust,ignore
{{#include ../../crates/concord-examples/src/contracts/full_dsl.rs:core_contract}}
```

This declaration is compiled as part of `concord-examples`. Concord validates
its structure during compilation, while ordinary Rust trait checking verifies
that the selected value, body, and response types support the operations in
which they are used.

## Use ordinary generated Rust

The declaration produces a named client builder, an immutable client, scope
values, explicit endpoint values, and named request wrappers. Required values
stay visible as builder inputs or method arguments. Optional and defaulted
values gain only the controls they can support. Endpoint-specific capabilities
such as pagination or typed response metadata appear only on endpoints for
which they are valid.

You do not need to interpret the declaration again in application code.
Direct methods such as `client.status()` and explicit endpoint values such as
`reference_api::Status::new()` select the same endpoint contract.

## Construct before executing

Calling a generated endpoint creates an inert request value. You may adjust
its endpoint-owned values, inspect its type, or create a safe request plan
without opening a connection. Request construction does not fetch
credentials, consume a rate-limit permit, run hooks, encode a body, or send
HTTP.

Execution begins when an async terminal is polled. The usual terminal is the
request itself:

```rust
let bytes = client.status().await?;
```

Some endpoints also expose `.response()` for typed response metadata or
`.paginate(...)` for a declared pagination contract. These methods are not a
second execution system; they enter the same managed lifecycle with a
different result shape.

## Separate service policy from deployment policy

Put stable service facts in the declaration. Put deployment choices on the
generated builder:

- the declaration owns paths, query entries, public headers, credential
  placement, response types, and endpoint identity;
- `configure_http` owns transport choices such as TLS material, an explicit
  proxy, and safe connection settings;
- `configure_runtime` owns limits, hooks, operational timeouts, and an
  explicitly shared limiter.

This boundary lets two applications use the same declared service contract
with different operational policy. It also lets two independently built
clients remain isolated, while clones of one built client deliberately share
the same runtime state.

Concord does not infer arbitrary service semantics from Rust type names, and
it does not turn a request plan into an authenticated native request. Use the
declaration for supported contract behavior and the builder for supported
runtime configuration; application-specific behavior outside those
boundaries remains application code.

## Related

- [Clients, scopes, and endpoints](../guide/clients-scopes-endpoints.md)
- [Understand generated Rust](../guide/generated-rust.md)
- [The managed lifecycle](managed-lifecycle.md)
- [Complete DSL reference](../reference/dsl.md)
- [Generated API reference](../reference/generated-api.md)
