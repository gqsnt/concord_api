# Testing strategy

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
just site           # validate and build the complete mdBook site
just site-serve     # validate once and serve the site with file watching
just docs           # site, doctests, and warning-denied rustdoc
just package        # product archives and extracted-manifest verification
just deny           # dependency and license policy
just workflow       # actionlint for the GitHub Actions workflow
just analyzer       # structural analysis and diagnostics
just bench          # observational public-lifecycle benchmark compilation
just verify         # every required local owner, including workflow lint, exactly once
just source-archive # fresh-clone-safe tracked source ZIP built by git archive
```

The documentation recipe owns warning-denied no-default Rustdoc for each
public product layer, selected façade/extras Rustdoc for `rate-limit`, selected
façade Rustdoc for `csv` and `json`, and all-feature Rustdoc. All-feature
workspace doctests remain the comprehensive executable documentation owner;
selected feature builds are added only when an example itself is
feature-sensitive.

The repository-level subprocess owners are also directly available as
`cargo run --locked -p concord-xtask -- fixtures --locked` and
`cargo run --locked -p concord-xtask -- msrv-ui --locked`. The source artifact
owner is `cargo run --locked -p concord-xtask -- source-archive`; it creates
`target/` when needed and writes `target/concord-source.zip`. Run Cargo-spawning
xtask commands serially on Windows.

Clippy, cargo-deny, and explicitly selected toolchains must be installed for
their recipes. Documentation builds require mdBook 0.5.4, installed with
`cargo install mdbook --version 0.5.4 --locked`. Never report a missing
tool/component as a passing check.

## Executable CI

`.github/workflows/ci.yml` runs on pushes, pull requests, and manual dispatch.
Eight jobs own quality/policy, tests/examples, documentation, stable UI, MSRV,
fixtures/packages, three-platform smoke coverage, and blocking rust-analyzer
checks. The Documentation job installs mdBook 0.5.4, validates and builds the
site, runs all-feature doctests, and generates warning-denied Rustdoc across
the maintained feature sample. Lifecycle, security, extras, and feature proofs
are not repeated in separate jobs after their workspace owner has run.
Performance timing remains manual/local; benchmark compilation is a local
gate.

All third-party actions are pinned to immutable revisions. `actionlint`
checks workflow syntax; xtask policy checks dependency direction and other
repository structure without parsing the workflow or Justfile. A checked-in
workflow is not evidence that a remote run passed.

## Determinism

Ordinary Rust tests do not launch Cargo or rustdoc. Repository subprocess
proofs belong to xtask. No test calls an external service or uses arbitrary
sleeps. Use `concord-test-support` scripted/loopback transport, paused Tokio
time, barriers, and exact event/attempt assertions. Verify first-excess
withholding for every finite surface. Cancellation tests assert no detached
correctness work remains.

Hardening proofs stay with the lowest semantic owner:

- macro semantic/UI tests own statically knowable authentication-header and
  request-head rejection, normalized DNS/IP host-shaping acceptance, exact
  diagnostics, rate-limit dimension grammar/visibility, effective inherited
  pagination-dependency closure (including final host expressions), exact
  generated `set_*`/`clear_*`/`reset_*` control names and collisions,
  independent absence/default capabilities, legacy bare-control absence on
  all four generated owners, and generated documentation capabilities;
- core unit tests own capped arithmetic, JSON-independent body defense,
  startup retention, reader-buffer reuse, shared response-progress arithmetic,
  streaming/buffered/status/provider empty-chunk totals and fairness,
  managed-request empty-chunk totals, wake behavior, exact-length ordering,
  pagination identity bytes, and lifecycle event order;
- extras unit tests own capped JSON serialization, concrete downstream
  codec/provider behavior, and paused-time local limiter quota/cooldown
  capacity, pruning, transactional admission, poisoning, and object-sharing
  proofs;
- façade integration tests own generated/runtime parity, public namespace
  reachability, generated key construction/client-clone sharing, and custom
  `RequestBodySpec` suppression before loopback transport;
- documentation, fixture, analyzer, package, and archive behavior remains in
  its repository-level command owner.

Exact-limit tests must pair maximum with maximum-plus-one and inspect retained
state, iterator progress, or deterministic events so a late reject cannot
masquerade as first-excess withholding. Error tests use sentinels and inspect
both `Display` and `Debug`.

Ready-empty stream probes use manual polling, upstream poll counters, and an
instrumented waker. Async response collectors additionally use explicit poll
boundaries, yield observation, and paused time for deadline control.
`ByteStream`, buffered success, status capture, and provider tests own
response suppression, the shared total ceiling, terminal redaction, and
unchanged nonempty-byte limits; the private record driver owns shared buffered
and streaming no-spin, pending/staging, large-source-chunk, and first
logical-record excess proofs. Managed request-stream tests own suppression
before native transport,
the total/per-poll contract, source drop, body-factory/multipart convergence,
and exact-length error ordering. Host-base semantic unit tests own inheritance
and `host off`; façade Trybuild cases own the IPv4/IPv6 user-facing spans and
DNS/IP compile-pass surface.

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

The exact current toolchain pinned by `rust-toolchain.toml` owns Trybuild
snapshots and Clippy. Rust 1.85 owns separate semantic JSON checks that require
compilation failure, user-facing message fragments, the expected primary source
span, and absence of internal bridge names or protected sentinels. The
generated-control MSRV cases also require the `set_*` replacement hint while
the bare method remains absent on their client-builder, scope-façade,
explicit-endpoint, or request-wrapper owner. Exact MSRV rendering is
intentionally not an owner.

`just verify` includes `just workflow` once, but it cannot establish remote
GitHub execution, Linux/macOS behavior from a Windows host, graphical editor
interaction, registry ownership, or legal approval. Record those scopes
separately; never infer them from a local gate.

## Related

- [Workspace and adding a feature](workspace.md)
- [Performance and scale](performance.md)
- [Documentation maintenance](documentation.md)
- [Release process](release.md)
