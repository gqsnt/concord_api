# Evidence and release gates

The Concord v3 unpublished beta (`3.0.0-beta.1`) is judged by executable
evidence, not by prose. The canonical current disposition, including which
gates have passed on which exact commit, is
[`dev_doc/release_gate_status.md`](../dev_doc/release_gate_status.md); this page
explains what the gates are and how evidence is produced.

Two decisions are intentionally separate:

- **beta integration** requires one coherent active language, API, runtime and
  documentation projection, no compatibility path, clean exact-candidate
  evidence, and Windows/Linux verification;
- **stable release and publication** additionally require owner package and
  license authority, final metadata and names, stable-version mechanics, and
  whatever human evidence the owner requires for a public freeze.

Windows and Linux are supported and mandatory. macOS is not currently a supported or release-gated platform.

## Evidence rules

The implementation specification's acceptance matrix (§27) lists stable
obligation IDs (`V3-L*` language and values, `V3-S*` public shaping and privacy,
`V3-E*` bodies, responses, auth and failure truth, `V3-R*` quotas, identity and
runtime projection, `V3-P*` traversal and streams, `V3-O*` tools, docs and final
build). Evidence follows these rules (TEST-01):

- an obligation is evidenced only by tests through the actual generated and
  core public paths; an uncompiled probe, a same-crate mock, printed
  observation, a normalized-token comparison or a model cannot stand in for a
  native regression;
- a stale test is updated to the selected contract and run; a known failing
  assertion is never marked expected and omitted;
- a single test may cover several obligations when its assertions genuinely
  do, but failures must stay locatable;
- every report maps obligations to test names, commands, platforms and exit
  codes.

## Where evidence lives

| Area | Evidence |
|---|---|
| Language, diagnostics and generated names | macro semantic tests (`crates/concord-macros/src/semantic/tests.rs`), the facade UI suites (`crates/concord/tests/ui/{pass,fail}`), `crates/concord/tests/v3_*.rs` |
| Canonical Catalog and application jobs | `crates/concord-examples/tests/catalog_v3.rs`, `tests/application.rs` (TLS loopback) |
| Two-crate downstream contract | `tests/downstream/concord-downstream-api` and `concord-downstream-app` |
| Runtime endpoints | `crates/concord-core/src/request/runtime_endpoint/tests.rs`, `crates/concord/tests/runtime/` |
| Riot V3 contract and rate-state relations | `crates/concord-examples/tests/riot_contract.rs`, `tests/baseline_recording.rs`, `fixtures/contracts/` |
| Contract format, check and diff | `crates/concord-inspect` tests and `concord-xtask contract` |
| V3-only language and tooling boundary | rejected-form UI fixtures; `tools/xtask/tests/v3_cli.rs`; [version policy](version_policy.md) |
| Documentation truth | `cargo run -p concord-xtask -- docs-check` and its unit tests; the plan tables in `docs/requests_and_values.md` are asserted by `crates/concord-examples/src/docs_spec.rs` tests |
| Rendered rustdoc | `cargo run -p concord-xtask -- doc-audit` renders the canonical Catalog (and a deprecation fixture) with rustdoc and checks the visible caller contract text (V3-O01) |

## Documentation is a tested product surface

`docs-check` (§19.2, DOC-01, V3-O05) fails when:

- a Rust-highlighted DSL block (marked `concord-dsl` in its Markdown source)
  is not a normalized excerpt of the compiled
  `crates/concord-examples/src/docs_spec.rs`, or an anchored excerpt of a
  compiled example;
- a Rust block is not an anchored excerpt of compiled code, or an anchored
  excerpt drifted from its source;
- a local link or heading fragment is broken, or `docs/README.md` does not link
  every document exactly once (including the V3-only version policy);
- the tutorial shows unbound, runtime or control internals before an ordinary
  awaited result;
- user documentation uses a retired v2 form outside an explicit migration
  context, or names a retired version as current;
- the grammar in `docs/spec.md` does not quote every keyword the parser
  declares, a parser fallback diagnostic does not list every legal clause, or
  generated value/credential control names disagree with the macro;
- a retired v2 surface is not rejected by a pinned UI fixture;
- the facade does not re-export the §7.2 root names, or the README feature
  matrix disagrees with the facade manifest;
- the contract tooling usage in
  `docs/planning_and_observation.md` drifted;
- release banners disagree with the workspace version.

Generated rustdoc is checked by running rustdoc (`just docs` includes
`cargo rustdoc -- -D warnings` across feature sets) and by a macro unit test
that inspects the documentation attached to a generated Catalog method.

## Gates

**Gate 0: package identity, legal authority and release namespace.** Explicit
license policy, approved and viable public package names, official metadata,
final archive, downstream, rustdoc and `cargo deny` evidence. Mandatory for
stable release and publication; it does not block a local unpublished beta
while every intended public package keeps `publish = false`. Only the project
owner can supply the legal and product decisions.

**Gate 1: language and generated surface** (`V3-L01`-`V3-L16`). Both canonical
declarations compile with the §22.3 signatures; local-name, placement,
normalization, value-control, initialization, conversion, clone, unbound and
liveness rules hold; downstream named requests implement `RequestControls`
themselves in two real crates.

**Gate 2: shaping and privacy** (`V3-S01`-`V3-S12`). Patch laws and order,
suppressed-encoder non-evaluation, templates and DNS labels, header ownership,
sticky sensitivity and a sentinel corpus absent from every automatic surface,
first-error latching, and planning without body, credential, limiter or
transport work.

**Gate 3: bodies, responses, auth and failure truth** (`V3-E01`-`V3-E21`).
Truthful dispatch knowledge at every boundary, body preparation and replay,
multipart boundaries, head-only versus buffered absence, complete-only status
bodies, credential ordering under concurrency, provider origin enforcement,
replay authorities, retry budget and server delay, observer containment.

**Gate 4: quotas, identity and runtime projection** (`V3-R01`-`V3-R13`). All
46 Riot endpoint plans and their declared sharing groups preserved, label and schema continuity,
atomic admission, local limiter accounting, runtime owner projection with
plan/send agreement, and typed rate-key encoding.

**Gate 5: traversal, streams, tools and final build** (`V3-P01`-`V3-P11`,
`V3-O01`-`V3-O08`). Controllers, initial-cursor and cycle checks, zero-page
and bound semantics, aggregate deadlines and budgets, cancellation, stream
leases and cancel-safe reads; generated docs, snapshot and diff tooling,
V3 contract fixtures, docs/feature/anchor alignment, the platform matrix,
measurements and a delivery audit.

**Gate 6: user exercises and documentation review.** The owner requested agent
execution on 17 October 2026; the [follow-up](../dev_doc/reviews/2026_10_17_followup/README.md)
records the actual exercise answers, native checks and independent agent
Rustdoc/diff review. This completes the requested technical evaluation. The
retained human-study protocol under `dev_doc/dx_study/` is a separate possible
empirical study; its participant metrics are unmeasured.

## Repository verification

Run where available and record every exit status:

```text
just fmt
just check
just lint
just test
just ui
just msrv
just fixtures
just docs
just docs-check
just package
just deny
just workflow
just analyzer
just bench
just source-export
just release-audit
just verify
just verify-implementation
```

`just verify-implementation` runs the machine validation owned by the
unpublished beta implementation (format, the check/clippy/test matrix, the
explicitly named UI suites, MSRV, fixtures, docs and `doc-audit`, the two-crate
downstream workspace, extracted public archives and their two-crate consumer
proof, xtask tools, benchmark compilation and the source export).
The `package` gate checks local archives, normalized manifests, Rustdoc and
downstream behavior; it does not publish them. Dependency, registry, tag and publication decisions stay in the separate
release report; the selected project license is checked in repository policy
and in the extracted archives.

Individual gates can be run directly, for example
`cargo test --workspace --all-features`, `cargo test --workspace --doc --all-features`
and `cargo run -p concord-xtask -- docs-check`. A pipeline that logs a failure
and still exits zero is not an acceptance gate; do not claim unavailable or
unfinished checks passed. The MSRV (Rust 1.86) and the pinned toolchain checks
run only where those toolchains are installed.

## Decisions

The beta integration decision passes only when its technical, documentation,
no-retrocompatibility, Windows, Linux, exact-branch and exact-merge checks pass
with a clean tree at `3.0.0-beta.1` and `publish = false`.

A stable release, tags, final public names and publication remain blocked until
Gate 0 and the owner-selected human-freeze policy pass. A blocked stable gate is
not solved by compatibility aliases, prose without evidence, or an unauthorized
waiver.
