# Crate boundaries

Concord's dependency graph points from reusable semantics toward optional
composition. The direction is a design constraint: no implementation crate
receives privileged access merely because it is maintained in the same
workspace.

```text
concord-examples
       |
       v
    concord ------------------------> concord-macros
       |  \
       |   +------------------------> concord-core
       v
concord-extras ---------------------> concord-core

concord-test-support, concord-xtask, and concord-perf are development owners.
fixtures/ is an isolated Cargo workspace.
```

The arrows above show normal product dependencies toward the crate being
depended on. More exactly:

```text
concord-core:   no Concord sibling dependency
concord-extras: concord-core
concord-macros: no Concord sibling dependency
concord:        concord-core + concord-extras + concord-macros
examples:       concord only
```

`tools/xtask/src/policy.rs` checks this set from Cargo metadata. Product crates
may use `concord-test-support` only as a path-only development dependency.

## `concord-core`

**Owns:** the semantic runtime kernel, client snapshot, managed executor,
private native HTTP, finite acquisition, safe errors and redaction, and public
extension traits.

**Depends on:** ecosystem primitives and the private native HTTP
implementation, but no other Concord product crate.

**Must not know:** façade naming, macro syntax, generated source names, or
first-party implementations such as JSON, OAuth2 client credentials, CSV, or
the local limiter.

**Public extension role:** authoritative contracts under
`concord_core::contracts` and the focused semantic modules reexported by the
façade.

**Proof owners:** core unit/integration tests, core compile-fail tests, and
warning-denied core Rustdoc. Executor behavior is primarily exercised under
`crates/concord-core/src/execute/tests`; public impossibility claims live under
`crates/concord-core/tests/ui`.

The key invariant is semantic ownership. If a useful extras implementation
cannot be written through a bounded core trait, improve that trait rather than
adding friend-only access to native values.

## `concord-extras`

**Owns:** first-party implementations of public semantic traits: JSON, CSV,
NDJSON, built-in pagination controllers, the process-local limiter and
response observer, and OAuth2 client credentials.

**Depends on:** `concord-core` with default features disabled, plus only the
ecosystem crates needed by selected extras features.

**Must not know:** macro internals, generated private types, façade feature
tokens, or native application transport.

**Public extension role:** examples of how downstream crates can implement
Concord contracts without privilege. The façade reexports enabled extras in
stable task-oriented namespaces.

**Proof owners:** extras unit tests and doctests, facade integration tests that
compose extras as ordinary types, and feature fixtures that prove disabled
implementations disappear.

Keeping extras downstream proves that `concord-core` contracts are sufficient
for independent implementations.

## `concord-macros`

**Owns:** declaration parsing, source spans, lexical resolution, inheritance,
cross-declaration validation, the semantic surface model, generated Rustdoc,
and Rust emission.

**Depends on:** compiler-oriented libraries such as `syn`, `quote`,
`proc-macro2`, plus pure value parsers needed for compile-time validation. It
does not depend on core or extras.

**Must not know:** concrete codecs/providers/controllers, runtime object
layout, or native transport. User types remain ordinary Rust paths.

**Public extension role:** none. Its only proc-macro entry is hidden behind
`concord::api!`; applications do not depend on the implementation crate.

**Proof owners:** macro library tests for each pipeline stage, facade Trybuild
tests for public pass/fail behavior, and dependency fixtures for the generated
bridge.

Because the macro cannot import core constants, structural limits that must be
checked at compile time are mirrored and emitted as const equality assertions.
The versioned bridge detects incompatible macro/runtime combinations.

## `concord`

**Owns:** the application façade, `api!` entry, public namespaces, feature
forwarding, macro capability handoff, and approved default composition.

**Depends on:** core, extras, and macros at the exact workspace version with
their defaults disabled.

**Must not know:** duplicate runtime semantics or extension-specific
shortcuts. Its hidden bridge delegates semantic operations to core.

**Public extension role:** canonical import path for applications and
extension authors. It reexports contracts and enabled first-party
implementations without changing their trait meaning.

**Proof owners:** facade generated/runtime integration tests, public API
tests, Trybuild suites, no-default/all-feature Rustdoc, and isolated Cargo
fixtures.

The façade also owns defaults that are composition choices rather than core
truth. The process-local limiter is installed here when `rate-limit` is
enabled; an explicit application limiter always wins.

## `concord-examples`

**Owns:** canonical complete declarations and usage compositions included by
the book.

**Depends on:** `concord` only among product crates, with explicit feature
forwarding.

**Must not know:** core, extras, or macro implementation paths. An example
that needs one of those paths would teach an application dependency graph that
Concord does not support.

**Public extension role:** compiler-owned examples for users and extension
authors.

**Proof owners:** the construction test imports every enabled declaration;
loopback runtime tests execute representative flows. Book pages include named
regions, while the crate and its tests remain the compiler owners.

## Development-only owners

`concord-test-support` owns deterministic execution and loopback
infrastructure. It is not product policy and cannot be a normal product
dependency.

`concord-xtask` owns portable repository checks: documentation inventory and
includes, isolated fixtures, MSRV UI tests, dependency policy, packaging, and
source archives. It may inspect product constants to validate exact generated
documentation, but it is not an application dependency.

`concord-perf` observes public lifecycle and macro-expansion scale. Benchmarks
must not gain a private fast path that applications cannot use.

`fixtures/` is excluded from the main workspace. Its isolated dependency
graphs prove facts Cargo feature unification cannot prove inside one
workspace: absence, renamed façade paths, and generated-contract mismatch.

## Boundary failure modes

| Failure | Why it is hazardous |
|---|---|
| Core depends on extras | one optional implementation becomes runtime policy |
| Macro depends on core/extras | compile-time loading and source semantics couple to runtime composition |
| Generated code names `concord_core` directly | renamed façade dependencies and single-dependency use break |
| Examples import implementation crates | documentation stops proving the supported application graph |
| Test support becomes a normal dependency | deterministic internals can leak into production behavior |
| Extras receives native transport | extensions can bypass redaction, bounds, and lifecycle ordering |

## Evolving the graph

Start with the semantic owner, not the desired filename:

1. Put lifecycle guarantees and extension traits in core.
2. Put optional concrete policies in extras.
3. Put declaration meaning and generated capabilities in macros.
4. Put public reexports, feature forwarding, and defaults in the façade.
5. Prove downstream usability through examples and facade tests.
6. Add a fixture only when the claim depends on an isolated Cargo graph.

Run these checks after a graph change:

```bash
cargo metadata --format-version 1 --all-features --locked
cargo tree -e features
cargo run --locked -p concord-xtask -- policy --locked
cargo run --locked -p concord-xtask -- fixtures --locked
```

## Related

- [Architecture overview](../architecture.md)
- [Feature composition and evolution](feature-composition.md)
- [Macro pipeline](../macro-pipeline.md)
- [Extension model](../../extending/README.md)
