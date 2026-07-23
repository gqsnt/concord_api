# Compatibility reference

Concord is currently version `0.1.0` and the repository has no verified
published release commitment. The architectural refactor therefore uses a
clean public break while preserving invariants and offering an explicit
migration guide at release hardening.

The stable direction is:

- applications depend on one `concord` package;
- declarations import `concord::api` and helper types from `concord`;
- generated paths resolve a renamed façade dependency and use no transitive
  package assumption;
- endpoint/scope source identifiers are `snake_case`;
- clients use named builders rather than positional secret/provider
  constructors;
- request bodies use explicit `#[body]`;
- named clauses are order-independent;
- runtime-defined endpoints use the current runtime endpoint API;
- runtime transport policy is configured in Rust, not custom DSL syntax;
- providers and representation/controller types are generic Rust types.

Generated code and the façade share a versioned private bridge contract. That
bridge is not a user extension surface. Compiler output emits against the
caller-visible façade package, including Cargo dependency renaming.

MSRV is Rust 1.85.0. Workspace dependencies are locked and internal sibling
versions are exact while packages remain unpublished.

See the root release migration guide produced in Phase 14 for concrete
before/after forms.
