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
  It owns JSON, CSV, NDJSON, built-in pagination policies, local limiter/observer,
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

A downstream package cannot address a transitive dependency. Applications
therefore declare APIs only through `concord::api!`, including when the façade
dependency is renamed, and generated semantic operations go through the
façade's hidden `__private` bridge.

The façade `api!` wrapper passes its own path and enabled capability tokens to
the hidden declaration compiler. This makes a downstream façade feature, such
as `dangerous-raw-response`, the source of truth without globally enabling a
proc-macro feature. The compiler emits the final capability-specific inherent
method directly.

The façade bridge delegates semantics to core and installs approved
feature-dependent composition such as the default local limiter. It is hidden,
version-checked, and not a downstream extension contract. Privacy compile
tests prove users cannot forge generated endpoint implementations.

## Ownership rules

At all times:

- the resolved macro model owns generated names and capabilities, including
  exact `set_*`, independently derived absence-capable `clear_*`, and
  declaration-default `reset_*` value controls;
- rate-limit declarations resolve applicability and ordered key dimensions in
  the lexical owner; inherited rules never re-resolve against descendant-only
  values;
- the final endpoint validates every effective rate rule after pagination and
  host shaping are resolved, rejecting direct or host-derived
  pagination-controlled partition dependencies;
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

## Managed boundary ownership

Core finalizes every generated and runtime-defined public request head under
one component, URL, header-count, and header-byte budget. Authentication plans
remain separate: public head construction finishes first, generic
authentication cannot target a request- or transport-owned header, and
private material is attached only after limiter authority at the native
boundary.

Generated keyed rules format only referenced public values through core's
bounded key constructor. Global rules retain the allocation-minimal fast path.
Generated declaration rule IDs are deterministic inside one generated client
contract but do not create a global namespace across unrelated macro
expansions; applications explicitly sharing a limiter across such contracts
own namespace compatibility.
The façade selects a fresh default `LocalRateLimiter` for each built client;
extras owns its token buckets, safe fully-replenished idle pruning, and
recoverable `StateCapacity` policy. Core and macros never depend on that
concrete implementation.

For received responses, core constructs the sanitized head, awaits limiter
observation under the remaining operation deadline, and only then invokes the
user response hook. Challenge/status handling and body acquisition follow.
Limiter failure or timeout therefore prevents later response hooks and body
work.

Core also owns structural startup ceilings before native parsing or copying
proxy and TLS inputs. Extras own concrete codec/provider behavior, but codec
errors cross back into core only as finite categories. No extension may
smuggle a native source error or an unbounded message into the managed
lifecycle.

All outgoing application stream forms—direct `StreamBody`, body factories,
and multipart/native serialization—converge on the final core-owned managed
request stream. That boundary suppresses empty chunks, owns request-wide
empty-chunk and byte totals, preserves exact-length accounting, and yields
cooperatively during ready-empty runs.

Every core-owned response path that polls a native body uses one private
response-progress owner. Streaming success applies its per-poll decision in
the bounded response stream; buffered success, terminal status capture, and
provider HTTP use the corresponding async helper and cooperative yield.
Neither owner retains a chunk value or native error, and neither performs a
background drain after terminal failure.

## Workspace

Open the repository root in rust-analyzer. Use the normal workspace for
product work; open `fixtures/Cargo.toml` separately only for Cargo-graph
fixture development. Optional all-feature analysis can set:

```json
{ "rust-analyzer.cargo.features": "all" }
```

Do not add nested product workspaces or package-local lockfiles. The only
lockfiles are the root and `fixtures/Cargo.lock`.

The workspace Tokio dependency selects no global capability set. Each crate
declares the features used by its ordinary code, while multi-thread runtimes
and paused-time support stay in dev dependencies or explicit executable
owners. Measure changes with `cargo tree -e features`; do not broaden the
workspace entry for test convenience.

See [Macro pipeline](macro-pipeline.md), [Testing](testing.md), and
[Release](release.md).
