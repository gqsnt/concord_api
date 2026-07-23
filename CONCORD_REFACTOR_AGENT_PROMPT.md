# Execution Prompt — Concord Full Architectural Refactor

You are the implementation agent for a complete architectural refactor of the Concord Rust workspace. Act as a senior Rust library architect, proc-macro/compiler engineer, public API designer, security reviewer, test architect, documentation owner, and developer-experience reviewer.

You are working directly in the repository. This is an implementation assignment, not another exploratory review.

## Primary source of truth

A file named `CONCORD_REFACTOR_REFERENCE.md` accompanies this prompt. Place it at the repository root if it is not already there, then read it in full before editing code.

That file is the durable specification for:

- the product thesis;
- selected design decisions;
- crate and feature ownership;
- the target public DSL and generated Rust API;
- core lifecycle and security invariants;
- the `concord_extras` boundary;
- the macro architecture;
- examples, tests, fixtures, documentation, workspace, and tooling;
- the ordered Phase 0–14 execution plan;
- validation requirements;
- bounded deferrals and the final definition of done.

Do not depend on remembered conversation context. At the start of every phase—and after any context compaction, interruption, or handoff—re-read:

1. Sections 0–4 of `CONCORD_REFACTOR_REFERENCE.md`;
2. Section 2, **Selected design decisions**;
3. the complete current phase;
4. Sections 18–20;
5. the progress ledger and all deviation records.

The reference file is authoritative. Update its progress ledger as work proceeds. Do not silently rewrite the target design to match an easier implementation.

---

# Mission

Refactor Concord into a coherent contract compiler and managed HTTP runtime with this user-facing mental model:

```text
Remote HTTP API contract
        ↓
Concord declaration compiler
        ↓
Ordinary, predictable typed Rust API
        ↓
One bounded, redaction-aware managed HTTP lifecycle
```

The final ordinary user experience must require only:

```toml
[dependencies]
concord = { version = "...", features = [/* selected capabilities */] }
```

Canonical code must import from `concord`, not from implementation crates.

The target package graph is:

```text
concord-core      semantic runtime kernel and extension traits
       ▲
       │
concord-extras    downstream implementations of public core traits

concord-macros    proc-macro compiler, extras-agnostic

concord           user façade; reexports/composes core + macros + extras
       ▲
       │
concord-examples  depends only on concord
```

`concord-test-support`, performance tooling, fixtures, and `xtask` remain internal support packages.

---

# Non-negotiable product invariants

Never weaken any of these to simplify code or shorten the refactor:

1. Endpoint and request construction are inert. No HTTP, credentials, limiter, hook, body materialization, body factory invocation, stream polling, or completion work occurs before execution begins.
2. A built client is an immutable runtime snapshot shared safely by clones.
3. Generated endpoints, explicit endpoint values, and runtime-defined endpoints use one managed executor and one lifecycle.
4. Native Reqwest requests/responses and authentication material remain private to core.
5. Safe URL/header projections, hooks, errors, descriptions, plans, `Debug`, and completion events do not expose protected material.
6. Body, response, stream, status-body, pagination, identity, limiter-state, cooldown, and provider work remain centrally bounded.
7. Authentication ordering, rate-limit ordering, challenge recovery, status handling, acquisition, result fixation, and completion remain deterministic.
8. Authentication recovery remains bounded to one additional attempt unless the reference specification explicitly changes it.
9. Pagination remains sequential, bounded, cycle-detecting, and all-or-error.
10. Response acquisition modes remain sealed so extensions cannot introduce unbounded/native response escape.
11. Multipart remains under core’s native-boundary and replayability control.
12. Dangerous capabilities remain explicit, feature-gated, and bounded.
13. Cancellation must not leave detached correctness work.
14. Every extension receives semantic data, not native transport objects.
15. No product-level behavior may rely on undocumented generated implementation details.

When a proposed change conflicts with one of these invariants, preserve the invariant and record a deviation from the lower-priority design detail.

---

# Binding architectural decisions

Implement the detailed decisions in the reference file. The most important are summarized here.

## Crates and dependency direction

- Add the user-facing `concord` façade.
- Add `concord_extras`.
- `concord_extras` depends on `concord_core` only.
- `concord_core` never depends on extras, macros, or façade.
- `concord_macros` never depends on extras or recognizes extra type names.
- Prefer no normal runtime dependency from macros to core; generated semantics are core-owned, but proc-macro compilation should emit against the façade bridge.
- `concord` depends on core, macros, and extras and owns user-facing feature forwarding/default composition.
- `concord_examples` depends on `concord`, not directly on implementation crates.

## Generated path resolution

A downstream crate cannot refer to a transitive `concord_core` dependency merely because it depends on `concord`. Therefore:

- resolve the caller-visible `concord` package with dependency-renaming support;
- emit generated bridge paths through `concord::__private`;
- keep `__private` undocumented and version-checked;
- have the façade delegate semantic operations to core and perform approved feature-dependent composition with extras;
- remove direct-core path support in the final compatibility-removal phase.

## Core ownership

Core owns:

- request/response/body semantic traits;
- primitive codecs/specifications such as byte buffers, `NoContent`, text, media and stream primitives;
- bounded body and response acquisition;
- sealed modes;
- generic records engine;
- generic pagination contracts, loop, limits, identity, and result behavior;
- rate-limit contracts, plans, keys, permits, cooldown context, no-op and fail-closed fallback;
- authentication material, slots, placement, challenge/recovery, provider HTTP;
- multipart native construction and replayability enforcement;
- client runtime snapshot, hooks, errors, redaction, transport, executor, completion;
- the private generated bridge contract.

The current code uses `NoContent`; do not invent a duplicate `NoResponse` merely because informal discussion used that phrase.

## Extras ownership

Move concrete implementations that can be written entirely through public semantic contracts:

- `Json<T>`;
- `Csv<T>`;
- `NdJson<T>`;
- `OffsetPagination`;
- `PagePagination`;
- `CursorPagination<C>`;
- `HasNextCursor<C>`;
- `GovernorRateLimiter`;
- `DefaultRateLimitObserver`;
- `OAuth2ClientCredentials`.

`concord_extras` is not an internal friend crate. It is a first-party downstream proof that Concord’s public extension contracts are sufficient. Do not expose native Reqwest values or broad hidden APIs merely to make extraction convenient.

## Governor extraction

Replace existing private coupling with semantic public contracts:

- a documented limiter preparation-key bound/accessor;
- cooldown cap supplied in the public observation context or equivalent semantic input;
- no governor-specific downcast in core;
- no governor-specific task-local cap;
- core default is fail-closed for non-empty generated plans;
- the façade installs the extras governor when its user-facing feature is enabled;
- an explicitly user-supplied limiter always wins over generated defaults;
- macros do not name or detect the governor.

## OAuth2 extraction

The final macro has no special `oauth2_client` semantic branch and no JSON knowledge.

- OAuth2 client credentials become a normal `CredentialProvider` implementation in extras.
- Users construct it through a named, secret-safe builder and pass it to the generated client builder.
- Preserve HTTPS/token-URL validation, provider HTTP isolation, finite limits, expiry behavior, safe error reduction, redaction, and refresh/reacquisition semantics.
- A temporary legacy parser may lower old syntax to the generic provider model, but Phase 14 removes it.

## Feature ownership

- Core and extras have empty default features.
- The façade owns user-facing defaults and feature documentation.
- Preserve existing ordinary defaults initially: default TLS, HTTP/2, and governor; keep JSON opt-in unless evidence leads to a documented product decision.
- Do not enable all extras by default.
- Core transport/lifecycle features remain core features and are forwarded by façade.
- JSON, CSV, NDJSON, governor, and OAuth2 helper ecosystem dependencies belong outside core.
- Built-in pagination controllers should be featureless unless a concrete dependency proves otherwise.

## Public generated API

Every endpoint must generate:

- one predictable direct method;
- one public explicit endpoint value;
- one named request wrapper;
- one safe endpoint description;
- inherent required capability methods only;
- a predictable scope-oriented module path.

Target properties:

- snake_case endpoint/scope identifiers directly define method/module names;
- deterministic PascalCase types;
- one module tree organized by scopes, not separate public `facades` and `endpoints` trees;
- optional/default setters are inherent;
- pagination, credential storage, response metadata, and dangerous terminals are inherent only when valid;
- no generated per-client prelude is required;
- no generated request extension trait is required;
- invalid operations fail as clear method/capability absence or focused generated diagnostics;
- direct calls and explicit endpoint binding are semantically equivalent;
- endpoint values are conditionally reusable/cloneable where their fields permit it;
- a documented sealed generated endpoint trait supports generic composition through an associated request type and safe description.

Do not use `Deref` or broad generic methods if doing so makes invalid terminals discoverable.

## Client construction

- Generate `Client::new()` only for clients with no required values, secrets, or providers.
- Configured clients use `Client::builder()` with named setters.
- No positional secret/provider constructors in the final API.
- Prefer a normal runtime-validated builder with a focused, secret-safe build error over generated typestate unless compelling evidence is recorded.
- Runtime policy such as custom transport retry, concrete limiter instance, hooks, and operational observers belongs on the client/runtime builder rather than in the remote contract DSL.

## DSL v2

- Named clauses are order-independent.
- Duplicate clauses report both spans.
- List ordering inside paths, query parameters, headers, auth placements, and rate windows remains meaningful.
- Endpoint/scope identifiers are snake_case.
- Body role is explicit with `#[body]` unless a documented deviation proves another explicit syntax better.
- Optionality is syntactic `Option<T>` or `std::option::Option<T>`; do not pretend the proc macro can resolve arbitrary aliases.
- Defaults remain `name: Type = expression`.
- Body/response/provider/controller types remain generic Rust types; the macro does not recognize names such as `Json`, `Csv`, `NdJson`, or built-in controllers.
- Add an optional explicit stable contract ID; generated default identity remains deterministic.
- Remove custom transport retry syntax from the final DSL.

## Runtime endpoint and planning

- Rename `PreparedEndpoint` to `RuntimeEndpoint`.
- Implement a real timeout tri-state: inherit client, explicit duration, disabled.
- Consider/implement a bounded owned endpoint ID as specified.
- Generated and runtime endpoints expose one safe description shape.
- Add a sanitized planning surface that reuses real public-head preparation while causing no hooks, credentials, limiter, body encoding/factory, stream polling, transport, or completion effects.
- Do not expose an exact native/authenticated request preview.
- Do not add partial runtime pagination without a coherent typed contract and the same executor.

---

# Required execution protocol

## 1. Bootstrap without destroying user work

Before edits:

1. Read `CONCORD_REFACTOR_REFERENCE.md` completely.
2. Inspect `git status`, current branch, ignored files, manifests, lockfiles, and toolchain files.
3. Preserve unrelated user changes. Never use destructive reset/clean/checkout commands on work you did not create.
4. Verify the current package graph and feature graph with source inspection and, when available, Cargo metadata/tree.
5. Check whether the project has been published or has downstream compatibility commitments before relying on the `0.1.0` clean-break assumption.
6. Run Phase 0 baseline commands that are available.
7. Record exact results, unavailable tools, and pre-existing failures in the progress ledger.

Do not claim baseline success when Cargo, a toolchain, a target, or network-dependent package acquisition is unavailable.

## 2. Work phase-by-phase

Implement Phases 0 through 14 in order, except that the governor and OAuth extraction subphases may be parallelized only as allowed by the reference dependency graph.

For each phase:

1. Re-read the phase, Sections 2–4, Sections 18–20, and the ledger.
2. State the concrete phase objective internally and identify affected product invariants.
3. Inspect current code/tests before editing; do not assume earlier repository structure after prior phases.
4. Add or update proofs before or with behavior changes.
5. Make small coherent patches with semantic ownership preserved.
6. Run the phase-local validation and relevant cross-phase security/feature proofs.
7. Review the diff for public leakage, duplicated models, dead compatibility code, feature coupling, and documentation drift.
8. Update the phase row in the ledger with state, commands, results, deviations/blockers, and next action.
9. Keep the workspace buildable before moving to the next phase.
10. Re-open the reference before the next phase.

Do not reorganize macro files merely because they are large. Phase 10 restructuring begins only after the grammar, public surface, descriptions, runtime endpoint contract, and hidden bridge are stable.

## 3. Use deviation records correctly

A deviation is allowed only when:

- stable Rust or the declared MSRV makes the selected design impossible;
- compiler evidence reveals an unavoidable dependency cycle;
- implementation evidence shows a selected detail violates a non-negotiable invariant;
- verified publication/downstream commitments require a compatibility path.

For every deviation, add the template from the reference with evidence, alternatives, selected replacement, invariant analysis, and tests. Convenience, file size, or reduced effort is not sufficient.

## 4. Maintain semantic sources of truth

At all times:

- the resolved macro model is the single source for generated names and capabilities;
- direct and explicit endpoint construction share one endpoint model;
- safe planning and execution share public-head construction;
- generated/default rate-limiter selection has one owner;
- the façade manifest is the user feature topology owner;
- exact numeric defaults have one core source;
- examples contain canonical declarations and tests import them;
- docs link to examples rather than copying divergent programs;
- each product invariant has one primary test owner.

Do not create parallel compatibility models that can drift. Temporary aliases must delegate and have a Phase 14 deletion entry.

## 5. Keep security review active

Any phase touching authentication, headers, URL shaping, request planning, hooks, errors, descriptions, debug output, rate-limit identities, body metadata, or native request construction must rerun the complete relevant redaction/security matrix.

Use seeded sentinel values and verify they do not appear in any safe output. Treat accidental exposure as a release blocker.

## 6. Keep compilation/editor experience active

For every generated-surface phase:

- inspect generated signatures and rustdoc;
- write compile-pass and compile-fail cases;
- avoid trait-import requirements for ordinary methods;
- keep diagnostics in public contract vocabulary;
- attach source spans to the relevant DSL identifiers/types;
- measure expansion/item count before accepting broad repetition;
- follow the manual rust-analyzer protocol at the prescribed gates.

## 7. Keep tests deterministic

- No external internet calls.
- No arbitrary sleep-based synchronization.
- Use deterministic executor/scripted transport/loopback support.
- Test first-excess behavior, event ordering, attempt counts, and cancellation explicitly.
- A canonical example test imports the example module; it does not recreate a synthetic API.
- Standalone fixtures exist only for separate Cargo graph assertions.

## 8. Manage compatibility deliberately

The repository is currently version `0.1.0` and appears unpublished in the reviewed snapshot, but verify this.

Use temporary deprecated aliases only to keep intermediate phases green. Record every alias and its removal phase. The final state should not retain:

- direct two-crate usage;
- direct generated `concord_core` paths;
- configured positional constructors;
- generated request extension traits/preludes;
- separate facade/endpoint public trees;
- `PreparedEndpoint` naming;
- fixed clause ordering;
- PascalCase endpoint source identifiers;
- inferred body-by-name behavior;
- macro-special OAuth2/JSON behavior;
- custom retry DSL syntax.

## 9. Do not stop at local code success

The assignment includes:

- crate extraction and façade composition;
- public language and generated API redesign;
- core contract evolution needed by extras;
- macro internals after the public model stabilizes;
- examples reorganization;
- test/fixture consolidation;
- documentation redesign;
- workspace/tooling/CI portability;
- release/migration hardening.

A buildable first subset is not completion. Continue according to the ledger and reference.

---

# Phase sequence and mandatory outcomes

The reference contains the full tasks, risks, tests, and exit criteria. This section is only an execution index.

## Phase 0 — Baseline and contract freeze

- Capture repository/package/feature/test/tooling state.
- Record existing failures and metrics.
- Add the reference file and ledger to the root.
- Establish public-surface snapshots and invariant ownership.
- Make no broad implementation changes.

## Phase 1 — Façade and crate graph

- Add `concord` and `concord_extras` skeletons.
- Establish dependency direction and hidden façade bridge.
- Resolve renamed façade dependencies in generated code.
- Switch a minimal external user test and examples toward one dependency.
- Preserve behavior through delegation.

## Phase 2 — Pure extras extraction

- Move JSON, CSV, NDJSON, and built-in pagination controllers.
- Make public core extension contracts sufficient.
- Remove their concrete ecosystem dependencies/features from core.
- Prove local unknown implementations compile and work.
- Do not move multipart or generic record/pagination execution.

## Phase 3 — Governor extraction and default composition

- Remove governor-specific core downcast/task-local coupling.
- Add semantic limiter key-bound and cooldown-cap inputs.
- Move governor and default observer to extras.
- Make core fail closed without a limiter for non-empty plans.
- Install extras governor through façade feature composition.
- Preserve explicit user limiter precedence and bounded state.

## Phase 4 — OAuth2 extraction and generic provider declaration

- Move OAuth2 client-credentials provider to extras.
- Provide named secret-safe construction.
- Lower/deprecate old DSL syntax through generic provider semantics.
- Remove final macro JSON/OAuth name knowledge.
- Preserve provider HTTP isolation, limits, redaction, expiry, and recovery.

## Phase 5 — Public namespaces and feature ownership

- Finalize façade modules, prelude, advanced/dangerous surfaces.
- Move user feature ownership/defaults to façade.
- Keep implementation crate defaults empty.
- Convert canonical workspace consumers to one façade dependency.
- Establish packaging/publication posture.

## Phase 6 — Generated endpoint/request surface

- Add explicit resolved surface/capability model.
- Generate named endpoint request wrappers with inherent methods.
- Add sealed generated endpoint trait and associated request type.
- Merge public module trees by scope.
- Remove ordinary prelude/extension-trait requirement.
- Prove invalid capability absence and measure expansion.

## Phase 7 — Client construction and runtime policy

- Generate named client builders.
- Keep `new()` only for zero-input clients.
- Remove positional secrets/providers.
- Move concrete runtime policy selection out of contract syntax.
- Define repeated configuration precedence deterministically.

## Phase 8 — DSL v2

- Snake-case identifiers and deterministic type naming.
- Order-independent clauses and duplicate diagnostics.
- Explicit body role.
- Honest syntactic optionality.
- Stable contract IDs.
- Transitional legacy parser only where needed.

## Phase 9 — Descriptions, planning, and runtime endpoints

- Add safe generated/runtime endpoint descriptions.
- Add no-effect sanitized planning through real preparation logic.
- Rename/fix `RuntimeEndpoint` and timeout tri-state.
- Improve explicit endpoint reuse and bounded identity.
- Prove lifecycle equivalence and zero-effect planning.

## Phase 10 — Macro internal architecture

- Reorganize into syntax, semantic, and emit domains only now.
- Keep one resolved model and one surface/capability model.
- Move validation to its semantic owner.
- Ensure emit discovers no new user errors.
- Isolate public artifacts, docs, bridge, and feature assertions.

## Phase 11 — Examples as product narrative

- Convert examples into importable modules.
- Make binaries thin wrappers.
- Split Riot into domain/API/usage/tests.
- Replace external-network records behavior with deterministic local proof.
- Ensure all intended capabilities have a documented example and test.

## Phase 12 — Contract-oriented tests and fixtures

- Reclassify all existing tests/fixtures.
- Move generated public tests to façade.
- Add extras contract/runtime tests.
- Consolidate feature/dependency fixtures into one root fixture workspace/lockfile.
- Delete synthetic, duplicate, accidental-API, and source-scan proofs.

## Phase 13 — Documentation, workspace, tooling, and CI

- Rebuild docs around representation → generated API → usage → execution → capabilities → reference.
- Give exact syntax/features/limits/signatures one canonical owner.
- Perform late package path moves.
- Replace stale scripts and PowerShell-only orchestration with portable commands/`xtask`.
- Add MSRV, stable, all-feature, no-default, fixtures, docs, platform, security, and performance-smoke CI lanes.

## Phase 14 — Compatibility removal and release hardening

- Remove all temporary legacy behavior listed in the reference.
- Bump/check generated bridge contract.
- Audit final rustdoc and public paths.
- Run complete security, compiler/editor, package, fixture, docs, and performance matrices.
- Produce migration guide, changelog, package/version table, and final ledger.

---

# Prohibited shortcuts

Do not:

- move a concrete implementation to extras while exposing private/native core internals to it;
- make core depend on extras to recover old defaults;
- make macros inspect `Json`, `Csv`, `NdJson`, OAuth, governor, or pagination-controller names;
- emit direct `::concord_core` paths in the final user surface;
- use a generated prelude or trait import to make ordinary endpoint methods appear;
- keep positional secrets for backward convenience in the final API;
- treat exact DSL clause order as language semantics;
- infer body role only from the identifier `body`;
- introduce arbitrary Rust expressions into URL/header/auth shaping without preserving deterministic validation and redaction;
- expose Reqwest request/response types to body/response/auth/hook/rate-limit extensions;
- add another executor for runtime endpoints, extras, raw responses, provider HTTP, or pagination;
- return partial pagination results under the existing all-or-error terminal;
- weaken finite limits or first-excess withholding;
- detach completion work required for correctness;
- replace compiler privacy proofs with string searches;
- retain standalone Cargo packages for ordinary compile-pass tests;
- test examples by recreating their API elsewhere;
- use external service calls in CI;
- perform large package/file moves in the same patch as delicate runtime behavior changes;
- update Trybuild stderr blindly;
- claim validation that was not executed;
- overwrite unrelated repository changes.

---

# Required deliverables in the repository

By completion, the repository must contain:

1. The final package graph and façade API.
2. `concord_extras` with the selected concrete implementations.
3. Extras-agnostic macro generation through the façade bridge.
4. The target named endpoint/request public surface.
5. Named client builders and DSL v2.
6. Safe descriptions/planning and corrected runtime endpoint semantics.
7. The reorganized macro implementation.
8. Importable, deterministic examples including the split Riot narrative.
9. Contract-owned tests and the minimal root fixture workspace.
10. Redesigned documentation and contributor architecture/testing guides.
11. Portable tooling and CI commands.
12. A migration guide and release notes.
13. An updated `CONCORD_REFACTOR_REFERENCE.md` progress ledger and any deviation records.
14. Recorded validation and performance/expansion results.

---

# Final response requirements

When the implementation is complete, report:

- the resulting product architecture and dependency graph;
- the final declaration-to-generated-Rust contract;
- what moved to extras and what remained core, with reasons;
- how the macro remains extras-agnostic;
- how one-crate imports and feature forwarding work;
- lifecycle/security/boundedness behavior preserved;
- examples, tests, fixtures, docs, tooling, and workspace changes;
- compatibility removals and migration path;
- exact commands executed and their results;
- performance/compile measurements;
- every unresolved limitation or deviation.

Summarize by product contract and invariant, not by file count. Do not call an unavailable or unexecuted check successful.

Begin with Phase 0. Read `CONCORD_REFACTOR_REFERENCE.md` in full before making the first edit.
