# Clients, scopes, and endpoints

Model the names your application uses most often: one client root, reusable
scopes beneath it, and endpoint operations at the leaves. Concord turns that
shape into the navigation methods and request types your code calls.

## Start at the client root

The `client` block names the generated client and owns facts shared by all
operations. In the guide contract, `ReferenceApi` supplies the fixed base,
client-wide values, a credential, and common request shaping. The built
`ReferenceApi` is the root from which callers enter endpoints and scopes.

Root endpoints are appropriate when no reusable grouping or scope value is
needed:

```rust
let request: reference_api::StatusRequest = client.status();
```

Calling `status()` creates a request; it does not send it. The return type
makes the chosen operation visible to the compiler and to Rustdoc.

## Reuse context with a scope

A scope groups child operations and captures values they share. This focused
region from the compiled guide contract defines a regional scope:

## Complete example

```rust,ignore
{{#include ../../crates/concord-examples/src/contracts/full_dsl.rs:regional_scope}}
```

`region` is required, so entering the scope requires it:

```rust
let regional = client.regions("eu-west".to_owned());
```

`language` and `scope_label` are not required arguments. They are adjusted on
the returned scope value with generated controls:

```rust
let regional = client
    .regions("eu-west".to_owned())
    .set_language("fr".to_owned());
```

The child `fetch` endpoint can read the visible scope values. The scope shapes
the host and regional path once, and every child inherits that context unless
its own declaration replaces or disables it. Scope values are still inert;
entering one does not clone a separate HTTP client or perform work.

Nested scopes work the same way. A child may use values from each visible
enclosing scope when the name is unambiguous. Keep reusable context at the
smallest scope that owns it, and keep exhaustive namespace and ambiguity rules
in the [DSL reference](../reference/dsl.md).

## Endpoints describe one operation

An endpoint binds an HTTP method, required endpoint inputs, public request
shape, authentication selection, timeout selection, and response
specification. Required endpoint values remain ordinary method arguments in
declaration order:

```rust
let request = regional.fetch(
    "42".to_owned(),
    vec!["stable".to_owned()],
);
```

This produces `reference_api::regions::FetchRequest`. Endpoint values that are
optional or defaulted become consuming controls on the request wrapper; the
wrapper does not acquire controls for values owned only by its enclosing
scope.

## Choose direct or explicit construction

The direct form is best for normal application flow:

```rust
let request = client
    .regions("eu-west".to_owned())
    .fetch("42".to_owned(), vec!["stable".to_owned()]);
```

Use the explicit endpoint type when the operation must be stored before a
client is chosen, planned before binding, or passed through bounded generic
code:

```rust,ignore
{{#include ../../crates/concord-examples/src/contracts/explicit_endpoints.rs:explicit_endpoint}}
```

`reference_api::regions::Fetch::new(...)` owns all required scope and endpoint
values because no scope façade is present to retain them. `client.request`
then binds that endpoint snapshot to the client and returns the same named
request type as the direct path.

Both forms share endpoint identity, request shaping, capabilities, and
execution behavior. The explicit form does not bypass validation or expose a
lower-level transport request. Prefer direct navigation until storing,
planning, or generic composition makes the extra value useful.

## Related

- [Values, defaults, and controls](values-and-controls.md)
- [Shape the public request](request-shaping.md)
- [Understand generated Rust](generated-rust.md)
- [Complete DSL reference](../reference/dsl.md)
- [Generated API reference](../reference/generated-api.md)
