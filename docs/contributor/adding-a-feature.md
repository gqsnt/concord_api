# Adding a feature

Add a Concord capability by following its ownership from semantic contract to
application surface. The smallest correct change is often distributed across
packages, tests, examples, and documentation, but each part should exist
because a named contract requires it.

## 1. Classify the capability

Decide which category describes the change:

| Category | Typical owners |
|---|---|
| declaration-only validation or diagnostic | macro parser/resolver, UI tests, DSL reference |
| generated application surface | macro derivation/emission, façade generated tests, generated API reference |
| semantic runtime capability | core public contract, executor, public API and lifecycle tests |
| concrete reusable implementation | extras feature, unit tests, façade re-export |
| transport configuration | core HTTP boundary, configuration tests, production HTTP guide |
| documentation/tooling policy | xtask, Justfile or CI, maintenance documentation |

If the proposed implementation needs native transport only because an existing
semantic trait lacks useful context, review that missing context first. Do not
create an extras-only private escape route.

## 2. State the contract before coding

Write down:

- public task and default path;
- compile-time versus runtime responsibility;
- effect ordering relative to hooks, authentication, limiting, transport,
  status, response acquisition, and completion;
- finite limits and first-excess behavior;
- sensitive inputs and safe error reduction;
- deadline, cancellation, overlap, and sharing behavior;
- no-default, selected-feature, all-feature, and MSRV expectations;
- canonical example and documentation owner.

For a runtime contract, use the structure applied in
[Runtime contracts](contracts/extensions.md): purpose, public model, internal
flow, invariants, bounds, errors/redaction, extension boundary, failure modes,
change hazards, and verification.

## 3. Implement at the lowest semantic owner

Keep dependency direction intact:

1. add or evolve the core semantic trait/value when the lifecycle itself needs
   a capability;
2. implement concrete downstream policy in extras when the public contract is
   already sufficient;
3. add compiler grammar and generated binding only when declarations must
   express the capability;
4. forward approved modules and features through the façade;
5. avoid application-specific helpers in core and native transport knowledge
   in macros or extras.

Core must validate downstream results before they reach native state or
retained storage. Generated code constructs typed semantic inputs; it must not
grow a second executor.

## 4. Decide whether the generated contract changes

The private compiler/runtime protocol changes only when emitted Rust and the
façade/core bridge must agree on a new operation or shape. A public trait
addition that generated code never names may not require a bump. A renamed
private helper, changed argument contract, or new generated call usually does.

Use the decision tree and atomic owner list in
[Generated contract versioning](generated-contract.md). When a bump is
required, update macro emission, façade/core owners, the expected version, and
the isolated mismatch fixture in one coherent change.

## 5. Define features and defaults deliberately

Optional implementation dependencies belong behind the narrowest feature.
Update:

- package feature declarations and dependency gates;
- façade feature forwarding and default membership, when approved;
- `cfg`-gated public exports;
- no-default absence fixtures;
- selected-feature and all-feature compilation/Rustdoc;
- the generated feature reference through the docs validator.

Do not make core semantics disappear with a concrete implementation feature.
A custom trait surface should remain usable when the built-in implementation
is disabled.

## 6. Build the proof matrix with the feature

Place each proof with the lowest owner:

| Behavior | Proof owner |
|---|---|
| parser/resolver rule | macro unit test |
| exact current diagnostic | façade/core Trybuild |
| Rust 1.86.0 diagnostic meaning | xtask semantic compiler-JSON fixture |
| runtime ordering and cancellation | core executor test |
| bound and first-excess state | owning core/extras unit test |
| public reachability/privacy | public API or compile-fail test |
| generated/runtime parity | façade integration test |
| downstream trait sufficiency | extras or façade extension test |
| isolated Cargo feature/contract | fixture graph |
| complete usage | examples crate test |

Use deterministic scripted or loopback transport, paused time, bounded
iterators, barriers, and manual polling. Do not add external-network tests or
arbitrary sleeps. A finite-limit test needs exact-boundary and first-excess
cases plus retained-state or iterator-progress assertions.

## 7. Add the canonical example

Put one complete compilable composition in `crates/concord-examples`. Give
book-owned excerpts named source regions and keep execution deterministic with
loopback support where network behavior matters. The example crate and its
tests remain compiler owners; the book includes the same code instead of
copying it.

Add a standalone fixture only when a separate Cargo graph is itself the
behavior. Ordinary compile-pass examples do not belong in fixtures.

## 8. Update one documentation owner per fact

Update only the pages whose responsibility changed:

- a user feature page for the working path, variations, operation, and
  limitation;
- a recipe only when the feature enables a new concrete composition;
- an extension page when downstream implementation work is supported;
- one reference owner for exhaustive syntax, flags, limits, errors, or
  unsupported forms;
- architecture/runtime-contract pages for new invariants, ordering, or change
  hazards;
- Rustdoc for exact items and callback contracts;
- `CHANGELOG.md` for a release-facing capability or compatibility change.

Summarize and link instead of copying an exhaustive table or lifecycle into
several pages. Run the docs generator with `--write` only when a canonical
generated fragment has changed, then verify without mutation.

## 9. Review compatibility and performance

Before calling the change complete, review:

- source compatibility and non-exhaustive enum strategy;
- generated names and collision diagnostics;
- no-default and feature-unification behavior;
- Rust 1.86.0 language and dependency compatibility;
- error category and redaction stability;
- allocation, cloning, state-cardinality, and hot-path work;
- compile-time expansion size when declarations grow;
- package closure and normalized manifests.

Performance tooling is observational. Compare the same machine, toolchain,
profile, and iteration count; do not invent a threshold from a single run.
See [Performance and scale](performance.md).

## 10. Run increasing gates

Run the focused package owners throughout the change. Before commit, run at
least:

```text
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features --locked
cargo nextest run --workspace --exclude concord-macros --lib --tests --examples --all-features
cargo test --workspace --doc --all-features --locked
cargo run --locked -p concord-xtask -- docs --locked
mdbook build
git diff --check
```

Then run every owner the feature affected: examples, default/all-feature
tests, doctests, UI, MSRV, fixtures, Rustdoc, package, analyzer, benchmark
compilation, or source archive as applicable. `just verify` is the maintained
full local composition.

Record failures and unavailable tools precisely. A local pass is not evidence
of remote CI, other platforms, registry ownership, or publication.

## Review checklist

- [ ] One semantic owner is named.
- [ ] Lifecycle ordering and effect ownership are explicit.
- [ ] Bounds stop at first excess and extension-owned work is identified.
- [ ] Sensitive values are absent from `Display`, `Debug`, hooks, and errors.
- [ ] Cancellation, deadlines, overlap, and clone sharing are tested.
- [ ] Generated and runtime-defined endpoints converge where applicable.
- [ ] No-default, selected, all-feature, and MSRV owners are updated.
- [ ] Nextest unit/integration/example owners and separate Cargo doctests pass.
- [ ] Canonical example code compiles and book includes resolve.
- [ ] Rustdoc and one book owner describe each fact.
- [ ] Changelog, package, analyzer, and performance impact were considered.

## Related

- [Workspace and adding a feature](workspace.md)
- [Feature composition and evolution](architecture/feature-composition.md)
- [Testing strategy](testing.md)
- [Documentation maintenance](documentation.md)
