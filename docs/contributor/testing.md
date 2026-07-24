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
| `tools/xtask` | Cargo-graph, semantic compiler-JSON, rustdoc, policy, and package orchestration |

## Maintained commands

`just --list` is the command inventory. Primary recipes are:

```text
just fmt          # format
just check        # default and no-default checks
just check-all    # all-feature check
just lint         # rustfmt + Clippy with warnings denied
just test         # stable workspace tests
just test-all     # all-feature workspace tests
just test-msrv    # Rust 1.85.0 checks plus semantic JSON diagnostics
just ui           # stable exact façade/core compile-pass and compile-fail cases
just fixtures     # one locked root fixture workspace
just examples     # canonical example modules and local tests
just docs         # generated tables, links, doctests, warning-denied rustdoc
just security     # redaction, bounds, lifecycle, planning matrix
just policy       # parsed recipe/workflow/action policy
just package      # local product archives and extracted-manifest verification
just perf-smoke   # observational operation-exact probe
just verify-local         # reproducible stable checks on this host
just verify-msrv          # Rust 1.85 compilation and semantic UI
just verify-package       # technical local package proof
just verify-release-local # all local scopes, deny, and observational performance
```

The repository-level subprocess owners are also directly available as
`cargo run --locked -p concord-xtask -- fixtures --locked` and
`cargo run --locked -p concord-xtask -- msrv-ui --locked`. Run
Cargo-spawning xtask commands serially on Windows.

Clippy, cargo-deny, and explicitly selected toolchains must be installed for
their recipes. Never report a missing tool/component as a passing check.

## Executable CI

`.github/workflows/ci.yml` runs on pushes, pull requests, and manual dispatch.
Its explicit jobs own formatting/metadata, feature checks, Clippy, workspace
tests, lifecycle/security/bounds, stable exact UI, MSRV semantic UI, extras,
fixtures, examples/docs, dependency/workflow policy, product packages, the
three-platform smoke matrix, and observational performance. Failed UI, MSRV,
and package lanes retain bounded command logs. The final `release_clean` job
reruns documentation generation and rejects tracked diffs or untracked files.

All third-party actions are pinned to immutable revisions. `actionlint` checks
workflow syntax in CI; `just policy` independently checks required job IDs,
action pins, timeouts, locked command ownership, cache-key facts, and the
release-clean commands. A checked-in workflow is not evidence that a remote
run passed.

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

`verify-local` cannot establish remote GitHub execution, Linux/macOS behavior
from a Windows host, interactive completion/navigation, registry ownership, or
legal approval. Record those scopes separately; never infer them from a local
gate.
