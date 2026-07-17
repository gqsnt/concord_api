# CONCORD_REPOSITORY_GUIDE
**Version:** 0.1  
**Status:** Informative contributor guide  
**Authority:** Repository workflow and contribution discipline  
**Depends on:** Specification Index, Conformance Plan, Implementation Program 0.2

## 1. Purpose

This guide explains how contributors should work in the Concord 0.1 repository without breaking the specification-first architecture.

The normative specifications define product behavior. This guide defines repository practice.

## 2. Workspace responsibilities

### `concord_core`

Runtime, stable public Rust API, generated-private integration, and deterministic development boundary.

### `concord_macros`

DSL compiler only. No runtime dependencies or execution behavior.

### `concord_test_support`

Downstream deterministic test helpers. It must not become a production transport extension.

### `concord_examples`

Compile-checked, deterministic application examples.

### `perf`

Local-overhead tests and benchmarks. It must not add dependencies to the normal application graph.

## 3. Dependency rules

1. `concord_core` does not depend on `concord_macros`.
2. `concord_macros` does not depend on `concord_core` runtime libraries.
3. Generated Rust may reference documented core public items and `__private` integration only.
4. Core feature modules do not depend on generated client types.
5. Authentication does not depend on pagination or rate-limit implementation internals.
6. Rate limiting does not depend on auth slot state.
7. Codecs do not depend on client or Reqwest request objects.
8. `concord_core` and `concord_macros` normal/build graphs have no direct `hyper`, `hyper-util`, `tower`, or `h2` dependency. An isolated deterministic upstream-contract probe may use `h2` as a dev dependency under ADR-039; it never enters published runtime/generated code or public APIs.
9. `concord_macros` resolves renamed core dependencies through `proc_macro_crate` or an equivalent reviewed mechanism; generated code never hardcodes the downstream crate name.

A dependency-rule exception requires an accepted decision entry.

## 4. Specification-first workflow

Before implementing or changing behavior:

1. Identify the authoritative normative document using the Specification Index.
2. Add or modify the requirement ID.
3. Manually review normative ownership, requirement language, and dependency order.
4. Update the requirement coverage plan.
5. Add a failing test or another exact implementation evidence target.
6. Update architecture when internal ownership changes.
7. Record an ADR when the change affects scope or architectural philosophy.
8. Implement the minimum code satisfying the requirement.
9. Run the exact evidence artifact and record its result in the conformance-evidence catalog.
10. Update examples and Rustdoc.

Do not implement behavior first and document it later.

## 5. Adding a DSL feature

A DSL feature requires:

1. grammar in `CONCORD_DSL_SPEC_0.1`;
2. semantic composition rules;
3. exact generated API in `CONCORD_GENERATED_API_SPEC_0.1`;
4. public core support when new types are needed;
5. execution/security requirements where applicable;
6. parser unit tests;
7. semantic tests;
8. compile-pass and compile-fail fixtures;
9. generated runtime test;
10. example and Rustdoc.

Implementation order:

```text
spec → parser test → parser → semantic test → resolver/IR
→ compile fixture → codegen → runtime conformance
```

### DSL design rules

- Prefer direct declarative concepts over generic profiles.
- Avoid arbitrary Rust expressions in request shaping.
- Reject ambiguity rather than infer surprising behavior.
- Keep punctuation regular.
- Keep configuration before children.
- Preserve one authoritative syntax for one behavior.
- Keep client-level public values fixed after construction; use scopes/endpoints for per-operation variability.
- Custom pagination uses an atomic factory and safe query/header patch, never `Default` plus public field writes.

## 6. Adding a public extension point

A new extension point requires evidence that existing extensions cannot represent the need cleanly.

The proposal must answer:

- What one behavior does the extension supply?
- When is it called?
- What exact data does it receive?
- Can calls overlap?
- Can the future/callback be canceled?
- Which data is sensitive?
- Which errors are application-visible?
- What must the extension bound itself?
- Which runtime guarantees remain enforced?

A new extension must not expose:

- native Reqwest requests/responses;
- transport replacement;
- execution-phase reordering;
- credential versions;
- body bytes unrelated to its function;
- private request or response plans.

Required work:

1. update Public API Specification;
2. update Security and Limits Specification;
3. update Execution Specification if lifecycle changes;
4. add downstream public-extension fixture;
5. add cancellation, concurrency, and redaction tests;
6. add public Rustdoc.

## 7. Adding a dangerous feature

A dangerous feature requires:

- explicit Cargo feature;
- disabled-by-default status;
- separate dangerous namespace;
- documented reduced guarantees;
- independent feature-boundary tests;
- security-model update;
- a decision record;
- no silent activation through another feature.

Dangerous APIs must still preserve guarantees not explicitly reduced.

## 8. Public API review

Every pull request changing a public item should include a public API diff or equivalent review output.

Review questions:

1. Is the item in the correct namespace?
2. Is it required by a documented workflow?
3. Can the capability be implemented privately instead?
4. Does the signature impose unnecessary `Clone`, `Send`, `Sync`, or `'static` bounds?
5. Does it expose an implementation adapter?
6. Is cancellation behavior documented?
7. Are errors safe?
8. Is the feature gate correct?
9. Does downstream compile evidence exist?

## 9. `__private` rules

`__private` may contain only items required for generated Rust to call core behavior.

Rules:

- no application examples import it;
- no advanced documentation references it except to state instability;
- every breaking change updates the generated contract marker;
- items are minimal adapters, not duplicate public frameworks;
- generated code should not construct large private plan structures field-by-field when a narrow builder can hide them.

## 10. `__development` rules

`__development` is for deterministic tests only.

It must not:

- appear in generated client type parameters;
- add production constructor arguments;
- bypass TLS capability checks;
- expose body or credential values in ordinary captures;
- become a generic mock transport for production applications.

Every development API is considered unstable.

## 11. Error and redaction rules

Before adding an error field, ask whether it may contain:

- URL data;
- headers;
- body bytes;
- credentials;
- provider data;
- cursor;
- rate-limit key;
- proxy configuration.

Rules:

1. Reduce native errors immediately.
2. Store sanitized status headers only.
3. Store a terminal non-success body only through the finite complete-only `StatusBody` contract. Never store or expose truncated prefixes, unbounded bodies, or body bytes in automatic diagnostics.
4. Never include codec input/output in messages.
5. Never include rate-limit key values in built-in errors.
6. Never include cursor/request identity in pagination errors.
7. Custom safe messages are explicitly application-owned.
8. Add a sentinel test for every new data-bearing error path.

## 12. Locking and async rules

- No lock across await.
- No user extension callback under a core lock.
- No provider call under a credential lock.
- No limiter sleep under its state lock.
- Prefer ownership over `Mutex<Option<T>>` for one-shot values.
- Cancellation must be handled by ownership/guards, not detached cleanup tasks.
- Avoid spawning background tasks unless a normative requirement requires work to outlive the caller.

A concurrency change requires deterministic race tests. Credential-slot transition changes additionally require Loom, Shuttle, or equivalent bounded interleaving evidence.

## 13. Test placement

### Parser/semantic unit tests

Live beside macro compiler modules or in focused test modules.

### Trybuild

Use for source-only public core/generated API failures and intentional diagnostic behavior. This
includes private-item access, absent methods, forbidden trait surfaces, raw owner/builder escape,
and generated client generic/constructor escape. Keep each expected diagnostic focused on the
Concord error under test. Trybuild suites use ordinary Nextest timeout policy.

### Downstream fixtures

Use a standalone Cargo project only when a distinct manifest is part of the contract. Dependency
renaming, a missing runtime dependency, an incompatible macro/core package, package metadata, and
Cargo feature selection justify standalone projects. Source-only visibility, privacy, method, and
trait failures do not. Ordinary generated use, nesting, repetition, and trait bounds belong in
normal integration tests or Trybuild.

The retained core `config_features` project owns feature-presence and feature-absence compilation,
including the public JSON marker. The macro `json_disabled` project is retained specifically because
its diagnostic depends on the caller selecting no `concord_core/json` feature; ordinary Text and
custom-codec generation remains in normal integration tests.

The retained core `development_features` project owns only development-feature presence, absence,
and generated-client compilation under that feature graph. Attempts to reach development-only
items through stable namespaces are source-only failures and belong in the core Trybuild suites;
the dangerous-namespace case runs with `dangerous-custom-retry` enabled so that namespace exists
without enabling `dangerous-dev-tools`.

Rendered public Rustdoc is an artifact contract rather than a source-only failure. A focused
integration test may launch isolated `cargo rustdoc` when it inspects the resulting HTML for public
terminal semantics and private-name exclusion. Such a test receives the bounded Cargo-process
Nextest override; it does not justify a standalone fixture manifest.

### Runtime deterministic tests

Use for execution ordering, auth, limiter, response, and pagination behavior.

### Property tests

Use for invariants across large input spaces.

### Model checking

Use Loom, Shuttle, or an equivalent bounded scheduler for the reduced credential transition core. Keep provider HTTP and full Tokio integration in deterministic integration tests.

### Feature-boundary tests

Keep isolated so Cargo feature unification does not hide missing-feature behavior.

## 14. Deterministic test conventions

- Use channels, barriers, and explicit gates.
- Use bounded timeouts only as deadlock protection, not as the primary synchronization mechanism.
- Avoid arbitrary sleeps.
- Script application and provider traffic separately.
- Inspect safe captures by default.
- Use explicit fake values for protected-value comparisons.
- Ensure mismatch diagnostics remain redacted.
- Set Nextest retries to zero.
- Run Rust unit and integration tests through Nextest. Use `cargo test` only for doctests.
- Never spawn `cargo test` from another Rust test. A nested Cargo check/build/tree/rustdoc process
  is permitted only when manifest metadata, dependency topology, feature selection, or a rendered
  Rustdoc artifact is the subject of the test. Source-only compile-fail behavior belongs in
  Trybuild.

## 15. Prohibited test patterns

Do not rely solely on:

- private type names;
- source-text searches;
- exact module paths;
- mutex types;
- enum layouts;
- full generated token snapshots;
- implementation helper ordering;
- test sleeps that guess scheduling.

Test public compilation or observable behavior instead.

## 16. Documentation rules

- Every stable public item has Rustdoc.
- Every feature-gated example names its feature.
- Normative behavior is not defined only in README or examples.
- Cross-document summaries cite requirement IDs.
- Every normative design question is resolved and every normative section is complete before final release.
- Every independently testable normative statement has a stable requirement ID.
- Declared normative document dependencies remain acyclic.
- Canonical examples must compile.
- Dangerous warnings appear at the method/type and feature documentation level.

## 17. Code organization rules

- Files have one primary responsibility.
- Split files before they become multi-subsystem orchestration containers.
- Avoid generic modules named `utils`, `common`, or `context` unless their scope is narrow and obvious.
- Keep public types near their public documentation and tests.
- Keep private generated integration in `__private` or focused generated modules.
- Keep feature-specific code behind module-level `cfg`, not scattered branch conditions when avoidable.

## 18. Unsafe code

Unsafe code is forbidden in core and macro crates.

A proposal to introduce unsafe code requires:

- a contract-level need;
- an accepted ADR;
- isolation in a narrow crate/module;
- complete safety documentation;
- Miri or equivalent evidence where applicable.

## 19. Dependency changes

A dependency proposal must state:

- functionality provided;
- why standard library/current dependencies are insufficient;
- feature impact;
- MSRV impact;
- binary/compile-time impact;
- security/advisory posture;
- whether it duplicates Reqwest internals.

Wildcard, unknown registry, and unreviewed Git dependencies are prohibited.

## 20. Pull request checklist

A behavior-changing PR should include:

- [ ] requirement IDs changed or added;
- [ ] coverage-plan rows updated;
- [ ] actual evidence rows added or updated only for implemented artifacts;
- [ ] normative ownership, normative prose, and dependency order manually reviewed;
- [ ] coverage-plan and conformance-evidence mappings manually reviewed;
- [ ] failing test added before implementation;
- [ ] focused tests pass;
- [ ] default and relevant feature checks pass;
- [ ] public API diff reviewed;
- [ ] redaction/security review completed;
- [ ] concurrency/cancellation reviewed;
- [ ] credential transition changes pass model checking;
- [ ] docs and examples updated;
- [ ] no temporary public API remains;
- [ ] ADR updated when architectural.

## 21. Contributor commands

Canonical commands:

```bash
just fmt
just fmt-check
just check
just check-all
just check-no-default
just clippy
just clippy-all
just test
just test-all
just test-no-default
just doctest
just doctest-all
just doc
just doc-all
just deny
just diff-check
just release
```

`just test` is the local default-feature Nextest run. `just test-all` uses the CI Nextest profile;
`just test-no-default` directly selects the private no-default managed HTTP/TLS contract. Nextest
is required for unit and integration tests, while the doctest recipes are the only maintained
`cargo test` uses. The default/all/no-default checks are deliberately separate feature surfaces.
`just release` also runs the dangerous-namespace Trybuild harness with
`dangerous-custom-retry` enabled and `dangerous-dev-tools` disabled. This keeps the existing
dangerous namespace present while proving that development-only execution support cannot escape
through it; the proof must not be replaced by an all-feature run.

Manual specification governance consists of normative-document review, coverage-plan review, conformance-evidence review, and release audit. Maintained command equivalents remain documented in the Conformance Plan and `justfile`.

## 22. Release checklist

Before release:

1. verify normative documents retain the status appropriate to the release stage. `Normative Release Final` requires complete passing or approved waived conformance evidence; the current status remains `Normative Implementation Candidate`;
2. complete and record normative-document, coverage-plan, and conformance-evidence review;
3. audit and run the release command;
4. review feature graph;
5. review public API diff;
6. review generated Rustdoc;
7. review all changed compile-fail diagnostics;
8. run security sentinel matrix;
9. run credential model checking;
10. compile examples and benchmarks;
11. verify no private API appears in application docs;
12. run `git diff --check`;
13. tag specification and crates together.

## 23. Guiding principle

When choosing between a general abstraction and a narrow feature contract, prefer the narrow contract.

A good Concord unit should answer clearly:

- what does it do;
- when is it called;
- what does it depend on;
- what data can it see;
- how is it tested;
- can its implementation change without affecting consumers?

If those answers require reading unrelated subsystems, the boundary should be redesigned before merging.
