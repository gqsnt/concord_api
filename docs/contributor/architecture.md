# Contributor architecture

## Product contract

```text
Remote HTTP API contract
        |
        v
concord-macros declaration compiler
        |
        v
ordinary typed Rust through concord
        |
        v
concord-core managed lifecycle
```

The package dependency graph is:

```text
concord-core <--- concord-extras
      ^                ^
      |                |
      +---- concord ---+---- concord-macros
               ^
               |
       concord-examples
```

- `crates/concord-core` owns the semantic runtime kernel, private native HTTP,
  finite acquisition, lifecycle, security, and public extension traits under
  `concord_core::contracts`.
- `crates/concord-extras` is an ordinary downstream consumer of those traits.
  It owns JSON, CSV, NDJSON, built-in pagination policies, governor/observer,
  and OAuth2 client credentials.
- `crates/concord-macros` parses, resolves, validates, and emits declarations.
  It is extras-agnostic.
- `crates/concord` is the application façade, public namespace owner, macro
  reexport, feature-forwarding owner, and approved default-composition owner.
- `crates/concord-examples` depends only on `concord`.
- `crates/concord-test-support` contains deterministic runtime and loopback
  infrastructure only.
- `fixtures` contains the isolated renamed-façade, contract-mismatch, feature,
  and MSRV Cargo graphs.
- `tools/xtask` owns typed documentation, fixture, MSRV, policy, and package
  orchestration; `tools/perf` observes public lifecycle behavior.

Core never depends on extras, macros, or façade. Extras depends only on core.
Macros do not depend on extras or core at runtime. Examples do not import an
implementation crate.

## Private generated bridge

A downstream package cannot address a transitive dependency. The compiler
therefore resolves the caller-visible `concord` package with Cargo dependency
renaming support and emits all generated semantic operations through
`concord::__private`.

The façade `api!` wrapper passes its own path and enabled capability tokens to
the hidden declaration compiler. This makes a downstream façade feature, such
as `dangerous-raw-response`, the source of truth without globally enabling a
proc-macro feature. The compiler emits the final capability-specific inherent
method directly.

The façade bridge delegates semantics to core and installs approved
feature-dependent composition such as the default governor. It is hidden,
version-checked, and not a downstream extension contract. Privacy compile
tests prove users cannot forge generated endpoint implementations.

## Ownership rules

At all times:

- the resolved macro model owns generated names and capabilities;
- direct/explicit construction share one endpoint model;
- planning/execution share public-head construction;
- generated/default limiter selection has one façade owner;
- exact numeric defaults have one core source;
- examples own canonical declarations and tests import them;
- each invariant has one primary test owner.

When a semantic trait cannot support an extras implementation, improve the
semantic contract. Do not expose Reqwest/native values or add friend-only
access. When behavior could create a second lifecycle or unbounded mode,
preserve the invariant and record a specification deviation.

Applications use semantic façade modules or the small `concord::prelude`;
keep imports focused on the contract family being implemented.

## Workspace

Open the repository root in rust-analyzer. Use the normal workspace for
product work; open `fixtures/Cargo.toml` separately only for Cargo-graph
fixture development. Optional all-feature analysis can set:

```json
{ "rust-analyzer.cargo.features": "all" }
```

Do not add nested product workspaces or package-local lockfiles. The only
lockfiles are the root and `fixtures/Cargo.lock`.

See [Macro pipeline](macro-pipeline.md), [Testing](testing.md), and
[Release](release.md).
