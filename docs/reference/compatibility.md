# Compatibility and MSRV

Concord's workspace version is `0.1.0`. The packages are currently marked
unpublished, so this repository does not claim a crates.io release or a
long-term compatibility window beyond the checked source and tests.

## Rust support

| Contract | Value |
|---|---|
| Minimum supported Rust version | Rust 1.86.0 |
| Workspace edition | Rust 2024 |
| Maintained package version | `0.1.0` |
| Cargo resolver | version 3 |

The MSRV is a real build owner, not a claim that every development tool runs
on Rust 1.86.0. The maintained matrix checks the library targets for `concord-core`,
`concord-macros`, `concord-extras`, and `concord`, then runs the dedicated
MSRV UI fixture owner. Ordinary development and the full workspace matrix use
the current stable toolchain.

Applications should treat Rust 1.86.0 as the minimum compiler for this checkout.
An optional dependency may still require a newer compiler if an application
resolves a version outside the workspace lockfile; verify the application's
own resolved graph.

The private declaration compiler uses Syn 3 and exposes no Syn type in
Concord's public API. Its normal dependency uses Syn's default compiler
capabilities without the complete item/file AST; `full` is enabled only for
macro-crate tests. The workspace lockfile may therefore contain both Syn 3 for
Concord and Syn 2 for upstream procedural-macro dependencies until those
dependencies migrate. That duplicate-major graph does not change the DSL,
generated Rust contract, or Rust 1.86.0 support.

## Package and façade contract

Applications depend on the single `concord` façade. The façade re-exports
public semantic modules and composes enabled first-party implementations.
`concord-core`, `concord-extras`, and `concord-macros` are sibling
implementation owners, not additional application dependencies.

Internal sibling dependencies use exact `=0.1.0` requirements while the
packages remain unpublished. The lockfile is authoritative for repository
validation.

The generated surface supports a renamed Cargo dependency. For example:

```toml
[dependencies.service_contract]
package = "concord"
path = "../concord"
```

```rust,ignore
use service_contract::{NoContent, api};

api! {
    client RenamedApi {
        base "https://renamed.example.test/";
    }

    GET health() -> NoContent;
}
```

Generated paths resolve through the caller-visible façade name. They do not
assume that a transitive package is directly importable or that the façade is
spelled `concord`.

## Generated-code compatibility

The proc macro and façade share a private generated-code bridge. Every macro
expansion emits a const assertion against the façade's bridge version and
against structural limits duplicated for compile-time diagnostics. A mixed
incompatible macro/core pair therefore fails compilation with a focused
contract-mismatch assertion instead of producing silently incompatible Rust.

That bridge is `#[doc(hidden)]` and is not an extension surface. Applications
must not import `concord::__private`, copy emitted internals, or persist
expanded source as a compatibility artifact. Rebuild generated code with the
resolved façade and proc-macro pair.

Generated public names, controls, and terminals are documented contracts for
the current checkout:

- client and scope navigation retain declaration-domain names;
- public value mutation uses `set_*`;
- absence capability uses `clear_*`;
- declaration defaults use `reset_*`;
- request bodies use explicit `#[body]`;
- named clauses are order-independent;
- ordinary methods are inherent and need no extension-trait import;
- feature-disabled capabilities are absent or fail their explicit trait bound.

There are no legacy positional secret/provider constructors, bare value
setter aliases, or compatibility endpoint trees.

## Feature compatibility

The maintained façade builds with no default features and with all features.
Feature flags are additive Cargo capabilities; the default bundle and every
opt-in feature are listed in [Feature flags](features.md).

`default-features = false` does not remove the core request lifecycle,
redaction, or finite head/body contracts. It does remove the default TLS,
HTTP/2, and local-limiter implementations. A declaration that needs an absent
helper or default must select it explicitly or provide a public extension.

## Change expectations

Until a published compatibility policy exists, any public declaration syntax,
generated surface, trait, error enum, feature, or runtime behavior can change
with the repository's versioned work. Non-exhaustive enums still require
fallback matching now.

User-visible changes belong in the repository's `CHANGELOG.md`. Changes to
the private generated contract must update the macro and core owners together
and keep mismatch fixtures focused. The release procedure owns package
contents, MSRV checks, and exact sibling-version validation.

## Related

- [Generated API reference](generated-api.md)
- [Feature flags](features.md)
- [Complete DSL reference](dsl.md)
- [Contributor testing](../contributor/testing.md)
- [Release process](../contributor/release.md)
