# Workspace and adding a feature

The repository is one Rust workspace with separate packages for compilation,
runtime semantics, first-party implementations, application ergonomics,
examples, tests, and maintenance tooling. Start a change in the package that
owns the behavior, then expand outward only when its public consequences
require it.

Use the nested [Adding a feature](adding-a-feature.md) checklist once the owner
is clear.

## Workspace shape

| Path | Responsibility |
|---|---|
| `crates/concord-core` | managed runtime, public semantic contracts, finite behavior, transport boundary |
| `crates/concord-macros` | declaration grammar, semantic validation, generated Rust |
| `crates/concord-extras` | optional first-party implementations of core contracts |
| `crates/concord` | application façade, feature forwarding, macro handoff, approved defaults |
| `crates/concord-examples` | canonical complete examples and deterministic usage proofs |
| `crates/concord-test-support` | scripted and loopback infrastructure shared by tests |
| `tools/xtask` | repository policy, docs, fixtures, MSRV, packages, and source archives |
| `tools/perf` | observational public-surface benchmarks and expansion-scale tooling |
| `fixtures` | isolated Cargo graphs for feature, dependency, generated-contract, and MSRV proofs |
| `docs` | the single mdBook source tree |

The detailed dependency direction and reasons for each boundary are in
[Crate boundaries](architecture/crate-boundaries.md).

## Two Cargo graphs

The root `Cargo.toml` lists product, support, example, and tool packages under
resolver version 3. Product crates and examples are default members; support
and tools remain workspace members so the complete validation matrix can
select them explicitly.

`fixtures/` is excluded from the root workspace and has its own
`fixtures/Cargo.toml` and `fixtures/Cargo.lock`. That separate graph is the
behavior under test. It can rename the façade, combine isolated features, or
substitute an incompatible generated-contract owner without contaminating
ordinary dependency resolution.

Only the root and fixture graphs own lockfiles. Do not add package-local
lockfiles. Cargo-spawning xtask commands should be run serially on Windows to
avoid competing for the same target and package-cache locks.

## Toolchain and compatibility

`rust-toolchain.toml` selects the current development toolchain and the
rustfmt, Clippy, and rust-analyzer components. Workspace package metadata
declares Rust 1.85 as the MSRV. These roles are distinct:

- current stable owns formatting, Clippy, exact Trybuild snapshots, ordinary
  tests, docs, packages, and analyzer output;
- Rust 1.85 owns product-library compilation and semantic compiler-JSON
  assertions;
- fixtures own behaviors that require a separate Cargo graph.

Do not update exact compiler diagnostics from an MSRV rendering. Exact UI
output belongs to the pinned current toolchain; the MSRV checks assert stable
meaning and user-facing spans. See
[Compatibility and MSRV](../reference/compatibility.md).

## Choose the change owner

Use behavior, not convenience, to select the first package:

| Change | Begin with |
|---|---|
| declaration syntax, inheritance, generated naming, static rejection | `concord-macros` |
| lifecycle order, bounds, redaction, stable semantic traits | `concord-core` |
| JSON, OAuth2, records codecs, local limiting, reusable helpers | `concord-extras` |
| public paths, feature forwarding, generated/default composition | `concord` |
| application narrative or complete compiled composition | `concord-examples` |
| deterministic test capability shared by packages | `concord-test-support` |
| repository graph, docs, policy, package, or archive orchestration | `tools/xtask` |
| benchmark scenario or expansion measurement | `tools/perf` |

A change can cross several packages, but ownership must remain singular at each
layer. For example, a new declaration clause is parsed and validated by the
macro crate, represented in generated code, executed by core, re-exported by
the façade, demonstrated by examples, and documented once. It is not six
independent implementations.

## Feature composition

Features are additive Cargo capabilities. The façade forwards optional product
surfaces, core owns feature-independent semantics, and extras provides concrete
implementations. Do not make macros depend on extras or let a default feature
become a hidden requirement of the core trait contract.

Every optional capability needs deliberate proof in three states where
relevant:

1. absent under `--no-default-features`;
2. present under its narrow selected feature;
3. composed under `--all-features`.

The exact current inventory is generated in
[Feature flags](../reference/features.md). Design rules are in
[Feature composition and evolution](architecture/feature-composition.md).

## Maintained commands

`just --list` is the runnable command inventory. During development, start with
the narrow package and test owner, then use the maintained gates:

```text
just fmt
just check
just lint
just test
just ui
just msrv
just fixtures
just docs
just package
just analyzer
just bench
just verify
```

Do not infer success from a workflow definition. Local commands establish only
the local platform and installed-tool results that were actually observed;
remote CI, other operating systems, registry state, and publication authority
remain separate evidence.

## Source and documentation ownership

Rustdoc is the exact Rust item authority. The book teaches tasks, contracts,
architecture, and maintenance. Code paths outside `docs/` are written as
code-formatted text, not fragile Markdown links. Canonical complete examples
live in `crates/concord-examples` and are included into the book through named
regions.

When a fact changes, update its single owner:

| Fact | Canonical owner |
|---|---|
| declaration grammar | `docs/reference/dsl.md` plus compiler tests |
| generated public shape | `docs/reference/generated-api.md` plus façade proofs |
| exact feature inventory | façade manifest plus generated reference fragment |
| exact numeric limits | linked core/extras constants plus generated reference fragment |
| error categories | core Rustdoc and `docs/reference/errors.md` |
| runtime ordering | core executor tests and managed-executor architecture |
| commands | `justfile` and the owning xtask |
| CI execution | `.github/workflows/ci.yml` |
| package graph | root and fixture manifests |

## Working-tree discipline

Preserve unrelated changes. Inspect `git status --porcelain=v1
--untracked-files=all` before editing and before every commit. Stage explicit
paths so an example, snapshot, generated fragment, or temporary review file
cannot enter a change accidentally.

Generated outputs under `target/`, local rendered screenshots, Trybuild `wip/`,
temporary migration notes, and execution prompts are not source artifacts.
Never solve a failing proof by committing one of those files.

## Related

- [Adding a feature](adding-a-feature.md)
- [Testing strategy](testing.md)
- [Documentation maintenance](documentation.md)
- [Release process](release.md)
