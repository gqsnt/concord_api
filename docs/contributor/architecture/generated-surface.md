# Generated surface and contract

Generated Rust is the typed seam between declaration meaning and the semantic
runtime. Its public items are application API; its hidden implementation is a
versioned compiler/runtime integration contract. Those two forms of
compatibility must be reviewed separately.

## Four models, four responsibilities

```text
source syntax
    -> resolved semantic model
        -> public surface and documentation model
            -> emitted Rust
```

| Model | Owns | Does not own |
|---|---|---|
| Source syntax | tokens, spans, source ordering | inherited meaning or generated capabilities |
| Resolved semantics | lexical owners, normalized configuration, endpoint identities, cross-layer validity | Rust spelling chosen for the public surface |
| Surface model | exact type/method/module names, setter matrix, terminal availability, endpoint Rustdoc facts | parsing or runtime effects |
| Emission | Rust syntax implementing the already-decided model | new diagnostics, fallback semantics, type-name recognition |

This ordering prevents drift. Collision validation and emitters consume the
same exact `set_*`, `clear_*`, and `reset_*` names. Endpoint Rustdoc and the
actual terminal methods consume the same capability model. Emission cannot
silently accept an invalid declaration that semantic validation rejected.

## Public generated artifacts

For a client `CatalogApi` with a scoped endpoint `products::by_id`, the stable
shape is:

```text
CatalogApi
CatalogApiBuilder
catalog_api
  Endpoint                       sealed generated trait
  products
    Products                     scope snapshot
    ById                         explicit endpoint snapshot
    ByIdRequest                  bound inert request
```

The client and scope navigation path and the explicit endpoint path converge
on the same endpoint model. The named request wraps core's generic pending
request without exposing its endpoint field. Generated snapshots retain the
shared client core and owned public values; they do not create transport or
execute extensions.

The [Generated API reference](../../reference/generated-api.md) is the
canonical user-facing inventory of names, controls, and terminals. This page
focuses on why their compiler ownership matters.

## Type-level availability is part of the API

The compiler uses inherent methods and sealed bounds to make incoherent
operations fail at compile time:

- pagination exists only for a declared paginated endpoint with a compatible
  response;
- credential storage exists only on the endpoint that supplies that
  endpoint-backed credential;
- endpoint value controls exist only for the value's lexical owner;
- clear and reset are independently derived from absence and default
  capability;
- `.response_bytes()` is emitted on every named request, while its sealed
  `ResponseBytesMode` bound admits buffered and discard modes and rejects
  streaming modes.

An unsupported operation should fail as an ordinary Rust method/type error.
Do not replace capability absence with one broad trait whose methods fail at
runtime.

## Visibility and sealing

Generated items have normal Rust visibility relative to the module containing
`api!`. The generated `Endpoint` trait supports bounded generic composition,
but it is sealed so downstream code cannot forge a type that the client will
execute as one of its endpoints.

Private endpoint fields and hidden semantic implementations prevent:

- arbitrary endpoint implementations;
- binding an endpoint to the wrong generated client;
- bypassing public-head preparation;
- reaching core's executor through a custom generated wrapper;
- mutating inherited values through an owner that does not possess them.

These are compiler-enforced boundaries, primarily owned by
`crates/concord/tests/ui/fail/privacy` and
`crates/concord/tests/ui/fail/capabilities`.

## The façade-relative private bridge

Generated implementations need runtime operations that applications should
not call. The façade exposes those operations under its hidden `__private`
module and passes its own `$crate` path into the proc macro.

That indirection has three jobs:

1. generated code works when the `concord` dependency is renamed;
2. applications need one direct product dependency, not transitive core or
   extras dependencies;
3. the façade can install approved feature-dependent composition while core
   remains independent of extras.

The bridge includes semantic construction and request operations, generated
endpoint traits and value types, and the generated-contract assertion. It is
not a public extension surface. Hidden does not mean unversioned: already
compiled emitted Rust and the runtime must agree on its meaning.

## Version and structural handshake

Every expansion emits a const assertion for generated-contract version 29.
It also emits equality assertions for structural values that the independent
macro and core must enforce identically, including authentication placement,
request-head, and generated rate-plan ceilings.

The version changes only when emitted private Rust and the façade/core bridge
must agree on a different operation, signature, type, or semantic obligation.
Public generated names, diagnostics, and source-language validation can change
without a bridge bump when they continue to use the same private contract.

The exact current owners and atomic bump checklist are in
[Generated contract versioning](../generated-contract.md).

## Type neutrality

The macro treats body, response, provider, controller, and other extension
paths as Rust types constrained by semantic traits. It does not recognize
`Json`, `Csv`, or another first-party spelling. A downstream type may use the
same name, and aliases or fully qualified paths work normally.

This is why extras can evolve downstream and why disabled feature
implementations disappear through Cargo rather than through proc-macro
branches. The `crates/concord/tests/public_api/type_neutrality.rs` tests own
that property.

## Change hazards

Review generated-surface changes for all of these:

- **Name collisions:** validate the exact emitted spelling before emission.
- **Owner leaks:** do not put inherited controls on a request that owns only
  endpoint values.
- **Capability drift:** generated docs, inherent methods, and sealed bounds
  must consume one surface decision.
- **Path drift:** never emit a direct `concord_core` or `concord_extras` path.
- **Privacy drift:** a convenience field or trait implementation can create a
  new executor entry.
- **Version drift:** a hidden bridge change without an atomic bump can compile
  against the wrong runtime.
- **Type coupling:** recognizing a concrete extras name makes aliases and
  independent implementations behave differently.

## Verification

```bash
cargo test -p concord-macros --lib --locked
cargo test -p concord --test ui --all-features --locked
cargo run --locked -p concord-xtask -- fixtures --locked
cargo run --locked -p concord-xtask -- policy --locked
cargo rustdoc -p concord --no-default-features --locked -- -D warnings
cargo rustdoc -p concord --all-features --locked -- -D warnings
```

Inspect new Trybuild stderr rather than accepting it blindly. A source scan is
not proof that a generated method is inaccessible; compile a downstream use
that must fail.

## Related

- [Compile-time to runtime flow](compile-runtime-flow.md)
- [Macro pipeline](../macro-pipeline.md)
- [Generated contract versioning](../generated-contract.md)
- [Feature composition and evolution](feature-composition.md)
- [Generated API reference](../../reference/generated-api.md)
