# Testing and validation

The test architecture follows contract ownership.

| Owner | Primary proofs |
|---|---|
| `concord-core` | lifecycle, security/redaction, bounds, transport, sealed modes, cancellation |
| `concord-extras` | downstream trait sufficiency and implementation behavior |
| `concord-macros` | syntax/semantic unit tests and expansion structure |
| `concord` | consolidated generated, runtime, extension, public-API, and UI proof families |
| `concord-examples` | canonical declaration construction, conventional Cargo examples, and deterministic local narratives |
| `fixtures` | renamed-façade, contract-mismatch, feature-isolation, and MSRV Cargo graphs |
| `tools/xtask` | Cargo-graph, semantic compiler-JSON, documentation, policy, and package orchestration |

## Maintained commands

`just --list` is the command inventory. Primary recipes are:

```text
just fmt            # formatting check
just check          # metadata, feature checks, and structural repository policy
just lint           # all-target/all-feature Clippy with warnings denied
just test           # default and all-feature Rust tests
just ui             # stable exact façade/core compiler contracts
just msrv           # Rust 1.85 product libraries and semantic JSON diagnostics
just fixtures       # isolated Cargo-graph contracts
just docs           # generated tables, links, doctests, and warning-denied rustdoc
just package        # product archives and extracted-manifest verification
just deny           # dependency and license policy
just analyzer       # structural analysis and diagnostics
just bench          # observational public-lifecycle benchmark compilation
just verify         # every required local owner exactly once
just source-archive # tracked source ZIP built by git archive
```

The repository-level subprocess owners are also directly available as
`cargo run --locked -p concord-xtask -- fixtures --locked` and
`cargo run --locked -p concord-xtask -- msrv-ui --locked`. Run
Cargo-spawning xtask commands serially on Windows.

Clippy, cargo-deny, and explicitly selected toolchains must be installed for
their recipes. Never report a missing tool/component as a passing check.

## Executable CI

`.github/workflows/ci.yml` runs on pushes, pull requests, and manual dispatch.
Seven jobs own quality, tests/docs, stable UI, MSRV, fixtures/packages,
three-platform smoke coverage, and blocking rust-analyzer checks. Lifecycle,
security, extras, and feature proofs are not repeated in separate jobs after
their workspace owner has run. Performance remains manual/local.

All third-party actions are pinned to immutable revisions. `actionlint`
checks workflow syntax; xtask policy checks dependency direction and other
repository structure without parsing the workflow or Justfile. A checked-in
workflow is not evidence that a remote run passed.

## Determinism

Ordinary Rust tests do not launch Cargo or rustdoc. Repository subprocess
proofs belong to xtask. No test calls an external service or uses arbitrary sleeps. Use
`concord-test-support` scripted/loopback transport, paused Tokio time,
barriers, and exact event/attempt assertions. Verify first-excess withholding
for every finite surface. Cancellation tests assert no detached correctness
work remains.

Canonical tests import declarations from `concord-examples`; they do not copy
them. Executable wrappers live under `crates/concord-examples/examples/`.
Ordinary compile-pass cases live in the package test suite. A standalone
fixture exists only when a separate Cargo graph is the behavior under test.

The inert local examples are ordinary Cargo examples:

```text
cargo run --locked -p concord-examples --example minimal
cargo run --locked -p concord-examples --example records --no-default-features --features csv,ndjson
```

The Riot wrapper intentionally calls a live service and is never part of the
deterministic matrix. Run it only with the documented environment and explicit
opt-in:

```text
cargo run --locked -p concord-examples --example riot --no-default-features --features riot-live
```

## UI and editor protocol

After generated-surface changes:

1. run default and all-feature façade UI suites;
2. run core privacy/sealed-mode UI suites;
3. inspect changed stderr against the public contract;
4. generate warning-denied Rustdoc and inspect signatures;
5. open the workspace root in rust-analyzer with default and all features;
6. confirm ordinary completion needs no generated prelude/trait import;
7. open fixtures separately only when testing their graph.

Stable Rust owns exact Trybuild snapshots. Rust 1.85 owns separate semantic
JSON checks that require compilation failure, user-facing message fragments,
the expected primary source span, and absence of internal bridge names or
protected sentinels. Exact MSRV rendering is intentionally not an owner.

`just verify` cannot establish remote GitHub execution, Linux/macOS behavior
from a Windows host, graphical editor interaction, registry ownership, or legal
approval. Record those scopes separately; never infer them from a local gate.
