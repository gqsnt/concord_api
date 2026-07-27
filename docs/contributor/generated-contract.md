# Generated contract versioning

Generated Rust is compiled in application crates but calls hidden operations
owned by the Concord façade and core. The generated contract versions that
private compiler/runtime agreement. It is not the DSL version, the public
crate version, or an extension API.

## Current contract

The current generated-contract version is **31**.

| Owner | Current source of truth | Responsibility |
|---|---|---|
| declaration compiler | `crates/concord-macros/src/emit/bridge.rs` | version expected by newly emitted Rust and structural equality assertions |
| semantic runtime | `crates/concord-core/src/generated_contract.rs` | version implemented by core |
| façade bridge | `crates/concord/src/lib.rs` hidden `__private` module | renamed-dependency-safe reexports and approved default composition |
| generated expansion | private support module inside each `api!` expansion | const handshake against the façade path |

Every expansion calls the façade-relative
`__private::assert_generated_contract(31)` during const evaluation. It also
asserts that the independent compiler and runtime agree on structural ceilings
the compiler must enforce before generated Rust reaches core.

Version 31 added dedicated exact/static-scheme Authorization construction and
the mirrored Authorization-scheme ceiling to the generated authentication
bridge. Version 30 added the hidden `PageRequestBodySpec` obligation to generated
pagination and permits newly emitted paginated endpoints to carry a trusted
repeatable request body. Version 29 removed the generated native-retry
baseline, whose old expansion called the removed `retry_reqwest_default`
bridge, and added the generated `.retry(RetryPolicy)` builder surface.
Version 28 replaced the removed
raw-response helper and request type with the normal response-bytes request
bridge. Version 27 introduced core-owned bounded formatting for generated
rate-limit key dimensions.

## What the contract covers

The contract covers private types, functions, constants, and semantic
obligations used directly by emitted Rust, including these categories:

- client-core construction and façade-selected runtime defaults;
- generated credential slots and safe construction errors;
- endpoint/head preparation and bounded public value formatting;
- generic pending request, response, pagination, and planning operations;
- semantic endpoint/body/response traits implemented by generated types;
- stable structural values mirrored by the compiler.

The public generated client, scopes, endpoints, requests, and methods are
important API, but a public surface change is not automatically a private
contract change. Ask whether old/new emitted Rust needs a different hidden
runtime operation.

## Why the façade owns the bridge path

A Rust package cannot address an undeclared transitive dependency. Generated
code therefore cannot emit `::concord_core`. The façade macro passes `$crate`
to the compiler, and all private paths resolve through that supplied façade.

This supports:

- `concord` renamed in a downstream `Cargo.toml`;
- one direct application dependency;
- façade-owned composition such as the default local limiter;
- exact-version coordination among core, macros, extras, and façade.

`fixtures/dependency/renamed_facade` compiles a broad generated surface through
a dependency named `runtime` and proves there is no hard-coded crate name.

## When to bump

Bump the contract when emitted Rust and the façade/core producer no longer
agree under the current private protocol.

Examples that require a bump:

- adding, removing, or incompatibly changing a hidden function/type used by
  emitted Rust;
- changing the argument/result or trait obligation of an emitted private call;
- changing private semantics such that code emitted for the old meaning would
  be interpreted differently by the new runtime;
- moving an emitted operation between bridge owners without a compatible
  façade reexport;
- adding a new mirrored structural invariant that requires a different
  versioned handshake rather than an additive equality assertion.

Changes that normally do not require a bump:

- accepting or rejecting a declaration earlier;
- improving spans or diagnostic text;
- changing generated public names while using the same hidden operations;
- adding/removing an inherent terminal whose implementation uses an existing
  compatible bridge capability;
- changing generated Rustdoc;
- refactoring core behind identical bridge semantics;
- adding an extras implementation or façade reexport that emitted Rust does
  not name.

When uncertain, compare the generated consumer with the façade/core producer.
Do not bump simply because a compiler file changed, and do not avoid a bump
because a changed item is `#[doc(hidden)]`.

## Atomic bump checklist

A bump is one coordinated product change:

1. Describe the old and new private operation and why compatibility cannot be
   preserved.
2. Update emitted Rust to the new protocol.
3. Update the compiler's expected version in
   `crates/concord-macros/src/emit/bridge.rs`.
4. Update core's implemented version in
   `crates/concord-core/src/generated_contract.rs`.
5. Update façade `__private` exports/composition so the renamed façade remains
   the only emitted root.
6. Update structural equality assertions if compile-time mirrored limits
   changed.
7. Update the core public-contract test and macro bridge tests.
8. Change the incompatible-façade fixture to a value different from the new
   contract and update its exact expected diagnostic.
9. Re-run the renamed-façade fixture and facade privacy/capability UI suites.
10. Record the private contract change in `CHANGELOG.md` and review sibling
    exact-version release coordination.

Do not land a compiler-only or runtime-only half. Workspace path dependencies
can hide packaging mismatches that the isolated fixture is designed to expose.

## Failure behavior

An incompatible version fails during compilation through a const assertion.
It must not:

- defer incompatibility to request execution;
- silently fall back to a reduced surface;
- expose native/private values in the diagnostic;
- suggest that applications call the hidden bridge;
- succeed merely because the incompatible endpoint is never executed.

The contract-mismatch fixture substitutes a façade reporting version 999 while
the generated consumer expects 31. `concord-xtask fixtures` requires the
focused mismatch diagnostic and treats unexpected compilation success as a
failure.

Structural mirrored-limit assertions have the same early-failure purpose.
They catch a compiler/runtime ceiling drift even if both sides accidentally
retain the same version. A deliberate semantic change still follows the bump
decision above; equality assertions are not a substitute for version review.

## Extension and security boundary

The bridge is public only in the Rust visibility sense required for expansion
across crate boundaries. It is hidden, unstable, and not supported for
handwritten use.

Downstream extensions implement the documented traits reexported by
`concord`; they do not implement generated-private endpoint traits or call
bridge functions. Facade Trybuild privacy cases prove that endpoint forging,
wrong-client binding, executor escape, and private request-field access remain
unavailable.

## Proof owners and commands

| Claim | Owner |
|---|---|
| compiler emits version and limit assertions | macro bridge unit tests |
| core implements exactly version 31 | `crates/concord-core/tests/public_api/generated_contract.rs` |
| mismatch fails early and clearly | `fixtures/dependency/contract_mismatch` |
| renamed façade works | `fixtures/dependency/renamed_facade` |
| emitted code has no direct core path | xtask policy plus macro emission tests |
| hidden surface cannot be forged | facade/core Trybuild privacy suites |

Run:

```bash
cargo test -p concord-macros --lib --locked
cargo test -p concord-core --all-features --locked
cargo test -p concord --test ui --all-features --locked
cargo run --locked -p concord-xtask -- fixtures --locked
cargo run --locked -p concord-xtask -- policy --locked
```

## Related

- [Generated surface and contract](architecture/generated-surface.md)
- [Macro pipeline](macro-pipeline.md)
- [Crate boundaries](architecture/crate-boundaries.md)
- [Compatibility and MSRV](../reference/compatibility.md)
- [Release](release.md)
