# Concord Refactor Reference

**Status:** implementation specification and long-running execution ledger  
**Intended consumer:** a high-reasoning coding agent working directly in the Concord workspace  
**Repository reviewed:** the workspace supplied as `concord_api(64).zip`  
**Current reviewed package version:** `0.1.0`  
**Current generated contract version:** `14`  

---

## 0. How to use this document

This file is the durable source of truth for the Concord refactor. Place it at the repository root as `CONCORD_REFACTOR_REFERENCE.md` before implementation starts. The implementing agent must read the complete document before editing code, then re-read the current phase, the non-negotiable invariants, and the progress ledger at the beginning of every phase.

The document has four roles:

1. It defines the product Concord is intended to be.
2. It fixes the target public declaration-to-usage contract before implementation details are reorganized.
3. It assigns every important concept to `concord_core`, `concord_macros`, `concord_extras`, `concord`, examples, tests, or documentation.
4. It provides an ordered implementation and validation sequence that remains usable after a long context window or interrupted session.

### Execution rules

- Treat the **Selected design decisions** in this document as binding defaults. Do not reopen them merely because the existing implementation differs.
- A decision may be changed only when implementation evidence shows that it violates a non-negotiable invariant, is impossible under stable Rust/MSRV constraints, or creates a dependency cycle. Record the evidence and replacement in the progress ledger before proceeding.
- Preserve security, finite bounds, redaction, lazy execution, lifecycle ordering, deterministic completion, and transport isolation. Reducing code is never a sufficient reason to weaken them.
- Keep the workspace buildable at the end of every phase. Temporary compatibility aliases are permitted between phases, but they must have an explicit removal phase.
- Run and record every required validation command. Never state that a command passed unless it was actually run.
- Do not use external network access in tests or canonical examples.
- Do not make examples, fixtures, or tests independent sources of API declarations when they claim to prove a canonical declaration.
- Avoid broad internal restructuring before the public surface and semantic boundaries are established.
- Update only the **Progress ledger** and explicit **Deviation records** in this file during implementation. Do not silently rewrite the target design to match an easier implementation.

### Progress ledger

The implementing agent must maintain this table in the repository copy of the file.

| Phase | State | Last validated commit/worktree | Commands run | Deviations or blockers | Next concrete action |
|---|---|---|---|---|---|
| 0. Baseline and contract freeze | Complete | Phase 0 + 1 combined phase worktree based on `bdc9717` | `cargo metadata --format-version 1 --all-features` PASS; `cargo fmt --all -- --check` PASS; both workspace checks PASS with pre-existing warnings; both workspace test matrices FAIL only in the pre-existing core Trybuild snapshots on Rust 1.97.1; Clippy UNAVAILABLE for the pinned toolchain; doctests and both rustdoc gates PASS; `just verify` stops at unavailable Clippy; eight focused invariant proofs PASS; `cargo deny check` PASS with duplicate-version warnings; benchmark compile/smoke PASS. Exact commands and results: `docs/contributor/evidence/refactor-phase-0-baseline.md`. | Clean starting worktree. All packages are `publish = false`; no repository evidence of downstream commitments was found. The crates.io name `concord` is occupied by an unrelated project, so public-registry publication under that name requires owner coordination or a registry decision. Remote/tag inspection was blocked by repository-access policy. Existing Rust 1.97 Trybuild diagnostic drift affects eight core UI cases. | Completed in the explicitly combined Phase 0 + 1 commit |
| 1. Façade and crate graph | Complete | Phase 0 + 1 combined phase worktree based on `bdc9717` | Required façade checks/tests, macro tests, examples check, full metadata, renamed-façade offline/locked fixture, dependency policy, workspace checks, façade rustdoc, and focused security proofs PASS. Renamed expansion: 27 `::runtime::__private`, 2 façade bridge aliases, 0 `::concord_core` paths. Exact commands and results: `docs/contributor/evidence/refactor-phase-1-facade-graph.md`. | The Phase 14 direct-core macro fallback and Phase 5 example dependency migration are recorded. Rust 1.97 macro UI drift was reviewed and normalized; the eight pre-existing core UI mismatches remain. Clippy remains unavailable. Publication-name conflict remains for release planning. No design deviation. | Re-read Phase 2; extract pure codecs, record decoders, and pagination controllers through public core contracts |
| 2. Pure extras extraction | Complete | Phase 2 worktree based on `6b87ac6` | Required core/extras/façade checks, all six façade feature combinations, extras/façade/macro tests, renamed and extension fixtures, dependency trees, MSRV checks, rustdoc gates, 351 core lifecycle tests, focused redaction proofs, canonical example proof, metadata, workspace checks, formatting, and diff checks PASS. Exact commands and results: `docs/contributor/evidence/refactor-phase-2-extras.md`. | Direct core UI remains unsuccessful because of the recorded Rust 1.97 diagnostic drift; no core snapshots were blessed. Clippy remains unavailable. Temporary core/façade `oauth2-client` ownership is scheduled for Phase 4. Publication-name conflict remains. No design deviation. | Re-read Phase 3; extract governor/observer through public semantic rate-limit contracts and move default composition to the façade |
| 3. Governor extraction and default composition | Complete | Phase 3 worktree based on `8c237e1` | Required core/extras/façade checks and tests, full macro suite, no-governor offline/locked fixture, dependency trees, MSRV checks, rustdoc/doctests, 329 all-feature core lifecycle tests, focused redaction, workspace checks, formatting, and diff checks PASS. The exact full core no-default command reaches the pre-existing Rust 1.97 Trybuild drift after its 247 unit tests pass. Exact commands and results: `docs/contributor/evidence/refactor-phase-3-governor.md`. | The Phase 14 direct-core observer bridge is recorded and delegates only generic validation. The eight-case baseline core UI drift and unavailable Clippy remain; no snapshots were blessed. Core/extras default-feature finalization remains Phase 5. No design deviation. | Re-read Phase 4; extract OAuth2 client credentials through the public provider HTTP contract and lower legacy syntax to generic provider semantics |
| 4. OAuth2 extraction and generic provider declaration | Complete | Phase 4 worktree based on `2ed0c43` | Required core/extras/façade tests, full macro/compiler/fixture suite, third-party provider and feature-absence proofs, dependency trees/policy, MSRV checks, rustdoc/doctests, 324 all-feature core lifecycle tests, deterministic OAuth TLS/redaction/boundedness tests, workspace checks, formatting, and diff checks PASS. Expansion: 7,419 lines, 421,898 characters, 25,692 fragments, 578 declaration-shaped lines. Exact commands and results: `docs/contributor/evidence/refactor-phase-4-oauth2.md`. | The exact full core no-default command reaches the three pre-existing Rust 1.97.1 dangerous-feature Trybuild rendering mismatches after 247 unit tests pass; no snapshots were blessed. Clippy remains unavailable. Manual rust-analyzer interaction remains a release gate. No design deviation. | Re-read Phase 5; finalize façade namespaces, empty implementation defaults, feature ownership, one-crate consumers, and packaging posture |
| 5. Public namespaces and feature ownership | Complete | Phase 5 worktree based on `b0ffa5d` | Required workspace checks, façade/examples tests, ten no-default/selected façade combinations, full macro/compiler/fixture suite, core/extras tests, MSRV matrix, rustdoc/doctests, dependency policy/tree/metadata, focused security matrix, support-package dry-run, Cargo deny, benchmark compile/smoke, formatting, and diff checks PASS. Exact commands and results: `docs/contributor/evidence/refactor-phase-5-namespaces-features.md`. | The exact full core/workspace Rust 1.97.1 commands retain the recorded three/eight Trybuild rendering mismatches; no snapshots were blessed. Clippy is unavailable. Product package dry-runs remain deferred until exact siblings are registry-resolvable in the documented support → core → extras/macros → façade order and Phase 14 supplies release metadata; the occupied crates.io façade name remains a release blocker. Manual rust-analyzer interaction remains a release gate. No design deviation. | Re-read Phase 6; add one resolved surface/capability model, named endpoint request wrappers with inherent methods, the sealed generated endpoint trait, and the unified scope module tree |
| 6. Generated endpoint/request surface | Complete | Phase 6 worktree based on `02644ef` | Required façade surface/UI/deterministic-runtime gates, full macro/compiler/fixture suite, façade all-feature/no-default tests, examples, workspace checks, 324 core lifecycle tests, 17 seeded security tests, rustdoc/doctests, MSRV compilation, formatting, diff checks, and expansion/timing measurements PASS. Expansion changed from 7,419 lines / 421,898 characters / 25,692 fragments / 846 declaration-shaped lines to 10,105 / 540,178 / 34,314 / 1,366. Exact commands and results: `docs/contributor/evidence/refactor-phase-6-generated-surface.md`. | The exact Rust 1.85 façade test command reaches three reviewed Trybuild rendering mismatches after every runtime test, the named compile-pass case, and one compile-fail case pass; no snapshot was blessed. Clippy remains unavailable. Manual rust-analyzer interaction remains a release gate. The named-wrapper expansion increase is recorded for Phase 10. No design deviation. | Re-read Phase 7; generate named client builders, remove configured positional constructors, and separate runtime policy selection from the contract declaration |
| 7. Client construction and runtime policy | Complete | Phase 7 worktree based on `f6d6b93` | Required façade/core/extras/examples validation, full macro/compiler/fixture suite, all-feature/no-default façade tests, 327 core lifecycle tests, 17 seeded security tests, MSRV workspace compilation, rustdoc/doctests, formatting, workspace checks, diff checks, and expansion measurement PASS. The exact full core and Rust 1.85 façade test commands retain only the recorded compiler-rendering drift. Exact commands and results: `docs/contributor/evidence/refactor-phase-7-client-construction.md`. | The exact full core command retains the eight-case Rust 1.97.1 Trybuild drift; the Rust 1.85 façade command has the three recorded Phase 6 differences plus whitespace rendering in the new explicit-secret diagnostic. No snapshots were blindly blessed. Clippy is unavailable and manual rust-analyzer interaction remains a release gate. Legacy custom-retry and observer DSL removal is scheduled for Phase 8; the `PreparedEndpoint` public rename remains Phase 9. No design deviation. | Re-read Phase 8; implement order-independent named clauses, duplicate-span diagnostics, snake-case source identifiers, explicit body role, syntactic optionality, and stable contract IDs |
| 8. DSL v2 | Complete | Phase 8 worktree based on `5ee17d5` | Required macro/façade surface and UI suites, examples, core/extras tests, 325 core lifecycle tests, 17 seeded security tests, dependency trees, renamed-façade expansion, MSRV compilation/unit tests, rustdoc/doctests, workspace checks, formatting, and diff checks PASS. Expansion remains 9,901 lines / 1,366 declaration-shaped lines and grows 1.45% in characters for documented contract identity. Exact commands and results: `docs/contributor/evidence/refactor-phase-8-dsl-v2.md`. | The exact full core and Rust 1.85 façade test commands retain only the recorded compiler-rendering drift; no snapshot was blindly blessed. Clippy is unavailable and manual rust-analyzer interaction remains a release gate. The full 1/10/100/1,000-endpoint performance protocol remains Phase 14 work. No design deviation. | Re-read Phase 9; add one safe generated/runtime description, zero-effect planning, bounded owned runtime identity, and the final `RuntimeEndpoint` timeout tri-state |
| 9. Descriptions, planning, and runtime endpoints | Complete | Phase 9 worktree based on `48908cc` | Required façade/core/examples/UI validation, 329 core lifecycle tests, five planning/runtime proofs, 17 seeded security tests, all-feature/no-default/MSRV matrices, rustdoc/doctests, dependency policy, Cargo deny, formatting, expansion measurement, and 12-category performance smoke PASS. Exact commands and results: `docs/contributor/evidence/refactor-phase-9-endpoints-planning.md`. | The exact full core and Rust 1.85 test commands retain only the recorded compiler-rendering drift; no snapshot was blindly blessed. Clippy is unavailable and manual rust-analyzer interaction remains a release gate. Expansion grew to 12,566 lines / 1,536 declaration-shaped lines and is recorded for Phase 10 consolidation. Runtime pagination remains the selected bounded deferral. No design deviation. | Re-read Phase 10; reorganize the macro into syntax, semantic, and emission domains while retaining one resolved/surface model and discovering no new emit-time user errors |
| 10. Macro internal architecture | Complete | Phase 10 worktree based on `6fe0c36` | Required macro/façade UI and generated-surface suites, workspace checks, all-feature façade tests, 17 seeded security tests, planning/runtime proofs, MSRV compilation/unit tests, rustdoc/doctests, dependency/rename policy, formatting, diff checks, exact expansion comparison, and clean compile measurement PASS. Exact commands and results: `docs/contributor/evidence/refactor-phase-10-macro-architecture.md`. | Clippy remains unavailable for Rust 1.97.1 and the interactive rust-analyzer protocol remains a manual release gate. The first aggregate macro run found two supplemental source-policy tests tied to retired flat file paths; their path enumeration was corrected and the full rerun passed without snapshot changes. Generated output is byte-for-byte identical to Phase 9 and clean macro check time improved from 7.142 s to 7.111 s. No design deviation. | Re-read Phase 11; turn examples into importable product-narrative modules, thin binaries, a split Riot example, and deterministic local behavior proofs |
| 11. Examples as product narrative | Complete | Phase 11 worktree based on `7b46fd4` | Required examples checks/tests/doctests, default/all-feature/no-default suites, workspace checks, MSRV compilation, Rustdoc warnings gate, support tests, dependency/metadata inspection, five planning proofs, and 17 seeded security tests PASS. Exact commands and results: `docs/contributor/evidence/refactor-phase-11-examples.md`. | Clippy remains unavailable for Rust 1.97.1; process-wide egress enforcement was unavailable, though every executed runtime test uses loopback and the manual Riot binary was not run. Manual rust-analyzer interaction remains a release gate. No design deviation. | Re-read Phase 12; inventory every proof, move generated public tests to the façade and concrete implementation tests to extras, consolidate graph-dependent fixtures, and delete synthetic/source-scan/accidental-API proofs |
| 12. Contract-oriented tests and fixtures | Complete | Phase 12 worktree based on `39cbf9d` | Required default/all-feature workspace suites, grouped façade/core UI suites, 14-case façade feature matrix, six graph-fixture groups, core/extras/façade/support/macro owner suites, 20 focused lifecycle/security/bounds tests, metadata/lockfile/dependency policy, MSRV compilation/runtime checks, rustdoc/doctests, Cargo deny, formatting, and diff checks PASS. Exact commands and results: `docs/contributor/evidence/refactor-phase-12-tests-fixtures.md`. | Rust 1.85 ordinary tests pass but exact full façade/core commands retain only 19/14 Trybuild rendering mismatches against the deliberately Rust 1.97-owned snapshots; no snapshot was blindly blessed. Clippy is unavailable and manual rust-analyzer interaction remains a release gate. No design deviation. | Re-read Phase 13; rebuild product/reference/contributor documentation, add portable xtask/CI lanes, finish late workspace layout work, and record tooling portability |
| 13. Documentation, workspace, tooling, and CI | Complete | Phase 13 worktree based on `62278d1`; structural path commit `1ffb54c` | Default/all-feature workspace checks and tests, locked fixtures, generated docs/link checks, doctests, three warning-denied Rustdoc gates, Rust 1.85 no-default workspace check and core/extras tests, Rust 1.97.1 warning-denied Clippy through the installed stable alias, command inventory, Cargo deny, format/diff checks, and 12-category performance smoke PASS. Exact commands/results: `docs/contributor/evidence/refactor-phase-13-docs-tooling.md`. | The exact-channel Clippy component is locally inapplicable, while the same-commit stable alias passes; CI installs it explicitly. CI/YAML parsing, Linux/macOS lanes, and interactive rust-analyzer were not executable locally and are not claimed. Publication remains intentionally disabled pending approved metadata/name ownership. Localized MSVC linker warning persists. No design deviation. | Re-read Phase 14; remove every temporary compatibility path, bump the bridge contract, produce migration/release artifacts, and run the final matrices |
| 14. Compatibility removal and release hardening | Not started | — | — | — | — |

### Deviation record template

```text
Decision affected:
Observed implementation constraint:
Evidence (compiler output, dependency graph, or failing invariant):
Options considered:
Chosen replacement:
Why the replacement still satisfies the product thesis and invariants:
Tests added or changed:
```

---

# 1. Product thesis

## 1.1 Concise mental model

```text
Remote HTTP API contract
        │
        ▼
Concord declaration compiler
        │
        ▼
Predictable ordinary Rust endpoint and request types
        │
        ▼
One immutable client snapshot and one bounded managed lifecycle
```

Concord is not primarily a Reqwest wrapper and not merely a compact HTTP configuration macro.

> **Concord is a contract compiler that translates a reviewed remote HTTP API declaration into an ordinary Rust capability surface, then executes every represented operation through one bounded, redaction-aware, deterministic lifecycle.**

The declaration language and the generated Rust API are one public language. The generated Rust surface is the primary product. The macro compiler and runtime kernel exist to make that surface predictable and safe.

## 1.2 Intended users

Concord serves:

- application teams integrating substantial remote APIs;
- library authors publishing typed clients;
- systems that require authentication, pagination, rate limiting, redaction, finite body handling, and deterministic completion to be consistent across endpoints;
- advanced users implementing semantic codecs, bodies, response specifications, credential providers, pagination controllers, limiters, and observation hooks without native request/response access.

Concord is not optimized for one-off arbitrary requests, alternate transports, unbounded streaming, native Reqwest mutation, proxying unknown APIs dynamically, or page concurrency without a separately designed semantic contract.

## 1.3 Essential guarantees

These are product invariants, not local implementation preferences.

| Invariant | Required meaning |
|---|---|
| Contract-shaped interface | Generated Rust topology corresponds predictably to clients, scopes, endpoints, parameters, bodies, responses, and capabilities. |
| Compile-time capability control | Invalid terminals and endpoint/client combinations are absent or rejected using public-language diagnostics. |
| Inert construction | Creating a client builder, scope, endpoint value, or pending request performs no HTTP, credential acquisition, limiter acquisition, hook invocation, stream polling, body encoding, or native request construction. |
| Immutable runtime snapshot | A built client has fixed transport, limits, hooks, limiter, credential slots, and client values shared safely by clones. |
| One executor | Generated calls, explicit generated endpoint values, and runtime-defined endpoints converge on the same core lifecycle. |
| Finite ordinary behavior | Request bodies, response bodies, streams, status bodies, provider exchanges, pagination, identities, limiter state, and cooldowns remain bounded. |
| Private authentication | Native authentication material is applied after safe observation and never appears in ordinary hooks, plans, errors, debug output, or safe URLs. |
| Redaction | Protected URLs, headers, credential values, bodies, and parser text are not leaked through public formatting or events. |
| Deterministic result semantics | Status handling, bounded acquisition, pagination all-or-error behavior, recovery limits, result fixation, and completion ordering are centrally defined. |
| Feature truthfulness | Disabled capabilities are absent or fail through focused compile-time/public-surface errors. |
| Semantic extension points | Extensions receive domain-level values and bounded semantic inputs, never native requests, native responses, or an alternate executor. |
| Type-neutral macro | The macro treats arbitrary Rust body, response, controller, provider, observer, and limiter types as opaque types satisfying core contracts. It does not recognize `Json`, `Csv`, `NdJson`, or other extras by spelling. |

Application-supplied synchronous or asynchronous trait implementations remain a deliberate trust boundary. Concord bounds work it owns; extensions must bound their own CPU, memory, I/O, logging, storage, retries, spawned tasks, and cardinality.

## 1.4 Intrinsic versus accidental complexity

Intrinsic complexity that must remain explicit:

- credential acquisition, versioning, refresh, and one bounded challenge recovery;
- body replayability and consistency across attempts;
- deadlines across hooks, body work, authentication, limiting, transport, and acquisition;
- safe logical requests versus privately authenticated native requests;
- status classification before response decoding;
- first-excess withholding and bounded streaming;
- pagination state, repeated identity detection, item/page ceilings, and all-or-error results;
- limiter authority, cooldowns, bounded state, and fail-closed behavior;
- completion after result fixation;
- cancellation without detached correctness work.

Accidental complexity to remove:

- requiring users to depend on `concord_core` and `concord_macros` separately;
- placing ecosystem codecs and concrete strategies in core;
- generated per-client preludes and extension traits;
- positional constructors containing several values, secrets, and providers;
- exact DSL clause ordering;
- method names indirectly derived from PascalCase declaration identifiers;
- separate generated `facades` and `endpoints` module trees;
- `PreparedEndpoint` terminology conflicting with internal prepared execution phases;
- examples as independent binaries rather than importable modules;
- tests that recreate canonical APIs;
- isolated Cargo packages for proofs that do not depend on a separate resolution graph;
- duplicated feature, syntax, limit, and public-path documentation;
- stale scripts and platform-specific command orchestration.

## 1.5 Meaning of desirable, elegant, and idiomatic

For Concord:

- **Desirable** means a user can predict the method, endpoint type, request type, optional setters, terminals, module path, and compile errors by reading one declaration.
- **Elegant** means one concept has one semantic owner and one public name; difficult runtime behavior is concentrated centrally rather than reflected in imports and call-site ceremony.
- **Idiomatic** means snake_case methods/modules, PascalCase types, named builders for configuration and secrets, inherent methods for discoverability, consuming immutable builders, ordinary associated types and trait bounds, and rustdoc that presents real public signatures.

---

# 2. Selected design decisions

These decisions are the implementation target unless a documented deviation is required.

1. The user-facing package is **`concord`**.
2. The implementation packages are **`concord_core`**, **`concord_macros`**, and **`concord_extras`**. `concord_examples` and `concord_test_support` remain internal workspace packages.
3. `concord` depends on and reexports core, macro, and extras capabilities. Ordinary users and every canonical example depend only on `concord`.
4. `concord_macros` must not depend on `concord_extras` and must not inspect extra type names. It should ideally have no normal dependency on `concord_core` either: it compiles syntax using `syn`/`quote` and emits tokens against the façade’s hidden bridge. A dev dependency may be used only if a test cannot be owned by the façade.
5. Generated code resolves the caller-visible **`concord`** package, including dependency renames, and references `concord::__private`. The façade delegates that bridge to core and performs feature-dependent composition with extras.
6. `concord_core` owns semantic traits, lifecycle state, bounded acquisition, sealed modes, redaction, transport isolation, primitive wire types, and all invariants.
7. `concord_extras` owns implementations that can be written entirely against public core semantic contracts.
8. Move to extras: `Json<T>`, `Csv<T>`, `NdJson<T>`, `OffsetPagination`, `PagePagination`, `CursorPagination<C>`, `HasNextCursor<C>`, `GovernorRateLimiter`, `DefaultRateLimitObserver`, and `OAuth2ClientCredentials`.
9. Keep in core: `Bytes` request/response behavior, `NoContent`, `Text`, `OctetStream`, `MediaType`, `Stream<M>`, `RequestBody`, `StreamBody`, multipart transport machinery and its semantic public plan, generic record streaming (`RecordDecoder`, `Records<D>`, `RecordStream`, `RecordBatches`), pagination execution/contracts/limits, rate-limit contracts/plans/fallback/no-op limiter, authentication materials/slots/placement, hooks, errors, HTTP/runtime configuration, and the executor.
10. Multipart remains core-owned because native boundary generation and bounded replayability are transport lifecycle responsibilities. Extras may later add convenience helpers, but it must not receive native Reqwest access.
11. `concord_core` and `concord_extras` have empty default feature sets. The façade owns user-facing defaults and feature documentation.
12. Preserve the current user-facing runtime defaults initially: `default-tls`, `http2`, and `rate-limit-governor`. Keep JSON opt-in unless a separate product decision changes it.
13. The final DSL has no special knowledge of OAuth2 or JSON. OAuth2 client credentials are supplied as an ordinary custom credential provider from extras.
14. Custom transport retry selection is runtime client configuration, not remote contract syntax. Remove `retry custom PATH` from the final DSL and retain the dangerous feature only on explicit HTTP builder APIs.
15. Every endpoint generates one named endpoint value and one named request wrapper with inherent capabilities. Remove generated request extension traits and generated per-client preludes.
16. Endpoint and scope identifiers in the DSL directly use snake_case Rust method/module names. Generated types are deterministic PascalCase transformations.
17. Named DSL clauses are order-independent. Duplicate clauses are rejected with both spans. Internal list ordering remains meaningful.
18. Request body role is explicit with `#[body]` on the parameter. Body parameters may occur where declared; generated required arguments preserve declaration order.
19. Syntactic `Option<T>` and `std::option::Option<T>` define optional parameters. Type aliases are not resolved by the macro and this limitation is documented. Defaults use `name: Type = expression`.
20. Configured clients use a named builder. `Client::new()` exists only for zero-required-input clients. No positional secret constructors remain.
21. Generated scope and endpoint modules form one scope-oriented tree; there are no public `facades` versus `endpoints` trees.
22. A documented sealed generated endpoint trait exposes an associated request type and safe description, enabling generic endpoint composition without allowing arbitrary execution types.
23. `PreparedEndpoint` becomes `RuntimeEndpoint`. The final timeout state is a true `InheritClient | Explicit(Duration) | Disabled` tri-state.
24. Add safe endpoint descriptions and a sanitized planning surface. Planning reuses real public-head construction but performs no transport, credentials, limiter, hooks, body encoding, body factory creation, stream polling, or response work.
25. Generated/rate-limit identity has an optional explicit stable contract ID. Without one, a deterministic source-path identity is generated. Rust renaming must not accidentally change an explicitly declared ID.
26. The project is currently `0.1.0` and unpublished. Prefer a clean breaking refactor with temporary intra-workspace aliases rather than preserving permanent duplicate APIs.
27. Tests live at the lowest layer that can prove the contract. Standalone Cargo graphs remain only for dependency rename/replacement, feature absence/unification, package metadata, or other assertions that cannot be reproduced in the main workspace.
28. Examples are importable library modules. Binaries are thin wrappers. Tests import the actual example declarations.

---

# 3. Target crate graph

## 3.1 Dependency graph

```text
                         ┌──────────────────┐
                         │ concord_macros   │
                         │ proc-macro only  │
                         │ no extras edge   │
                         └────────┬─────────┘
                                  │ reexported by
                                  ▼
┌──────────────────┐      ┌──────────────────┐      ┌──────────────────┐
│ concord_core     │◄─────│ concord          │─────►│ concord_extras   │
│ semantic kernel │      │ public façade    │      │ trait impls      │
└────────▲─────────┘      └────────▲─────────┘      └────────┬─────────┘
         │                         │                         │
         └─────────────────────────┴─────────────────────────┘
                    extras depends on core

concord_examples ───────────────► concord
perf/tools ─────────────────────► concord (+ explicitly unstable dev support when required)
concord_test_support is dev/tooling-only
```

The proc macro must emit paths through `concord::__private`; it cannot emit a direct `::concord_core` path while still promising a one-dependency user experience, because transitive dependencies are not available in the downstream extern prelude.

“Macros only use core” therefore means:

- the generated semantic contract is core-owned;
- the façade’s hidden bridge delegates to core;
- the macro is agnostic to extras;
- the proc-macro implementation should not compile the runtime kernel for the host target without a demonstrated need.

## 3.2 Package roles

### `concord_core`

Owns the managed semantic kernel and public extension contracts. It may be published because `concord` and `concord_extras` need it as a registry dependency, but it is not the documented ordinary entry point. Exact-version sibling dependencies are required.

### `concord_macros`

Owns source parsing, normalization, semantic validation, generated surface modeling, and token emission. It has no runtime feature graph and no knowledge of format/strategy implementations.

### `concord_extras`

Owns canonical downstream implementations of public extension traits. It is also a structural proof that those extension contracts are sufficient without private/native access.

### `concord`

Owns the product:

- `api!` reexport;
- stable public paths;
- top-level common types;
- domain modules;
- user-facing features and defaults;
- façade prelude;
- hidden generated bridge;
- feature-dependent default composition such as the built-in governor.

### `concord_examples`

Owns the executable product narrative. It depends only on `concord` plus normal application dependencies such as Serde derives and Tokio for runnable binaries.

### `concord_test_support`

Owns shared deterministic executors, loopback/TLS servers, scripted exchanges, and fixture helpers. It is not a user extension crate.

## 3.3 No-cycle rules

- `concord_core` must not depend on `concord_extras`, `concord`, or `concord_macros`.
- `concord_extras` depends on `concord_core`, never on `concord` or `concord_macros`.
- `concord_macros` depends on neither `concord` nor `concord_extras`. Avoid a normal core dependency.
- `concord` depends on all three implementation packages.
- Core tests must not use extras as a shortcut for testing lifecycle semantics; use local deterministic test implementations. Extras tests prove concrete implementations.
- Generated public integration and UI tests belong primarily to `concord`, avoiding a macro-dev-dependency cycle.

---

# 4. Core versus extras ownership

## 4.1 Ownership rule

A type belongs in extras when all of the following are true:

1. It is a concrete codec, protocol helper, strategy, controller, observer, or limiter rather than a lifecycle invariant.
2. It can be implemented using documented public semantic contracts.
3. Moving it does not require exposing a native Reqwest request/response or private executor state.
4. Its absence does not make the core lifecycle semantically incomplete.

A type belongs in core when any of the following are true:

- core must inspect or seal it to enforce bounded acquisition;
- it crosses the native transport boundary;
- it owns lifecycle phase ordering, retries, recovery, cancellation, or completion;
- it is the generic contract required for third-party implementations;
- it is a primitive representation needed for every client;
- it provides the fail-closed behavior when no optional implementation exists.

## 4.2 Detailed movement table

| Current item | Final owner | Reason and required work |
|---|---|---|
| `Json<T>` | `concord_extras::codec` | Ordinary `EncodeBody`/`DecodeBody` implementation using Serde. Move tests and JSON media-type validation. Core removes public JSON feature and Serde codec dependency. |
| `Csv<T>` | `concord_extras::records` | Concrete `RecordDecoder` implementation. Generic bounded record engine stays core. |
| `NdJson<T>` | `concord_extras::records` | Concrete `RecordDecoder` implementation. Preserve independent feature from buffered JSON. |
| `OffsetPagination` | `concord_extras::pagination` | Concrete controller policy; core keeps `PaginationController`, loop, limits, identity, pending types, and errors. |
| `PagePagination` | `concord_extras::pagination` | Same. |
| `CursorPagination<C>` | `concord_extras::pagination` | Same. |
| `HasNextCursor<C>` | `concord_extras::pagination` | Helper contract specific to the built-in cursor controller, not required by core execution. |
| `GovernorRateLimiter` | `concord_extras::rate_limit` | Concrete process-local limiter. Core keeps plans, requests, permits, cooldown context, `RateLimiter`, no-op/fallback, and enforcement points. |
| `DefaultRateLimitObserver` | `concord_extras::rate_limit` | Concrete Retry-After interpretation policy used by the governor. Core retains observation contracts and cap data. |
| `OAuth2ClientCredentials` | `concord_extras::auth` | Concrete `CredentialProvider` protocol adapter using JSON. Core keeps provider HTTP, material, slots, recovery, and placement. |
| `Text` | `concord_core::codec` | Primitive UTF-8 wire codec with no ecosystem dependency; appropriate ordinary baseline. |
| `OctetStream` and `MediaType` | `concord_core::codec` | Primitive streaming media contract. |
| `Bytes` request/response impls | `concord_core` | Fundamental bounded buffered representation. |
| `NoContent` | `concord_core::response` | Fundamental discard response specification and sealed mode use. |
| `Stream<M>`, `StreamBody`, `ByteStream` | `concord_core` | Core-owned bounded streaming lifecycle. |
| `RequestBodySpec`, `ResponseBodySpec`, `EncodeBody`, `DecodeBody` | `concord_core` | Public extension contracts and blanket adapters. |
| Multipart plan/body/factory/parts/marker | `concord_core::body` | Core must create native multipart forms, enforce replayability/length bounds, and isolate Reqwest. Do not move merely because it is feature-gated. |
| `RecordDecoder`, `Records<D>`, `RecordStream`, `RecordBatches` | `concord_core::records` | Generic bounded stream framing and acquisition integration. |
| `PaginationController`, `PageLimit`, `PaginatedRequest` | `concord_core::pagination` | Generic execution contract and terminal semantics. |
| `RateLimiter`, rate plans/keys/rules/windows/permit/cooldown contexts | `concord_core::rate_limit` | Execution authority and bounded key preparation are core invariants. |
| `NoopRateLimiter` and private fallback | `concord_core::rate_limit` | Explicit opt-out and fail-closed baseline are core semantics. |
| Authentication material, slots, placement, challenge policy | `concord_core::auth` | Security and lifecycle ownership. |
| HTTP/runtime builders, hooks, errors, redaction | `concord_core` | Central runtime contract. |

## 4.3 Public-contract requirement for extras

`concord_extras` must compile using the same public contracts available to a third-party extension crate. Do not create a broad `concord_core::__extras` friend API to make extraction easy.

Permitted exception: an exact-version hidden constructor may be added only when the operation is semantically core-owned and cannot be expressed safely as a public extension contract. Every such exception must be documented and tested as a narrow bridge. The governor and OAuth2 provider should not require one.

## 4.4 Required core contract changes for governor extraction

The current governor relies on core-private downcasting and a task-local cooldown cap. Replace those couplings with explicit semantic data:

1. Add a documented default method to `RateLimiter`, for example:

   ```rust
   fn preparation_key_limit(&self) -> usize {
       DEFAULT_RATE_LIMIT_KEY_BYTES
   }
   ```

   Core uses this value when preparing bounded keys. The governor overrides it. Custom limiters receive a safe default.

2. Include the centrally configured cooldown cap in the rate-limit observation context passed to the limiter, or provide an equivalent public semantic accessor. The extras governor clamps observer-proposed cooldowns to that cap. Remove core-private governor downcasts and task-local coupling.

3. Core’s default runtime limiter becomes the fail-closed fallback. It grants requests with an empty plan and rejects generated non-empty plans when no explicit/default limiter was installed.

4. The façade hidden bridge installs an extras governor as the generated default when the `rate-limit-governor` façade feature is enabled. The macro calls the bridge without naming the governor.

5. Distinguish a generated default limiter from an explicitly user-supplied limiter so a DSL-selected observer can replace/configure only the generated default and never override explicit runtime configuration.

## 4.5 Required auth contract changes for OAuth2 extraction

The final DSL treats OAuth2 as an ordinary provider implementation:

```rust
credentials {
    credential oauth = custom OAuth2ClientCredentials;
}
```

The exact existing custom-provider grammar may be reused or clarified, but the macro must not contain an `oauth2_client` semantic branch in the final state.

`concord_extras::auth::OAuth2ClientCredentials` should expose a named builder:

```rust
let provider = OAuth2ClientCredentials::builder(token_url)
    .client_id(client_id)
    .client_secret(client_secret)
    .scope("users.read offline")
    .build()?;

let api = AuthApi::builder()
    .oauth_provider(provider)
    .build()?;
```

The provider must preserve:

- HTTPS-only token URL validation;
- no userinfo or fragments;
- provider HTTP isolation;
- bounded provider request/response behavior inherited from core;
- safe error reduction without parser/body text;
- expiry handling;
- refresh-as-reacquisition behavior;
- redacted debug output;
- no native application request access.

Generated custom-provider credential handles should support safe provider replacement where the type model permits it, restoring runtime rotation without a special OAuth branch.

---
# 5. Ideal public representation

## 5.1 Representation thesis

The target representation is a compact contract DSL using Rust types and signature-like endpoint declarations. It deterministically generates:

- one client and named client builder;
- immutable scope facades;
- one explicit endpoint value per endpoint;
- one named pending request wrapper per endpoint;
- inherent optional/default setters and capability terminals;
- static safe endpoint descriptions;
- predictable module paths and rustdoc;
- a sealed generated endpoint trait for generic composition;
- bridge implementations that converge on one core executor.

The DSL is justified because hierarchical scopes, inherited shaping, authentication plans, lexical rate limits, endpoint-backed credentials, and pagination bindings do not map cleanly to a trait-only interface. The response to proc-macro editor limitations is not to force the contract into unnatural attributes; it is to make generated Rust exceptionally ordinary and predictable.

## 5.2 Declaration-to-generation mapping

| Declaration construct | Generated Rust contract |
|---|---|
| `client UsersApi` | `UsersApi`, `UsersApiBuilder`, module `users_api` |
| `scope league_v4` | method `league_v4()`, module `league_v4`, facade type `LeagueV4` |
| `GET get_user` | method `get_user`, endpoint type `GetUser`, request type `GetUserRequest` |
| Required endpoint parameter | Required direct-method and endpoint-constructor argument in declaration order |
| `Option<T>` endpoint parameter | Omitted from direct required arguments; inherent setter and `clear_*` method |
| Defaulted endpoint parameter | Omitted from direct required arguments; inherent setter and `reset_*` method |
| `#[body] value: BodySpec` | Direct method accepts `<BodySpec as RequestBodySpec>::Input` at the declared required-argument position |
| Response specification | Await output is `<ResponseSpec as ResponseBodySpec>::Output` |
| Pagination clause | Only that request wrapper has `paginate` |
| Endpoint credential source | Only that source request wrapper has `store_as_*` |
| Dangerous raw capability | Generated only when response mode and façade feature permit it |
| Explicit endpoint value | Public at the predictable scope module path |
| Runtime endpoint | `client.request(runtime_endpoint)` enters the same executor through a generic managed runtime request |
| Endpoint ID/description | Generated from the same normalized contract model, never separately maintained |

The direct facade method and explicit endpoint binding are equivalent:

```rust
api.get_user(id)
```

has the same semantic meaning as:

```rust
api.request(users_api::GetUser::new(id))
```

There must not be parallel endpoint models.

## 5.3 Minimal API

Target declaration:

```rust
use concord::{api, Json};

api! {
    client UsersApi {
        base "https://api.example.com/v1/";
    }

    GET get_user(id: UserId) -> Json<User> {
        id "users.get";
        path ["users", ep.id];
    }
}
```

Target usage:

```rust
let api = UsersApi::new()?;

let user: User = api.get_user(UserId(42)).await?;

let response = api
    .get_user(UserId(43))
    .timeout(Duration::from_secs(2))
    .response()
    .await?;
```

Predictable generated names:

```text
UsersApi
UsersApiBuilder
users_api::GetUser
users_api::GetUserRequest
```

The source endpoint name is already the method name. No PascalCase-to-acronym heuristic is needed.

## 5.4 Realistic multi-scope Riot API

```rust
use concord::{api, Json};

api! {
    client RiotApi {
        base "https://riotgames.com/";

        vars {
            platform_route: PlatformRoute = PlatformRoute::Euw1;
            regional_route: RegionalRoute = RegionalRoute::Europe;
        }

        secrets {
            api_key;
        }

        credentials {
            credential riot_key = api_key(secret.api_key);
        }

        auth header "X-Riot-Token" = riot_key challenge never;
        timeout 20s;
    }

    scope platform() {
        host [vars.platform_route, "api"];
        path ["lol"];
        rate_limit {
            20 / 1s;
            100 / 2m;
        }

        scope league_v4() {
            path ["league", "v4"];

            GET league_entries(
                queue: LeagueQueue,
                tier: LeagueTier,
                division: LeagueDivision,
                page: u32 = 1,
            ) -> Json<Vec<LeagueEntryDto>> {
                id "riot.league_v4.league_entries";
                path ["entries", ep.queue, ep.tier, ep.division];
                query { "page" = ep.page; }
                rate_limit { 50 / 10s; }
            }
        }
    }

    scope regional() {
        host [vars.regional_route, "api"];
        path ["lol", "match", "v5"];

        GET match_by_id(match_id: MatchId) -> Json<MatchDto> {
            id "riot.match_v5.match_by_id";
            path ["matches", ep.match_id];
        }

        GET matches_by_puuid(
            puuid: Puuid,
            start: u64 = 0,
            count: NonZeroU32 = DEFAULT_MATCH_PAGE_SIZE,
        ) -> Json<Vec<MatchId>> {
            id "riot.match_v5.matches_by_puuid";
            path ["matches", "by-puuid", ep.puuid, "ids"];
            query {
                "start" = ep.start;
                "count" = ep.count;
            }
        }
    }
}
```

Usage:

```rust
let riot = RiotApi::builder()
    .platform_route(PlatformRoute::Euw1)
    .regional_route(RegionalRoute::Europe)
    .api_key(api_key)
    .build()?;

let entries = riot
    .platform()
    .league_v4()
    .league_entries(
        LeagueQueue::RankedSolo,
        LeagueTier::Diamond,
        LeagueDivision::One,
    )
    .page(2)
    .await?;

let matches = riot
    .regional()
    .matches_by_puuid(puuid)
    .start(100)
    .count(NonZeroU32::new(50).unwrap())
    .await?;
```

Predictable explicit paths:

```text
riot_api::platform::league_v4::LeagueEntries
riot_api::platform::league_v4::LeagueEntriesRequest
riot_api::regional::MatchById
riot_api::regional::MatchesByPuuid
```

Scope facade chaining remains a strong part of the current design and should be preserved.

## 5.5 Authenticated API using an extras provider

```rust
use concord::{api, Json};
use concord::auth::{AccessToken, OAuth2ClientCredentials};

api! {
    client AuthApi {
        base "https://service.example.com/";

        secrets {
            api_key;
        }

        credentials {
            credential application_key = api_key(secret.api_key);
            credential session = endpoint login;
            credential oauth = custom OAuth2ClientCredentials;
        }
    }

    POST login(
        #[body] input: Json<LoginRequest>,
    ) -> SessionTokenCodec;

    GET profile() -> Json<Profile> {
        auth bearer session challenge unauthorized;
    }

    GET application_status() -> Json<ApplicationStatus> {
        auth header "X-Api-Key" = application_key challenge never;
    }

    GET oauth_profile() -> Json<Profile> {
        auth bearer oauth challenge unauthorized;
    }
}
```

Usage:

```rust
let oauth = OAuth2ClientCredentials::builder(token_url)
    .client_id(client_id)
    .client_secret(client_secret)
    .scope("profile.read")
    .build()?;

let api = AuthApi::builder()
    .api_key(api_key)
    .oauth_provider(oauth)
    .build()?;

api.login(LoginRequest { username, password })
    .store_as_session()
    .await?;

let profile = api.oauth_profile().await?;
```

Required properties:

- secrets and providers are named builder values, never positional;
- credential source and HTTP placement remain separate concepts;
- incompatible material/placement combinations fail at the declaration span;
- endpoint-backed storage is inherent on the exact source request type;
- provider replacement is safe and typed where supported;
- no secret appears in generated `Debug`, errors, safe plans, hooks, or docs.

## 5.6 Pagination using extras controllers

```rust
use std::num::NonZeroU32;

use concord::{api, Json, PageLimit};
use concord::pagination::OffsetPagination;

api! {
    client DirectoryApi {
        base "https://directory.example.com/v1/";
    }

    GET list_users(
        offset: u64,
        limit: NonZeroU32,
    ) -> Json<Vec<User>> {
        path ["users"];
        query {
            "offset" = ep.offset;
            "limit" = ep.limit;
        }

        paginate OffsetPagination {
            offset: bind ep.offset,
            page_size: bind ep.limit,
        }
    }

    GET get_user(id: UserId) -> Json<User> {
        path ["users", ep.id];
    }
}
```

Usage:

```rust
let users = api
    .list_users(0, NonZeroU32::new(100).unwrap())
    .paginate(PageLimit::hard_cap(NonZeroU32::new(20).unwrap()))
    .await?;
```

The following must fail because `GetUserRequest` has no `paginate` method, without mentioning a missing extension trait:

```rust,compile_fail
api.get_user(UserId(42)).paginate(PageLimit::take(2));
```

Core retains sequential execution, cycle detection before repeated send, item/page/identity ceilings, fresh endpoint state per page, and all-or-error collection.

## 5.7 Custom request and response specifications

```rust
use concord::body::{
    BodyError, RequestBody, RequestBodyContext, RequestBodyMetadata, RequestBodySpec,
};
use concord::response::{
    Buffered, ResponseBodyError, ResponseBodySpec, ResponseHead,
};

pub struct AuditBody;

impl RequestBodySpec for AuditBody {
    type Input = AuditEnvelope;

    fn metadata(
        input: &Self::Input,
        context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, BodyError> {
        // Structural, bounded metadata only.
        todo!()
    }

    fn prepare(
        input: Self::Input,
        context: RequestBodyContext<'_>,
    ) -> Result<RequestBody, BodyError> {
        todo!()
    }
}

pub struct AuditResponse;

impl ResponseBodySpec for AuditResponse {
    type Output = AuditResult;
    type Mode = Buffered;

    fn from_body(
        body: Bytes,
        head: &ResponseHead<'_>,
    ) -> Result<Self::Output, ResponseBodyError> {
        todo!()
    }
}

api! {
    client AuditApi {
        base "https://audit.example.com/";
    }

    POST submit_audit(
        #[body] envelope: AuditBody,
    ) -> AuditResponse {
        path ["audits"];
    }
}
```

Usage remains ordinary:

```rust
let result: AuditResult = api.submit_audit(envelope).await?;
```

Advanced imports are appropriate only because the user intentionally implements semantic extension contracts.

## 5.8 Runtime-defined endpoint

```rust
use concord::endpoint::{EndpointId, RuntimeEndpoint};
use concord::http::Method;

let endpoint = RuntimeEndpoint::builder(
    EndpointId::new("catalog.search")?,
    Method::GET,
)
.path_segment("search")
.query("q", query)
.query_optional("cursor", cursor)
.response::<Json<SearchResults>>()
.build()?;

let results: SearchResults = api.request(endpoint).await?;
```

Runtime endpoint requirements:

- builder validation freezes an immutable inert value;
- generated client base URL and immutable runtime snapshot are reused;
- no native transport access is exposed;
- only public safe headers and semantic auth/rate objects are accepted;
- the same core lifecycle executes it;
- timeout state is a genuine tri-state;
- runtime pagination remains unsupported until a complete continuation/result contract is designed;
- runtime identities support bounded owned strings while generated static identities remain allocation-free where practical.

## 5.9 Reusable explicit endpoint values

```rust
use users_api::GetUser;

let endpoint = GetUser::new(UserId(42))
    .expand(Expand::Teams);

assert_eq!(endpoint.description().id(), "users.get");

let plan = api.plan(&endpoint)?;
assert_eq!(
    plan.safe_url(),
    "https://api.example.com/v1/users/<redacted>",
);

let first: User = api.request(endpoint.clone()).await?;
let second = api.request(endpoint).response().await?;
```

Generic composition:

```rust
fn bind_endpoint<E>(api: &UsersApi, endpoint: E) -> E::Request
where
    E: users_api::Endpoint,
{
    api.request(endpoint)
}
```

Generated endpoint values should have conditional `Clone` implementations whenever all stored values are safely cloneable. Reuse must not be tied incidentally to pagination.

---

# 6. Public-language assessment and target rules

## 6.1 One façade dependency

Target dependency:

```toml
[dependencies]
concord = {
    version = "...",
    features = ["json", "default-tls", "http2", "rate-limit-governor"]
}
```

Target imports:

```rust
use concord::{api, Json, PageLimit};
```

The technical package split remains, but ordinary users do not version or import it directly.

## 6.2 Generated preludes and extension traits

Remove generated per-client preludes and endpoint request extension traits.

Current behavior requires imports such as a generated `prelude::*` to make optional setters or pagination methods visible. That is an implementation workaround exposed as product API. It harms rust-analyzer completion and produces irrelevant trait-import diagnostics.

Generate named wrappers instead:

```rust
pub struct GetUserRequest {
    inner: concord::__private::Request<GetUser>,
}

impl GetUserRequest {
    pub fn expand(self, value: Expand) -> Self { /* map endpoint */ }
    pub fn clear_expand(self) -> Self { /* map endpoint */ }
    pub fn timeout(self, value: Duration) -> Self { /* delegate */ }
    pub fn response(self) -> concord::ResponseRequest<GetUser> { /* delegate */ }
}
```

Each wrapper implements `IntoFuture` and only the inherent capabilities valid for the endpoint.

A normal `concord::prelude` may remain as optional import convenience. It must never be required to activate generated methods.

## 6.3 Clause ordering

Named clauses are order-independent. For example, these are semantically equivalent:

```rust
GET item() -> Bytes {
    timeout 1s;
    rate_limit { 1 / 1s; }
}
```

```rust
GET item() -> Bytes {
    rate_limit { 1 / 1s; }
    timeout 1s;
}
```

Rules:

- each singular clause may appear at most once;
- duplicate errors show original and duplicate spans;
- list entry order remains meaningful in path atoms, query/header declarations, auth placements, and rate windows;
- formatting may choose a canonical presentation order;
- diagnostics speak in contract terms and never say “normative order.”

## 6.4 Client construction

Endpoint required arguments remain positional because they describe one operation and correspond to a Rust function signature.

Client configuration uses named setters because it is long-lived, heterogeneous, secret-bearing, and expected to evolve.

Rules:

- generate `Client::new()` only when there are no required client values, secrets, or provider instances;
- every configured client uses `Client::builder()`;
- required values, secrets, and providers have named setters;
- missing-input errors may name the source identifier but never format the value;
- do not use typestate by default; environment-driven configuration is naturally fallible at `build()`;
- repeated runtime/HTTP configuration calls compose deterministically in call order rather than creating duplicate-closure bookkeeping errors.

## 6.5 Naming

Target mapping:

| DSL identifier | Method/module | Type |
|---|---|---|
| `oauth` | `oauth()` | `Oauth` |
| `oauth_token` | `oauth_token()` | `OauthToken` |
| `league_entries` | `league_entries()` | `LeagueEntries` |
| `league_v4` scope | `league_v4()` / module `league_v4` | `LeagueV4` |

An optional explicit type-name override may be considered for acronym aesthetics, but it must not change the method name or identity unless explicitly requested.

## 6.6 Generated module shape

Target:

```text
riot_api/
  Endpoint
  platform/
    Platform
    league_v4/
      LeagueV4
      LeagueEntries
      LeagueEntriesRequest
```

The module that represents a scope owns its facade, child scopes, endpoints, and request wrappers. Users do not navigate generation artifact categories.

## 6.7 Scope values and endpoint values

- Required scope values remain facade method arguments.
- Optional/defaulted scope values use consuming inherent setters on the generated facade.
- Entering a child scope produces an immutable snapshot; it does not mutate the client.
- Required endpoint values and required body inputs are direct-method/constructor arguments.
- Optional/defaulted endpoint values become consuming inherent setters on both explicit endpoint values and bound request wrappers.
- Setters include `clear_*` for optional values and `reset_*` for defaults where the distinction is meaningful.

## 6.8 Body syntax

The body is not inferred from the parameter name `body` and is not required to be last.

Use:

```rust
POST create_user(
    tenant: TenantId,
    #[body] input: Json<CreateUser>,
    trace: Option<TraceId>,
) -> Json<User>;
```

The AST must retain the body parameter name, type, role span, and declaration position.

## 6.9 Response syntax

Keep arbitrary Rust response specification types in the return position:

```rust
-> Json<User>
-> Stream<OctetStream>
-> Records<NdJson<Event>>
-> ApplicationResponseSpec
```

The macro treats the type as opaque. Core owns sealed acquisition mode. A custom specification can transform bounded semantic input but cannot select an unbounded/native mode.

## 6.10 Authentication

Keep:

- credential source separate from placement;
- typed credential material;
- endpoint-backed credentials;
- arbitrary custom providers;
- challenge policies;
- protected auth headers/query material;
- private native application;
- one bounded recovery.

Change:

- no positional secrets;
- no extras-specific source syntax;
- endpoint storage and provider-replacement terminals are inherent;
- concrete provider objects are client-builder inputs;
- feature diagnostics name façade features and public paths.

## 6.11 Pagination

Keep arbitrary controller paths and explicit binding. The macro does not know built-in controller names.

Expose built-ins from:

```rust
concord::pagination::{OffsetPagination, PagePagination, CursorPagination}
```

Provide constructors:

```rust
PageLimit::take(NonZeroU32)
PageLimit::hard_cap(NonZeroU32)
```

Do not require callers to construct enum variants directly for ordinary use.

## 6.12 Rate limits

Keep lexical declaration, deterministic composition, bounded key construction, process-local default governor, custom limiter support, and fail-closed behavior.

Separate:

- remote quota declarations in the contract;
- concrete limiter and observer instances in client runtime configuration;
- stable contract identity from incidental Rust naming.

## 6.13 Contract versus runtime policy

Belongs in the declaration:

- topology and route composition;
- endpoint/scope parameters;
- body and response specifications;
- credential requirements and placement;
- endpoint-backed credential relationships that create generated terminals;
- pagination bindings;
- remote quota declarations;
- endpoint-specific execution defaults that materially belong to the endpoint contract, such as timeout override;
- stable endpoint ID.

Belongs on the generated client/runtime builder:

- concrete Reqwest retry configuration;
- concrete limiter instance;
- concrete hooks/observer instances;
- proxy, TLS, compression, and transport configuration;
- local finite limits;
- debug level;
- concrete provider construction.

## 6.14 rust-analyzer and diagnostic acceptance

The target editor experience is:

- hover on a direct endpoint call shows a named request type;
- completion after the call lists all and only valid inherent setters and terminals;
- no import is suggested merely to make a generated method exist;
- go-to-definition reaches the source declaration span where proc-macro span support permits, otherwise a stable generated rustdoc item;
- module/type names are predictable from source names;
- feature-disabled items are absent;
- cross-client endpoint errors mention public client/endpoint types;
- body/response/provider/controller trait failures anchor at the declared type span;
- generated-name collisions show both declarations;
- diagnostics never expose bridge type names, seal modules, or parser ordering implementation.

Completion inside a proc-macro token tree will remain less capable than completion in ordinary Rust. The generated call-site surface and generated rustdoc must compensate.

---
# 7. Derived core architecture

## 7.1 Semantic phases

```text
Parsed declaration
    ↓
Resolved contract and generated-surface model
    ↓
Generated immutable endpoint value
    ↓
Bound side-effect-free named request wrapper
    ↓ first poll
Prepared operation
    ↓
Attempt lifecycle
    ↓
Bounded response acquisition
    ↓
Fixed result
    ↓
Completion observation
```

These phases are semantically distinct. Do not collapse them merely to reduce type count.

## 7.2 Concept ownership matrix

| Concept | Data owner | Validation point | Immutability point | Public surface | Private invariant owner |
|---|---|---|---|---|---|
| Client declaration defaults | Macro normalized model | Compile time | Generated code | DSL/rustdoc | Macro resolver |
| Client runtime configuration | Generated builder + core builders | Builder setters and `build()` | `ClientCore` construction | Named generated builder, domain builders | Core |
| Runtime snapshot | `ClientCore` shared by `Arc` | `build()` | Immediately after build | Indirect through generated client | Core |
| Endpoint description | Generated static descriptor or owned runtime descriptor | Macro resolve/runtime builder | Endpoint creation/build | Read-only safe API | Macro/core |
| Endpoint value | Generated type or `RuntimeEndpoint` | Compile time/runtime builder | Consuming builder completion | Public concrete value | Generated bridge/core |
| Bound request | Generated request wrapper containing core request state | Capability legality at compile time | Every consuming setter | Public named wrapper | Core generic request |
| Public request head | Core head builder | Request preparation/planning | Prepared head | Sanitized plan only | Core |
| Body specification | User/extra type and endpoint input | Trait checking; metadata/limit at execution | Body input moves into pending request | Core trait through façade | Core owns phases/native body |
| Response specification | User/extra type | Trait checking and runtime media/decode checks | Type-level | Core trait through façade | Core sealed acquisition mode |
| Authentication | Generated credential store + core slots/providers | Compile-time source/placement; build-time providers; execution-time versions | Client snapshot plus versioned slots | Safe handles/material contracts | Core |
| Rate limiting | Macro lexical plan + runtime limiter | Compile-time plan; build-time limiter; execution-time key/authority | Client snapshot and operation plan | Contracts in core, implementations in extras | Core execution + limiter implementation |
| Pagination | Generated bindings + controller + core loop | Compile-time bindings; runtime state/ceilings | Per-operation controller state | Core contracts, extra controllers | Core loop |
| Hooks | Runtime snapshot | Build time | Client build | Sanitized event traits | Core ordering/result fixation |
| Redaction | Core | Every public projection/error/event | Continuous | Safe wrappers | Core |
| Completion | Core executor | After result fixation | Event creation | Sanitized event | Core |
| Dangerous capabilities | Façade feature + explicit domain API | Cargo/compile time | Built client/request | `concord::dangerous` | Core |
| Extras | Concrete local types | Their builders/trait callbacks | Per implementation | Reexported by façade | Must obey core contracts |

## 7.3 One managed lifecycle

The current single managed lifecycle is correct and must be preserved. Generated calls, explicit endpoints, and runtime endpoints converge before effects begin.

Observable ordering remains conceptually:

```text
public endpoint/head preparation
→ known finite checks
→ before-send hook
→ body materialization and metadata consistency
→ credential preflight
→ limiter acquisition
→ credential finalization
→ private native request/auth application
→ managed transport
→ sanitized response-head observation
→ limiter observation
→ bounded challenge/status handling
→ bounded response acquisition
→ fixed result
→ completion
```

Do not simplify by:

- letting codecs/body specs construct native requests;
- exposing native responses to response specs;
- adding an alternate transport/executor;
- putting authentication material in public logical headers;
- letting pagination own another execution pipeline;
- running pages concurrently under current semantics;
- returning partial results from the existing pagination terminal;
- detaching result-determining completion work.

## 7.4 Legitimate internal phase models

Keep distinct:

- logical auth plan, prepared auth, finalized auth, applied native auth;
- request body metadata, prepared managed body, native body;
- response specification and sealed acquisition mode;
- direct/page completion and aggregate pagination completion;
- logical public head and private native request;
- runtime configuration builder and immutable runtime snapshot.

These are not duplicate models; they encode phase boundaries and security invariants.

## 7.5 Accidental duplicate models to remove

- public `facades` and `endpoints` generated trees;
- endpoint setters duplicated into extension traits;
- documentation-only methods generated to hide private generic bounds;
- `PreparedEndpoint` naming beside internal prepared phases;
- canonical examples recreated in tests;
- core-owned concrete implementations duplicated through feature/prelude docs;
- manually repeated feature/default/export tables.

## 7.6 Safe planning semantics

The planning API is a sanitized, side-effect-free projection of the actual public-head construction path.

It may:

- resolve fixed base, host, path, and public query values;
- validate public URL structure;
- construct safe logical URL/redacted URL;
- validate and include public headers after sanitization;
- expose method, endpoint ID, route template, timeout selection, declared auth/rate capability metadata, and response acquisition category;
- perform deterministic conversion of endpoint values already required for URL/header shaping.

It must not:

- call hooks;
- invoke `RequestBodySpec::metadata` or `prepare` unless the product explicitly accepts extension callbacks in planning; the selected target is **not to invoke them**;
- encode a body;
- create a body/stream/multipart factory result;
- poll a stream;
- acquire/finalize credentials;
- acquire/observe a limiter;
- create a native request;
- send transport;
- expose protected auth placement or secret values.

If exact body metadata cannot be obtained without extension callbacks, the plan reports the body specification identity/category available statically and marks dynamic metadata as unavailable.

---

# 8. Current repository architecture map

This section records the reviewed starting point so the implementing agent can distinguish intentional migration from accidental regression.

## 8.1 Current workspace

```text
concord_core/          Runtime, public API, concrete extras, tests
concord_macros/        Parser, resolver, code generation, UI and fixture tests
concord_examples/      Shared extension types, main binary, independent examples
concord_test_support/  Deterministic and loopback support
perf/                  Benchmark package
 docs/                 User/reference/contributor manual
```

Root workspace members are currently:

```text
concord_core
concord_macros
concord_test_support
concord_examples
perf
```

The current README requires two product dependencies and imports `concord_core::prelude::*` plus `concord_macros::api`.

## 8.2 Current core

Strengths:

- immutable `ClientCore` snapshot;
- one executor;
- mature authentication phase model;
- private native HTTP boundary;
- bounded request/response/stream/status/provider behavior;
- deterministic sequential pagination;
- rate-limit plans and fail-closed no-governor behavior;
- safe hook/error/redaction types;
- sealed response acquisition modes;
- extensive deterministic tests.

Current concrete implementations mixed into core:

- JSON codec in `src/codec/mod.rs`;
- CSV and NDJSON decoders in `src/records`;
- built-in pagination controllers in `src/pagination/controller.rs`;
- governor in `src/rate_limit/governor.rs`;
- OAuth2 client credentials in `src/auth/oauth2.rs`.

Current coupling that must be removed during extraction:

- macro detects OAuth use and emits a JSON feature assertion;
- OAuth2 is gated by the core `json` feature;
- runtime config constructs/downcasts `GovernorRateLimiter` directly;
- governor reads a core task-local cooldown cap;
- core prelude/advanced modules combine contracts and concrete extras;
- core default features own the user-facing default governor.

## 8.3 Current macro

Current high-level pipeline is correct:

```text
parse.rs → resolve.rs → codegen.rs
```

Current source files:

```text
ast.rs
model.rs
parse.rs
resolve.rs
codegen.rs
names.rs
crate_path.rs
```

Strengths:

- normalized IR;
- lexical inheritance;
- reference resolution;
- generated-name collision validation;
- credential and auth placement checks;
- pagination binding checks;
- lexical rate-limit origin normalization;
- generated contract handshake;
- dependency rename support for core.

Current public-surface problems:

- exact clause ordering is parser-enforced;
- endpoint names are PascalCase and method names are derived;
- body role is inferred from the name `body` and final position;
- optionality is syntax-inspected but insufficiently specified;
- generated support has separate facade/endpoint trees;
- optional request methods and pagination are extension traits;
- generated per-client prelude is required;
- configured client constructor is positional;
- codegen detects JSON through OAuth and emits core feature docs/assertions;
- generated code resolves `concord_core` directly;
- public docs and bridge generation are interleaved in one emitter.

## 8.4 Current examples

Current examples are separate files under `concord_examples/examples`, while `src/lib.rs` contains shared extension types and `tests/canonical_examples.rs` defines a separate synthetic `DeterministicApi`.

The current Riot example combines domain types, two APIs, observers, usage, and live behavior in one file of roughly 1,350 lines. The records example performs external-host-shaped execution rather than serving as a deterministic canonical proof.

## 8.5 Current tests and fixtures

Current tests include strong core unit/integration coverage, macro generated-surface tests, Trybuild UI tests, core feature fixtures, and a nested macro fixture workspace with twelve packages.

Several standalone fixtures prove assertions that can live in normal façade integration tests. There are multiple lockfiles and editor roots.

## 8.6 Current documentation/tooling mismatches

- exact features, limits, syntax, and exports have multiple owners;
- the learning path introduces runtime detail early;
- manual examples and executable examples are disconnected;
- README lists a `scripts/` directory that is absent;
- contributor docs call `python scripts/check.py`, which is absent;
- `justfile` globally selects `powershell.exe`, contradicting portability;
- normal pinned toolchain and declared MSRV are not distinct explicit validation lanes.

---

# 9. Mismatch analysis

| Area | Current contradiction | Product consequence | Target correction |
|---|---|---|---|
| Packaging | Two required product crates | Users understand implementation/version pairing | One `concord` dependency |
| Core ownership | Concrete codecs/controllers/governor/OAuth in core | Feature graph and semantic kernel are conflated | Extract public-trait implementations to extras |
| Macro neutrality | OAuth causes JSON name/feature knowledge | Macro knows an implementation detail | Generic provider; opaque body/response types |
| Generated path | Expansion resolves `concord_core` | One façade dependency is impossible | Resolve renamed `concord`; emit `concord::__private` |
| Methods | Optional/pagination methods are traits | Weak completion and trait-import errors | Named request wrappers with inherent methods |
| Names | Pascal source names indirectly produce method names | Call-site name is not explicit | Snake-case declaration names |
| Modules | Separate `facades` and `endpoints` | Users navigate generator internals | One scope tree |
| Client construction | Values/secrets/providers positional | Easy swaps and poor evolution | Named builder |
| Grammar | Fixed clause order | Parser mechanics become language semantics | Order-independent clauses |
| Body | Identifier spelling and position determine role | Hidden rule and poor locality | `#[body]` parameter role |
| OAuth | Built-in macro syntax tied to core JSON | Extras separation is impossible | Ordinary `CredentialProvider` from extras |
| Governor | Core downcast/task-local coupling | Concrete strategy contaminates kernel | Explicit limiter metadata/observation context |
| Runtime policy | Custom retry in DSL | Transport policy leaks into contract | Generated client HTTP builder |
| Runtime endpoint | `Inherit` and `Disabled` can collapse | Distinct public operations share meaning | True tri-state; rename `RuntimeEndpoint` |
| Explicit endpoint values | Deep paths and selective clone | Composition/reuse is secondary | Predictable paths, descriptions, conditional clone |
| Rustdoc | Documentation-only substitutes | Docs compensate for poor public types | Real named request methods |
| Examples | Independent binaries | Canonical code cannot be imported/tested | Library modules + thin binaries |
| Example proof | Synthetic API claims canonical proof | Tests prove a different product | Import actual declarations |
| Fixtures | Enabled-feature/extension proofs isolated | Fragmented Cargo/rust-analyzer state | Main-workspace façade tests |
| Docs | Several owners for exact facts | Drift | One canonical owner per fact category |
| Tooling | Missing scripts and PowerShell shell | Contributor commands are stale/nonportable | Portable `just` and optional Rust `xtask` |

---

# 10. Keep, change, remove

| Decision | Item | Rationale |
|---|---|---|
| Keep | Function-like contract DSL | Best fit for hierarchy, inheritance, endpoint values, auth, pagination, and lexical quotas |
| Keep | Exactly one client per `api!` invocation | Clear namespace and ownership |
| Keep | Signature-like endpoint declarations | Strong Rust correspondence |
| Keep | Constrained request-shaping expression language | Deterministic, auditable, redaction-aware |
| Keep | Scope facade chaining | Natural remote topology |
| Keep | Explicit generated endpoint structs | Storage, composition, generic execution, testing |
| Keep | Arbitrary Rust body/response/controller/provider types | Correct extension seam |
| Keep | Sealed response modes | Prevents native/unbounded acquisition |
| Keep | Immutable runtime snapshot | Correct lifecycle ownership |
| Keep | One managed executor | Central product guarantee |
| Keep | Private native transport/auth boundary | Security and semantic isolation |
| Keep | Bounded auth recovery | Intrinsic behavior correctly centralized |
| Keep | Sequential all-or-error pagination | Coherent result/completion semantics |
| Keep | Lexical rate-limit plan | Predictable and redaction-safe |
| Keep | Generated contract versioning | Necessary private bridge guard |
| Keep | Dependency rename support | Normal Cargo behavior |
| Change | Public package model | Add `concord` façade |
| Change | Concrete implementations | Move trait implementations to extras as specified |
| Change | Macro path | Resolve façade and target hidden façade bridge |
| Change | Endpoint declaration names | Use snake_case directly |
| Change | Request surface | Generate named wrappers/inherent methods |
| Change | Generated modules | Merge by scope |
| Change | Client construction | Named builder; no positional secrets |
| Change | Clause parsing | Order-independent named clauses |
| Change | Body syntax | Explicit role |
| Change | OAuth | Generic provider from extras |
| Change | Governor integration | Core contract + façade default composition |
| Change | Endpoint generic capability | Document sealed endpoint trait and description |
| Change | Runtime endpoint | Rename and correct timeout/identity semantics |
| Change | Public modules | Domain-oriented façade paths |
| Change | Examples/tests/docs/workspace | Rebuild around product contract |
| Remove | Generated per-client preludes | Inherent methods make them unnecessary |
| Remove | Generated request extension traits | Macro convention should not leak into usage |
| Remove | Fixed-order UI fixtures | They protect parser leakage |
| Remove | Configured positional `new` | Unsafe evolution and poor readability |
| Remove | Public facade/endpoint namespace split | Duplicate representation |
| Remove | Macro JSON/OAuth name detection | Violates type neutrality |
| Remove | Core JSON/CSV/NDJSON/governor/OAuth concrete implementations | New extras ownership |
| Remove | Synthetic canonical example API | Does not prove examples |
| Remove | External-network canonical example behavior | Not deterministic |
| Remove | Standalone enabled-feature fixtures | Workspace matrix can prove them |
| Remove | Standalone extension certification package | Façade integration tests are the downstream boundary |
| Remove | Source scans as primary privacy proof | Compiler/API tests are stronger |
| Remove | Manual comprehensive export index | Rustdoc owns exact exports |
| Remove | Stale scripts and PowerShell-only shell selection | Must match actual portable workspace |

---
# 11. Target feature topology and public namespaces

## 11.1 Feature ownership

The façade is the single user-facing feature owner. Internal packages use empty defaults so feature behavior is explicit and testable.

### `concord_core`

Recommended final features:

```toml
[features]
default = []
default-tls = ["dep:rustls", "reqwest/rustls-no-provider"]
http2 = ["reqwest/http2"]
records = []
multipart = ["reqwest/multipart"]
gzip = ["reqwest/gzip"]
brotli = ["reqwest/brotli"]
deflate = ["reqwest/deflate"]
dangerous-raw-response = []
dangerous-custom-retry = []
dangerous-dev-tools = ["dep:concord_test_support"]
```

Remove from core:

```text
json
csv
ndjson
rate-limit-governor
```

Remove Serde, Serde JSON, and CSV normal dependencies from core after OAuth extraction. Keep `mime` because core validates media types for primitive codecs/streaming and public body/response contracts.

### `concord_extras`

Recommended final features:

```toml
[features]
default = []
json = ["dep:serde", "dep:serde_json"]
csv = ["concord_core/records", "dep:csv", "dep:serde"]
ndjson = ["concord_core/records", "dep:serde", "dep:serde_json"]
rate-limit-governor = []
oauth2-client = ["dep:serde_json"]
```

Built-in pagination controllers are dependency-free and available without a feature. If compile footprint evidence justifies a `pagination` feature, the façade must enable it by default and document it; do not add a feature merely for symmetry.

### `concord`

Recommended final features:

```toml
[features]
default = ["default-tls", "http2", "rate-limit-governor"]

default-tls = ["concord_core/default-tls"]
http2 = ["concord_core/http2"]
json = ["concord_extras/json"]
records = ["concord_core/records"]
csv = ["records", "concord_extras/csv"]
ndjson = ["records", "concord_extras/ndjson"]
multipart = ["concord_core/multipart"]
rate-limit-governor = ["concord_extras/rate-limit-governor"]
oauth2-client = ["concord_extras/oauth2-client"]
gzip = ["concord_core/gzip"]
brotli = ["concord_core/brotli"]
deflate = ["concord_core/deflate"]
dangerous-raw-response = ["concord_core/dangerous-raw-response"]
dangerous-custom-retry = ["concord_core/dangerous-custom-retry"]
dangerous-dev-tools = ["concord_core/dangerous-dev-tools"]
```

Use exact-version internal dependencies and `default-features = false` on every edge.

### `concord_macros`

No runtime feature mirror. The macro may know syntax-level dangerous constructs while they exist, but it does not mirror format, controller, provider, or limiter features.

## 11.2 Feature behavior rules

- A type-based optional capability is represented by a conditional façade reexport. The macro does not scan the type spelling to emit a feature assertion.
- A syntax-level capability may emit a targeted façade-feature assertion only while that syntax exists.
- `csv` and `ndjson` imply generic records support.
- `oauth2-client` is independent from the ordinary `json` codec feature even though both use Serde JSON internally.
- Disabling `rate-limit-governor` preserves fail-closed behavior for generated non-empty rate plans unless the user supplies a custom limiter.
- No default feature in core or extras can silently alter behavior when those crates are tested directly.

## 11.3 Public namespace target

Top-level common surface:

```rust
pub use concord_macros::api;
pub use bytes::Bytes;
pub use core request/error/common response types;
#[cfg(feature = "json")]
pub use concord_extras::codec::Json;
pub use core primitive codecs such as Text and OctetStream;
pub use core PageLimit;
pub use extras pagination controllers;
```

Domain modules:

```text
concord::auth
concord::body
concord::codec
concord::endpoint
concord::error
concord::hooks
concord::http
concord::pagination
concord::rate_limit
concord::records
concord::response
concord::runtime
concord::dangerous        (feature-gated)
concord::advanced         (curated compatibility/convenience reexports)
concord::prelude          (optional convenience only)
concord::__private        (doc-hidden generated bridge)
```

Rules:

- Advanced users implement traits through `concord::<domain>`, never through documented `concord_core` paths.
- `advanced` should not be the only discoverable home for a semantic domain.
- Reexports preserve type identity; do not wrap core/extra types solely to change their path.
- Do not publicly reexport `concord_core` or `concord_extras` as broad modules. That recreates the implementation split.
- `__private` may reexport core bridge items and add façade composition functions, but it is not an extension surface.

## 11.4 Hidden façade bridge

The façade bridge has three responsibilities:

1. Reexport core generated-contract primitives and type aliases.
2. Compose optional extras into generated defaults without making the macro name them.
3. Keep generated code valid under a renamed façade dependency.

Example conceptual functions:

```rust
#[doc(hidden)]
pub use concord_core::__private::{
    assert_generated_contract,
    build_client_core,
    request,
    map_endpoint,
    // ...narrow core bridge...
};

#[doc(hidden)]
pub fn default_runtime_config() -> RuntimeConfigBuilder {
    let runtime = concord_core::__private::default_runtime_config();
    #[cfg(feature = "rate-limit-governor")]
    let runtime = concord_core::__private::install_generated_default_limiter(
        runtime,
        Arc::new(concord_extras::rate_limit::GovernorRateLimiter::new()),
    );
    runtime
}
```

The final signatures must be designed to preserve explicit-user-limiter precedence and observer semantics. The macro calls these façade functions but remains unaware of the concrete type.

---

# 12. Target macro architecture

## 12.1 Pipeline

Keep:

```text
parse → resolve/normalize → emit
```

The normalized model is essential. Do not generate directly from syntax AST.

## 12.2 Target source tree

```text
concord-macros/src/
  lib.rs
  crate_path.rs

  syntax/
    mod.rs
    api.rs
    values.rs
    request_shape.rs
    credentials.rs
    authentication.rs
    pagination.rs
    rate_limits.rs
    diagnostics.rs

  semantic/
    mod.rs
    model.rs
    symbols.rs
    names.rs
    inherit.rs
    values.rs
    authentication.rs
    pagination.rs
    rate_limits.rs
    surface.rs
    validate.rs

  emit/
    mod.rs
    client.rs
    scopes.rs
    endpoints.rs
    requests.rs
    credentials.rs
    descriptions.rs
    docs.rs
    bridge.rs
    assertions.rs
```

This is a hybrid organization:

- syntax is divided by grammar domain for local diagnostics and evolution;
- semantic domains share one symbol/inheritance infrastructure;
- a first-class `surface` model owns every generated public name and capability;
- emission is divided by generated artifact for auditability;
- bridge emission is isolated from public API emission;
- generated docs consume the same surface model as code.

Do not split files merely because they are large. Every split must improve language evolution, diagnostic precision, generated-code auditability, test locality, contributor comprehension, compile behavior, or semantic separation.

## 12.3 Resolved surface model

Before token emission, each endpoint should have a model similar to:

```rust
struct EndpointSurface {
    method_name: Ident,
    endpoint_type_name: Ident,
    request_type_name: Ident,
    module_path: Vec<Ident>,
    stable_id: ContractId,
    required_arguments: Vec<ArgumentSurface>,
    optional_setters: Vec<SetterSurface>,
    default_setters: Vec<SetterSurface>,
    capabilities: CapabilitySet,
    description: EndpointDescriptionModel,
    feature_requirements: FeatureSet,
}
```

`CapabilitySet` determines emitted inherent methods and should include at least:

```text
await_output
response_metadata
pagination
endpoint_credential_store targets
raw_response eligibility
timeout controls
safe planning
clone eligibility
```

Code generation must not rediscover these decisions.

## 12.4 Validation placement

| Validation | Owner |
|---|---|
| Token grammar and malformed literals | Syntax parser |
| Duplicate named clauses | Syntax parser with both spans |
| Static base URL/host/path/header atoms | Syntax parser |
| Name/reference resolution | Semantic symbols/resolver |
| Scope inheritance | Semantic inheritance |
| Generated-name collisions | Surface resolver |
| Credential cycles/material/placement | Auth semantic resolver |
| Pagination bindings/body incompatibility | Pagination semantic resolver |
| Lexical rate-limit ancestry/cost/windows | Rate-limit semantic resolver |
| Rust trait implementation for arbitrary types | Generated compile assertions at the type span |
| Façade feature availability for syntax-level constructs | Generated façade assertion |
| Dynamic URL/header/body/auth/key values | Core runtime preparation |
| Runtime endpoint structural validity | Runtime endpoint builder |
| Finite limits/transport effects | Core executor |

Emission should not discover user semantic errors. Impossible emitter states are internal assertions.

## 12.5 Crate path resolution

Final `crate_path.rs` resolves package `concord`:

```rust
match proc_macro_crate::crate_name("concord") {
    FoundCrate::Itself => quote!(crate),
    FoundCrate::Name(name) => quote!(::#name),
    error => focused message requesting the `concord` dependency,
}
```

Direct `concord_macros` use without the façade is unsupported in the final product.

## 12.6 Type neutrality

Delete:

- `client_uses_json` and equivalent name/token scans;
- `assert_json_feature` generated logic;
- docs that claim request/response features are core-owned;
- built-in OAuth-to-JSON coupling;
- tests whose desired behavior depends on macro recognition of an extra type name.

Add tests proving:

- a local type named `Json` receives no special treatment;
- an aliased extras type works;
- a fully qualified extras type works;
- a third-party custom type works with no macro dependency edge;
- the macro package graph contains no extras dependency.

## 12.7 Code generation size

Named request wrappers add concrete methods but remove generated extension traits, seal modules for those traits, prelude aliases, trait docs, and rustdoc substitutes. Measure rather than assume.

Track emitted token/item counts and clean/incremental check time for generated APIs with approximately 1, 10, 100, and 1,000 endpoints.

---

# 13. Ideal workspace tree

```text
Cargo.toml
Cargo.lock
rust-toolchain.toml
rustfmt.toml
deny.toml
justfile
CONCORD_REFACTOR_REFERENCE.md

crates/
  concord/
    Cargo.toml
    src/
      lib.rs
      prelude.rs
      advanced.rs
      dangerous.rs
      private.rs
    tests/
      generated_surface.rs
      generated_surface/
      public_extensions.rs
      public_extensions/
      deterministic_generated_runtime.rs
      public_compile.rs
      rustdoc.rs
      ui.rs
      ui/pass/
      ui/fail/

  concord-core/
    Cargo.toml
    src/
      lib.rs
      auth/
      body/
      client/
      codec/
      endpoint/
      error/
      execute/
      hooks/
      http/
      operation/
      pagination/
      rate_limit/
      records/
      redaction/
      request/
      response/
      runtime/
      __private.rs
      __development.rs
    tests/
      lifecycle.rs
      lifecycle/
      security.rs
      security/
      bounds.rs
      bounds/
      transport.rs
      configuration.rs
      public_types.rs
      ui.rs
      ui/pass/
      ui/fail/

  concord-extras/
    Cargo.toml
    src/
      lib.rs
      auth/
        mod.rs
        oauth2_client.rs
      codec/
        mod.rs
        json.rs
      pagination/
        mod.rs
        offset.rs
        page.rs
        cursor.rs
      rate_limit/
        mod.rs
        governor.rs
        retry_after.rs
      records/
        mod.rs
        csv.rs
        ndjson.rs
    tests/
      public_contract.rs
      oauth2_runtime.rs
      governor_runtime.rs
      records_runtime.rs

  concord-macros/
    Cargo.toml
    src/
      lib.rs
      crate_path.rs
      syntax/
      semantic/
      emit/
    tests/
      semantic_surface.rs
      emitted_surface.rs

  concord-test-support/
    Cargo.toml
    src/
      deterministic/
      loopback/
      tls/
      cargo_fixture/

  concord-examples/
    Cargo.toml
    src/
      lib.rs
      getting_started/
      contracts/
      authentication/
      request_response/
      pagination/
      rate_limiting/
      runtime_endpoints/
      dangerous/
      riot/
      bin/
    tests/
      construction.rs
      local_runtime.rs
      riot_contract.rs

tools/
  xtask/
    Cargo.toml
    src/main.rs
  perf/
    Cargo.toml
    benches/

fixtures/
  Cargo.toml
  Cargo.lock
  dependency/
    renamed_facade/
    contract_mismatch/
  features/
    Cargo.toml
    src/bin/
    tests/

docs/
  README.md
  SUMMARY.md
  guide/
  concepts/
  recipes/
  reference/
  contributor/
```

Moving existing packages under `crates/` and `perf` under `tools/` is a late organizational phase after semantic/public work is stable. Do not combine large path moves with core behavior changes.

Essential boundaries:

- façade versus proc macro: required by Rust packaging;
- macro versus runtime core: distinct compile/runtime roles;
- extras versus core: deliberate extension-contract proof and dependency ownership;
- test support: shared deterministic infrastructure;
- examples: importable narrative;
- performance tools: benchmark-only dependencies.

Historical/testing-driven boundaries to remove:

- standalone extension certification package;
- package per enabled feature;
- separate fixture workspaces/lockfiles under product crates;
- synthetic canonical example API.

---
# 14. Examples as the executable product narrative

## 14.1 Target examples tree

```text
concord-examples/src/
  lib.rs

  getting_started/
    mod.rs
    minimal.rs

  contracts/
    mod.rs
    full_dsl.rs
    regional_scopes.rs
    explicit_endpoints.rs

  authentication/
    mod.rs
    api.rs
    extensions.rs
    usage.rs

  request_response/
    mod.rs
    bodies.rs
    specifications.rs
    records.rs

  pagination/
    mod.rs
    offset.rs
    page_number.rs
    cursor.rs
    custom.rs

  rate_limiting/
    mod.rs
    lexical.rs
    custom_limiter.rs
    observer.rs

  runtime_endpoints/
    mod.rs
    generated_and_runtime.rs

  dangerous/
    mod.rs
    raw_response.rs
    custom_retry.rs

  riot/
    mod.rs
    domain/
      mod.rs
      routing.rs
      account.rs
      league.rs
      match_api.rs
      status.rs
      tournament.rs
      data_dragon.rs
    api/
      mod.rs
      riot.rs
      data_dragon.rs
    usage.rs

  bin/
    minimal.rs
    riot.rs
    records.rs
```

Binaries parse environment/arguments, call imported module functions, and report results. They do not declare APIs.

## 14.2 Current example disposition

| Current file | Target module | Required façade features | Teaches | Proof strategy | Documentation owner |
|---|---|---|---|---|---|
| `src/main.rs` | `getting_started::minimal` | product defaults; `json` only if the tutorial uses it | Small declaration, build, direct await, metadata | Compile and deterministic construction/plan | Guide 1–3 |
| `full_dsl_reference.rs` | `contracts::full_dsl` | as declared | Complete syntax topology | Compile + generated surface assertions | `reference/dsl.md` |
| `regional_api.rs` | `contracts::regional_scopes` | `json`, governor if quotas execute | Multi-host scopes, inherited auth, lexical limits | Safe-plan + local scripted runtime | `recipes/regional-apis.md` |
| `authentication.rs` | `authentication::{api,usage}` | `json`, `oauth2-client` where shown | Static, endpoint-backed, OAuth, custom providers | Deterministic provider/application runtime | Authentication guide |
| `body_workflows.rs` | `request_response::bodies` | `json`, `multipart` | Buffered/stream/custom/multipart bodies | Local scripted runtime | Bodies/responses guide |
| `generic_specifications.rs` | `request_response::specifications` | base | Custom request/response traits | Public compile test + local runtime | Extension concept |
| `pagination.rs` | `pagination::*` | `json` | Built-in extras controllers and custom controller | Local multi-page runtime | Pagination guide |
| `prepared_endpoint.rs` | `runtime_endpoints::generated_and_runtime` | base | Explicit generated versus runtime endpoint | Build, plan, local execution | Endpoint guide |
| `rate_limiting.rs` | `rate_limiting::*` | governor for default; base for custom | Lexical plan, custom limiter, observer | Deterministic permits/observations | Pagination/rate guide |
| `records.rs` | `request_response::records` | `csv`, `ndjson` | Generic record stream and concrete decoders | Local chunked scripted responses | Bodies/records guide |
| `dangerous_raw_response.rs` | `dangerous::raw_response` | `dangerous-raw-response`, likely `json` | Explicit bounded raw success | Compile capability + local runtime | Dangerous reference |
| `dangerous_custom_retry.rs` | `dangerous::custom_retry` | `dangerous-custom-retry` | Explicit runtime transport retry policy | Compile + deterministic attempt accounting | Dangerous reference |
| `riot.rs` | `riot::{domain,api,usage}` | `json`, governor; live binary also TLS/Tokio | Real product-scale API | Compile actual APIs; deterministic representative tests; manual live binary | `recipes/riot.md` |

## 14.3 Example rules

1. Every canonical `api!` declaration lives in an importable module.
2. No binary contains an independent `api!` declaration.
3. A test that claims to validate an example imports that exact module.
4. CI performs no external HTTP.
5. Live examples are manual targets and excluded from deterministic gates.
6. Compile-only examples still have a surface/construction proof.
7. Example features are learning/capability bundles, not a complete duplicate of the façade feature graph.
8. Documentation links to stable module rustdoc/source sections rather than reproducing complete programs.
9. Examples depend only on `concord`, never on core, extras, or macros directly.
10. Example custom extensions import traits from façade domain modules, proving the documented user path.

---

# 15. Test architecture

## 15.1 Proof taxonomy

| Proof type | Owner |
|---|---|
| Parser grammar and clause permutations | Macro syntax unit tests |
| Resolver normalization and semantic validation | Macro semantic unit tests |
| Generated names/capability surface | Façade integration tests + macro surface-model unit tests |
| Compiler diagnostics | Façade Trybuild tests |
| Public API compilation | Façade integration tests compiled as an external crate |
| Core deterministic lifecycle | Core integration tests |
| Concrete extras behavior | Extras unit/integration tests |
| Generated-to-runtime integration | Façade deterministic integration tests |
| Feature absence/unification | Isolated root feature fixture workspace |
| Dependency rename/replacement/contract mismatch | Isolated root dependency fixtures |
| Private boundary | External compile-fail tests + public-only integration tests |
| Redaction/security | Core security tests; extras-specific redaction tests where applicable |
| Documentation | Doctests, rustdoc, links to imported examples |
| Examples | `concord-examples` tests importing actual modules |
| Performance | `tools/perf` smoke and baselines |
| rust-analyzer | Manual release acceptance protocol plus signature/surface tests |

## 15.2 Existing family classification

Category key:

1. ordinary unit test;
2. crate integration test;
3. Trybuild compile-pass/fail;
4. main-workspace feature matrix;
5. genuinely separate Cargo graph;
6. duplicate/obsolete proof.

| Existing family | Category | Final disposition |
|---|---:|---|
| Core in-module tests | 1 | Keep near semantic code; split only when locality improves |
| Core public API/runtime/status/redaction/pagination/auth tests | 2 | Regroup by lifecycle/security/bounds/transport |
| Core UI tests | 3 | Keep boundary/security cases; consolidate harnesses |
| Core `config_features` fixture | 5 | Merge into one root feature fixture workspace |
| Macro parser/resolver/name/codegen tests | 1 | Keep with new syntax/semantic/emit owners |
| Macro generated integration tests | 2/4 | Move public generated surface/runtime proofs to façade |
| Macro Trybuild suite | 3 | Move public-language diagnostics to façade; delete accidental-order/prelude tests |
| Macro fixture driver | Mixed | Split ordinary proofs from graph-dependent proofs |
| Dependency policy test | 4 | Keep as workspace metadata assertion |
| Broad rustdoc HTML checks | 4/docs | Retain only focused privacy/path smoke; prefer Rust signatures/doctests |
| Source string scans in extension certification | 6 | Remove as primary proof |
| Example target compile checks | 4 | Keep with explicit module proof mapping |
| Synthetic `canonical_examples.rs` API | 6 | Delete; import real modules |
| Performance package | performance | Keep, migrate to façade |

## 15.3 Fixture decision table

| Current fixture | Final category | Decision | Target proof |
|---|---:|---|---|
| `concord_core/tests/fixtures/config_features` | 5 | Merge | Root `fixtures/features` bins for core/façade absence |
| `contract_mismatch` | 5 | Keep only if the private contract guard cannot be proven equivalently in workspace | Adapt to façade/core bridge replacement and document unsupported graph |
| `contract_mismatch/incompatible_core` | support | Keep only as nested support package | Not an independent test owner |
| `custom_retry_disabled` | 5 | Merge | Feature fixture proving dangerous API absence |
| `custom_retry_enabled` | 4 | Remove standalone | Façade all-features generated-surface test |
| `development_tools_enabled` | 4/6 | Remove standalone | Workspace all-features/perf build |
| `extension_certification` | 2/4 | Remove standalone | Façade `public_extensions` integration tests |
| `json_disabled` | 5 | Merge | Façade feature fixture; macro remains type-neutral |
| `multipart_disabled` | 5 | Merge | Façade/core feature fixture |
| `missing_core` | 6 after façade | Remove | Direct macro use is unsupported; users depend on façade |
| `no_governor_rate_limits` | 5 | Keep assertion, merge | No-default façade fixture proving fail-closed build/runtime |
| `oauth_disabled` | 5 | Merge | Absence of `OAuth2ClientCredentials` reexport; no macro special branch |
| `raw_response_disabled` | 5 | Merge | Dangerous method/type absence |
| `renamed_core` | 5 | Adapt and keep | Rename façade package and prove generated path resolution |
| Compression/TLS/HTTP2 absence bins | 5 | Merge | Root feature fixture |

Final independent fixture package count should be small:

```text
fixtures/features
fixtures/dependency/renamed_facade
fixtures/dependency/contract_mismatch   # only if retained after review
```

Use one fixture workspace lockfile.

## 15.4 Target core test tree

```text
concord-core/tests/
  lifecycle.rs
  lifecycle/
    laziness.rs
    preparation.rs
    hook_order.rs
    auth_recovery.rs
    cancellation.rs
    completion.rs
    streaming.rs

  security.rs
  security/
    redaction_matrix.rs
    protected_urls.rs
    auth_material.rs
    status_bodies.rs
    native_boundary.rs

  bounds.rs
  bounds/
    request_body.rs
    response_body.rs
    streams.rs
    records.rs
    pagination.rs
    limiter_keys.rs

  transport.rs
  configuration.rs
  public_types.rs
  ui.rs
  ui/pass/
  ui/fail/
```

Core lifecycle tests use local test providers/controllers/limiters/observers. They must not depend on extras for convenience.

## 15.5 Target extras test tree

```text
concord-extras/tests/
  public_contract.rs
  json.rs
  records.rs
  pagination.rs
  governor.rs
  oauth2.rs
```

Proofs:

- all implementations use only public core contracts;
- JSON media types and body-safe errors;
- CSV/NDJSON framing, limits, and parser-text redaction;
- controller overflow/termination semantics;
- governor capacity, pruning, key bound, cooldown cap, and concurrency behavior;
- OAuth provider isolation, HTTPS validation, bounded responses, expiry, refresh, and redaction.

## 15.6 Target façade test tree

```text
concord/tests/
  generated_surface.rs
  generated_surface/
    minimal.rs
    scopes.rs
    names.rs
    optional_values.rs
    explicit_endpoints.rs
    authentication.rs
    pagination.rs
    dangerous.rs

  public_extensions.rs
  public_extensions/
    body.rs
    response.rs
    credentials.rs
    pagination.rs
    rate_limit.rs
    hooks.rs

  deterministic_generated_runtime.rs
  public_compile.rs
  rustdoc.rs
  ui.rs
  ui/pass/
  ui/fail/
```

These tests must not list direct core, macros, or extras dependencies in their test crate imports. They consume `concord` as an external user would.

## 15.7 Product invariant to test owner

| Invariant | Primary proof |
|---|---|
| Predictable names | Macro surface unit + façade surface compile test |
| Clause order independence | Parser permutation unit tests |
| Duplicate clause spans | Parser unit + Trybuild |
| Precise reference/collision diagnostics | Resolver unit + Trybuild |
| Macro knows no extras | Cargo metadata policy + type-neutrality compile tests |
| Single dependency and renamed façade | Façade integration + renamed fixture |
| Optional methods are inherent | Façade signature/rustdoc tests |
| Invalid terminals absent | Trybuild |
| Cross-client endpoints rejected | Trybuild |
| Endpoint construction inert | Core lifecycle test |
| First poll begins effects | Core lifecycle test |
| Hook/body/auth/limiter ordering | Core deterministic test |
| First excess withheld | Core bounds tests |
| Native auth/private material inaccessible | Core security compile/runtime tests |
| One bounded auth recovery | Core auth lifecycle tests |
| Redaction across errors/events/debug | Core redaction matrix |
| Pagination sequential/all-or-error/cycle-safe | Core pagination tests |
| Extras controllers obey core contract | Extras tests |
| Governor absent fails closed | Feature fixture |
| Governor behavior bounded | Extras governor tests |
| Runtime/generated endpoints share executor | Façade deterministic test |
| Timeout tri-state distinct | Core + façade tests |
| Feature-disabled capabilities absent | Feature fixture |
| Examples are canonical | Example tests importing modules |
| Docs stay current | Doctests/link checks/generated tables |
| Compile/runtime regressions bounded | Perf/expansion metrics |

## 15.8 Fragmentation prevention rules

1. Use the lowest layer that proves the assertion.
2. A standalone package requires a written Cargo-graph reason.
3. Enabled-feature success belongs in the normal matrix.
4. Feature absence belongs in an isolated fixture.
5. Dependency rename/replacement/unification belongs in an isolated fixture.
6. Public extension implementations belong in façade integration tests or extras tests.
7. Source scanning may supplement but never replace compiler/API proof.
8. Tests import canonical examples.
9. Deterministic tests use no external network and no arbitrary wall-clock sleep.
10. Every fixture has an owner, reason, and deletion condition.
11. One fixture workspace owns one lockfile.
12. UI tests distinguish product invariants from temporary compatibility behavior.

## 15.9 rust-analyzer acceptance protocol

At release checkpoints:

1. Open only the main workspace.
2. Build a minimal generated client; endpoint completion appears.
3. Hover a direct call; a named request wrapper is displayed.
4. Type `.` after a realistic Riot endpoint; valid setters/terminals appear without imports.
5. A non-paginated request does not suggest `paginate`.
6. Go-to-definition reaches source span or stable generated rustdoc.
7. Collision/type errors anchor to DSL declarations.
8. Feature-disabled items are consistently inactive/absent.
9. Fixture diagnostics do not pollute the main project.
10. No trait-import quick fix is required for generated request methods.

---

# 16. Documentation redesign

## 16.1 Learning journey

Documentation begins with:

1. how a remote API is represented;
2. what Rust code is generated;
3. how that Rust is used;
4. what happens when a request executes;
5. how capabilities extend the model;
6. exact syntax, features, defaults, errors, and limitations.

Do not begin tutorials with internal lifecycle phase names or private bridge details.

## 16.2 Target tree

```text
docs/
  README.md
  SUMMARY.md

  guide/
    01-represent-an-api.md
    02-understand-generated-rust.md
    03-build-a-client-and-make-requests.md
    04-authentication.md
    05-bodies-responses-and-streams.md
    06-pagination-and-rate-limits.md
    07-explicit-and-runtime-endpoints.md

  concepts/
    contract-model.md
    managed-lifecycle.md
    finite-behavior.md
    security-and-redaction.md
    extension-model.md

  recipes/
    regional-apis.md
    testing-api-clients.md
    production-configuration.md
    riot.md

  reference/
    dsl.md
    generated-api.md
    features.md
    limits.md
    errors.md
    compatibility.md
    dangerous.md
    unsupported.md

  contributor/
    architecture.md
    macro-pipeline.md
    testing.md
    documentation.md
    release.md
```

## 16.3 Canonical owners

| Fact | Canonical owner |
|---|---|
| Product mental model | Guide 1 + contract concept |
| Exact DSL grammar | `reference/dsl.md` |
| Predictable generated names/types/methods | `reference/generated-api.md` |
| Exact signatures and trait bounds | Rustdoc |
| Complete runnable programs | Importable example modules |
| Managed lifecycle | `concepts/managed-lifecycle.md` |
| Security/redaction | `concepts/security-and-redaction.md` |
| Finite behavior | `concepts/finite-behavior.md` |
| Exact Cargo features | `reference/features.md`, generated/checked from façade manifest |
| Exact numeric defaults | `reference/limits.md`, generated/checked from one source |
| Error taxonomy | `reference/errors.md` + Rustdoc |
| Dangerous APIs | `reference/dangerous.md` |
| Unsupported behavior | `reference/unsupported.md` |
| Private bridge/macro pipeline | Contributor docs only |
| Test/fixture policy | `contributor/testing.md` |
| Commands | `just --list`/`xtask`, mechanically checked |

## 16.4 Delete, merge, rewrite, generate

Delete:

- manual complete export inventories duplicated from rustdoc;
- stale `scripts/check.py` and missing `scripts/` references;
- two-crate installation instructions;
- repeated exact defaults outside the limits reference;
- repeated full programs already owned by examples;
- user-facing private bridge/version details except compatibility notes.

Merge:

- current getting-started/basic introduction into Guide 1–3;
- basic/advanced body pages into one guide plus extension concept;
- basic/advanced pagination/rate pages into one guide plus references;
- broad practical guide pages into focused recipes.

Rewrite:

- authentication around sources, placement, provider objects, lifecycle, and named builders;
- explicit endpoints as an ordinary capability;
- runtime endpoints using final terminology;
- generated API reference around deterministic mapping;
- contributor architecture around façade/compiler/kernel/extras boundaries.

Generate or mechanically check:

- feature table from `crates/concord/Cargo.toml`;
- limit table from one core defaults source;
- links to example modules;
- command inventory;
- public generated method presence through rustdoc/compile tests.

---
# 17. Staged implementation plan

## Phase 0 — Baseline, source-of-truth installation, and contract freeze

### Goal

Establish a trustworthy starting point, install this document in the repository, and freeze the target declaration-to-usage contract before structural edits.

### Prerequisites

None. Do not modify product code before completing the baseline record.

### Tasks

1. Copy this file to the repository root as `CONCORD_REFACTOR_REFERENCE.md`.
2. Record:
   - current branch and `git status --short`;
   - existing uncommitted/user changes that must not be overwritten;
   - `cargo metadata --format-version 1 --all-features` package/dependency graph;
   - workspace/default members;
   - package features and defaults;
   - current generated contract version in core and macros;
   - current fixture manifests and lockfiles;
   - current example target list;
   - current docs tree and contributor commands.
3. Run the existing documented gates exactly as they exist. Record failures as baseline failures rather than fixing them opportunistically:

   ```bash
   cargo fmt --all -- --check
   cargo check --workspace --all-targets
   cargo check --workspace --all-targets --all-features
   cargo test --workspace
   cargo test --workspace --all-features
   cargo clippy --workspace --all-targets --all-features -- -D warnings
   cargo test --workspace --doc --all-features
   cargo rustdoc -p concord_core --all-features -- -D warnings
   cargo rustdoc -p concord_macros --all-features -- -D warnings
   ```

   Also run the current `just verify` if the host supports the current PowerShell shell. Record portability failure separately.
4. Generate a baseline inventory of public symbols and features. A small temporary script or `cargo rustdoc` JSON tool may be used, but do not introduce a permanent tool yet.
5. Capture representative current generated signatures for:
   - minimal endpoint;
   - optional/default parameter;
   - nested scopes;
   - authentication;
   - pagination;
   - explicit endpoint;
   - runtime/prepared endpoint;
   - raw response.
6. Add or confirm high-value safety tests before extraction begins:
   - endpoint construction is inert;
   - authentication material is private/redacted;
   - first excess bytes are withheld;
   - pagination is sequential/all-or-error;
   - no-governor generated rate plan fails closed;
   - completion follows result fixation;
   - generated contract mismatch is detected.
7. Add a `Target public contract` test module or design fixture containing the ideal minimal/Riot/auth/pagination/custom/runtime/explicit sketches as compile-target comments or ignored design snapshots. It should not yet be expected to pass; its role is to prevent design drift.
8. Fill the Phase 0 ledger row with exact commands and baseline failures.

### Do not do in this phase

- Do not split macro files.
- Do not move concrete implementations.
- Do not rename public types.
- Do not “clean up” tests merely because they are large.
- Do not change lifecycle ordering.

### Validation and exit criteria

- Baseline commands and failures are recorded.
- The worktree’s pre-existing changes are known.
- The reviewed target public sketches are present in the repository reference.
- Every non-negotiable runtime invariant has an identified existing or planned test owner.
- No product behavior changed.

### Main risks

- Mistaking an existing baseline failure for a refactor regression.
- Losing user changes.
- Beginning implementation before generated public signatures are fixed.

---

## Phase 1 — Add the façade and establish the final crate graph

### Goal

Make one `concord` dependency possible before moving implementations. Establish the hidden façade bridge and renamed-façade expansion path without changing core runtime semantics.

### Prerequisites

Phase 0 complete.

### Target dependency state at phase end

```text
concord -> concord_core + concord_macros + concord_extras (empty/skeleton initially)
concord_extras -> concord_core
concord_examples may begin migration to concord
concord_macros has no concord_extras edge
```

### Tasks

1. Add packages at the existing root layout initially to avoid combining semantic work with directory moves:

   ```text
   concord/
   concord_extras/
   ```

   The late workspace phase moves them under `crates/`.
2. Configure exact-version, `default-features = false` internal dependencies.
3. Give `concord_extras` an empty `lib.rs`, empty default feature set, `#![forbid(unsafe_code)]`, and `#![deny(missing_docs)]`.
4. Implement `concord/src/lib.rs` as a compatibility façade over the current core surface:
   - `pub use concord_macros::api;`
   - top-level common reexports;
   - `prelude`, `advanced`, and feature-gated `dangerous` modules forwarding current core types;
   - domain modules may initially be thin reexports and are finalized in Phase 5.
5. Add `concord::__private`:
   - doc-hidden;
   - reexports the current core generated bridge narrowly;
   - exposes the same generated contract assertion;
   - contains no extras-specific logic yet except a reserved module structure for later default composition.
6. Mirror current user-facing features on the façade and forward them to core for this transitional phase. Preserve current behavior.
7. Change macro crate-path resolution:
   - first resolve Cargo package `concord`;
   - support dependency renames;
   - temporarily fall back to `concord_core` only for existing internal tests/legacy use;
   - record the fallback as transitional and covered by a removal test in Phase 14.
8. Generated code must use one resolved façade token path consistently. Rename internal codegen variables from `core_path` to `facade_path` or `runtime_path` only where it improves correctness; avoid a broad macro rewrite.
9. The façade bridge must make generated code compile without a direct downstream core dependency.
10. Add façade integration tests that use only:

    ```rust
    use concord::{api, Bytes, NoContent};
    ```

    Prove a minimal client builds and a pending request is constructible.
11. Add/convert the renamed dependency fixture from `renamed_core` to `renamed_facade`:

    ```toml
    runtime = { package = "concord", path = "..." }
    ```

    Expansion must use `::runtime::__private` through the resolved alias.
12. Add a dependency policy test asserting:
    - façade edges are exact version;
    - macros have no extras edge;
    - extras has no façade/macro edge;
    - examples are scheduled to remove direct implementation dependencies.
13. Bump the generated private contract version if the emitted bridge path or signatures change.
14. Update only the minimal README quick-start dependency/import snippets necessary to keep the repository’s first entry point truthful. Full docs rewrite waits.

### Transitional compatibility

A direct `concord_core + concord_macros` path may remain briefly to keep old macro tests compiling. It must be clearly marked internal/legacy and removed in Phase 14. Do not document it as a supported alternative.

### Validation

```bash
cargo check -p concord --all-targets
cargo check -p concord --all-targets --all-features
cargo test -p concord
cargo test -p concord --all-features
cargo test -p concord_macros
cargo check -p concord_examples --all-targets   # as far as migrated
cargo metadata --format-version 1 --all-features
```

Run renamed-façade fixture offline/locked.

### Exit criteria

- A downstream test package depends only on `concord` and invokes `api!` successfully.
- Generated code references the renamed façade, not a transitive core crate.
- `concord_macros` has no normal dependency on extras and preferably none on core.
- Runtime behavior is unchanged.
- Feature forwarding matches the current behavior.

### Risks

- Proc-macro self/rename path handling.
- Accidentally broadening `__private` as a user extension API.
- Feature forwarding mismatch.
- Dev-dependency cycles if generated macro integration tests remain in the macro package; move them to façade rather than adding a cycle.

---

## Phase 2 — Extract pure trait implementations into `concord_extras`

### Goal

Move implementations that already fit public core contracts and require no core lifecycle redesign: JSON, CSV, NDJSON, and built-in pagination controllers. Remove macro knowledge of JSON type spelling.

### Prerequisites

Façade and skeleton extras package are operational.

### Tasks: codec extraction

1. Split `concord_core/src/codec/mod.rs` into contracts/primitives if useful, but do not reorganize unrelated code.
2. Move `Json<T>` and its media-type helper/tests to:

   ```text
   concord_extras/src/codec/json.rs
   ```

3. Implement `concord_core::codec::{EncodeBody, DecodeBody}` using public imports only.
4. Add `concord_extras/json` with Serde and Serde JSON dependencies.
5. Reexport `Json` through:

   ```text
   concord::codec::Json
   concord::Json
   concord::prelude::Json (when enabled)
   ```

6. Remove the public `concord_core/json` codec feature.

### Tasks: records extraction

1. Keep in core:
   - `RecordDecoder`;
   - record errors;
   - `Records<D>`;
   - bounded `RecordStream` and `RecordBatches`;
   - generic execution integration.
2. Move `csv.rs` and `ndjson.rs` concrete decoders and tests to extras.
3. Add extras features:
   - `csv` enables `concord_core/records`, `csv`, and Serde;
   - `ndjson` enables `concord_core/records`, Serde, and Serde JSON.
4. Preserve independent NDJSON behavior; `ndjson` must not require the buffered `json` extra.
5. Reexport through façade records module and top-level/prelude only where ordinary use justifies it.

### Tasks: pagination extraction

1. Leave `PaginationController`, `PaginationError`, `PageLimit`, pending/loop/identity/limits in core.
2. Move:
   - `OffsetPagination`;
   - `PagePagination`;
   - `CursorPagination<C>`;
   - `HasNextCursor<C>`
   to `concord_extras::pagination`.
3. Keep the built-ins featureless unless measured compile cost justifies a feature.
4. Update DSL examples to use façade paths or unqualified façade prelude names. The macro continues to parse arbitrary controller type paths and must not special-case these names.

### Tasks: transitional OAuth decoupling

The current core OAuth implementation uses the core `json` feature. Before deleting that feature:

1. Introduce a temporary core feature named `oauth2-client` containing only the internal OAuth provider’s Serde JSON dependency.
2. Update the current built-in OAuth syntax assertion/docs to refer to the temporary façade `oauth2-client` feature, not `json`.
3. Do not expose `Json<T>` from core under this feature.
4. Mark this feature ownership as transitional until Phase 4 moves OAuth to extras.

### Tasks: macro type neutrality

1. Delete `client_uses_json`, `endpoint_feature_summary` JSON logic, and `assert_json_feature` emission.
2. The generated hidden specification module imports `concord::prelude::*` through the façade path. It gains `Json` only when the façade feature provides it.
3. Add tests:
   - local custom type named `Json` is not special;
   - aliased extras JSON works;
   - fully qualified `concord::codec::Json<T>` works;
   - missing JSON feature fails at the type/import/declaration span without macro implementation names;
   - macros package metadata has no extras edge.
4. Move JSON-dependent generated integration tests from the macro crate to the façade where necessary.

### Tasks: dependency cleanup

- Remove `csv`, `serde`, and `serde_json` from core except the temporary OAuth-only Serde JSON dependency.
- Remove core public exports and feature docs for moved types.
- Update core tests to stop using moved implementations; lifecycle tests use local simple codecs/controllers.
- Add extras tests for the moved behavior.

### Validation

```bash
cargo check -p concord_core --no-default-features --all-targets
cargo check -p concord_extras --no-default-features --all-targets
cargo check -p concord_extras --all-features --all-targets
cargo test -p concord_extras --all-features
cargo test -p concord --all-features
cargo test -p concord_macros
cargo tree -p concord_macros
cargo tree -p concord_core
```

Feature combinations:

```bash
cargo check -p concord --no-default-features
cargo check -p concord --no-default-features --features json
cargo check -p concord --no-default-features --features records
cargo check -p concord --no-default-features --features csv
cargo check -p concord --no-default-features --features ndjson
cargo check -p concord --no-default-features --features csv,ndjson
```

### Exit criteria

- JSON/CSV/NDJSON and built-in pagination controllers are implemented only in extras.
- Core generic record/pagination behavior is unchanged.
- Macro source and dependency graph contain no extras type knowledge.
- `concord` preserves the intended user import paths.
- Core normal dependencies no longer include CSV or ordinary JSON codec dependencies.
- All moved implementation tests pass in extras.

### Risks

- Accidental use of core-private record/response APIs by moved decoders. Fix contracts rather than creating a friend bridge.
- Feature leakage causing NDJSON to require buffered JSON.
- Existing macro tests directly importing core moved types; relocate the proof rather than retaining duplicate core implementations.

---

## Phase 3 — Extract the governor and make default composition façade-owned

### Goal

Move the concrete governor and default Retry-After observer to extras while preserving bounded key preparation, cooldown caps, explicit limiter precedence, and fail-closed behavior.

### Prerequisites

Pure extras extraction complete; façade hidden bridge exists.

### Core contract changes

1. Add a documented `RateLimiter` metadata method with a safe default, such as:

   ```rust
   fn preparation_key_limit(&self) -> usize;
   ```

   Prefer a default implementation returning the current core default. Preserve the ability to configure zero for a global-only key if current semantics require it.
2. Remove `Any` downcast/private metadata used solely to detect `GovernorRateLimiter`.
3. Add the runtime rate-limit cooldown cap to `RateLimitResponse` or an equivalent public semantic observation context. Ensure it is sanitized and non-sensitive.
4. Remove the governor-specific task-local cap from core.
5. Make core `RuntimeConfigBuilder::new()` install the fail-closed fallback, not a concrete governor.
6. Add a narrow mechanism for generated default limiter installation that preserves whether the user explicitly supplied a limiter. Possible shape:

   ```rust
   #[doc(hidden)]
   pub fn install_generated_default_limiter(
       self,
       limiter: Arc<dyn RateLimiter>,
   ) -> Self;
   ```

   This must not mark the limiter as explicit.
7. Keep ordinary `.rate_limiter(...)` explicit and authoritative.
8. Core preparation of generated non-empty plans:
   - succeeds with explicit limiter;
   - succeeds with façade-installed generated default;
   - fails with `LimiterUnavailable` when neither exists;
   - always allows empty plans through fallback.

### Extras work

1. Move `concord_core/src/rate_limit/governor.rs` to extras.
2. Move `DefaultRateLimitObserver` and Retry-After parser/policy to extras.
3. Implement only public core `RateLimiter`/`RateLimitObserver` contracts.
4. Override preparation key limit.
5. Clamp observer-proposed cooldowns using the cap passed in the observation context.
6. Preserve all existing finite bounds:
   - quota capacity;
   - cooldown capacity;
   - idle TTL;
   - key bytes;
   - checked arithmetic;
   - bounded state pruning;
   - deterministic acquisition semantics.
7. Move governor-specific unit/runtime tests to extras.

### Façade composition

1. `concord/rate-limit-governor` enables the extras implementation.
2. `concord::__private::default_runtime_config()` installs an extras governor as the generated default when enabled.
3. `prepare_generated_rate_limits_with_observer<O>` in the façade:
   - leaves explicit user limiter untouched;
   - constructs/configures a governor with `O::default()` only when using the generated default path;
   - fails closed without the governor feature when a declaration has a non-empty rate plan and no explicit limiter.
4. The macro continues to call only façade bridge functions and never names the governor.

### Test migration

- Core execution tests that only need a ready limiter use local deterministic limiter implementations.
- Governor algorithm/state tests move to extras.
- Façade tests prove default injection.
- Isolated feature fixture proves no-governor fail-closed behavior.
- Test explicit custom limiter precedence with and without governor feature.
- Test DSL observer does not override explicit limiter.
- Test cooldown cap propagation and clamping.

### Validation

```bash
cargo test -p concord_core --no-default-features
cargo test -p concord_extras --features rate-limit-governor
cargo test -p concord --features rate-limit-governor
cargo test -p concord --no-default-features
cargo check -p concord_core --all-targets --all-features
cargo check -p concord_extras --all-targets --all-features
```

Run isolated no-governor fixture and all redaction/lifecycle tests.

### Exit criteria

- Core contains no `GovernorRateLimiter` implementation or concrete-type downcast.
- Extras governor uses only public core contracts.
- Façade defaults preserve current ordinary user behavior.
- Core alone fails closed for non-empty generated rate plans unless given a limiter.
- Explicit limiter precedence and observer semantics are proven.
- Cooldown cap remains centrally configured and actually applied by the default implementation.

### Risks

- Accidentally marking the generated default as user-explicit.
- Changing key limit semantics during trait redesign.
- Losing cooldown cap enforcement.
- Core tests depending on governor timing behavior; replace with deterministic test doubles rather than weakening assertions.

---

## Phase 4 — Extract OAuth2 and make provider implementations ordinary Rust

### Goal

Remove the final extras-specific authentication implementation and JSON dependency from core, eliminate built-in OAuth2 macro semantics, and use the generic credential provider model.

### Prerequisites

Façade/extras structure stable; named client builder work may be partially prepared but is completed in Phase 7.

### Extras provider

1. Move OAuth2 client credentials implementation and tests to:

   ```text
   concord_extras/src/auth/oauth2_client.rs
   ```

2. Make it a public `CredentialProvider<Material = AccessToken>`.
3. Add a named builder that validates:
   - absolute HTTPS URL;
   - host present;
   - no userinfo/password;
   - no fragment;
   - required client ID/secret;
   - optional scope with bounded owned storage if a core bound exists.
4. Preserve form encoding before Basic base64, provider request content type, response status checks, token type validation, expiry overflow checks, bounded provider body acquisition, safe error reduction, and refresh-as-reacquisition.
5. Use only public core auth/provider HTTP contracts.
6. Add façade reexports under `concord::auth` and optionally top-level/prelude only if ordinary use benefits.

### DSL and generated API

1. Use the ordinary custom provider source form. If current syntax is retained:

   ```rust
   credential oauth = custom OAuth2ClientCredentials;
   ```

2. Generated client builder accepts a named provider instance:

   ```rust
   .oauth_provider(provider)
   ```

3. Authentication placement uses the provider’s associated material type. Incompatible placement fails through generated trait/type checks at the declaration.
4. Generated credentials API supports safe replacement for custom providers when possible. Prefer a typed method such as:

   ```rust
   api.credentials().oauth().replace_provider(new_provider).await?;
   ```

5. Remove built-in `oauth2_client` syntax from the final grammar. During this phase a temporary parser compatibility branch may emit old code or a deprecation warning, but it has a Phase 14 removal item.
6. Delete macro `client_uses_oauth2`/feature assertion logic after migration.
7. Delete core private OAuth factory functions and missing-JSON placeholder provider.

### Core dependency cleanup

- Remove core `auth/oauth2.rs`.
- Remove temporary core `oauth2-client` feature.
- Remove Serde JSON and any now-unused Serde dependency from core.
- Confirm core has no concrete provider protocol implementation beyond primitive static/endpoint slot mechanics.

### Tests

- Port every existing deterministic OAuth test to extras.
- Add façade generated-auth tests using `OAuth2ClientCredentials` as a normal provider.
- Add a local third-party provider with the same material type to prove macro neutrality.
- Add feature-absence fixture proving `concord::auth::OAuth2ClientCredentials` is absent without `oauth2-client`; no macro-specific JSON/OAuth diagnostic is expected.
- Test provider replacement and redaction.
- Test that OAuth extras can be aliased or fully qualified in the DSL.

### Validation

```bash
cargo tree -p concord_core
cargo tree -p concord_macros
cargo test -p concord_extras --features oauth2-client
cargo test -p concord --features oauth2-client
cargo test -p concord --all-features
cargo check -p concord_core --no-default-features --all-targets
```

### Exit criteria

- Core has no Serde/Serde JSON/CSV normal dependency.
- Macro contains no OAuth2 or JSON implementation branch.
- OAuth2 works as an ordinary extras provider through the façade.
- All provider HTTP isolation, boundedness, and redaction tests remain.
- Named provider construction replaces positional OAuth secrets.

### Risks

- Losing source-specific credential rotation semantics. Restore through generic provider replacement rather than reintroducing macro special cases.
- Exposing provider response/parser details in errors.
- Accidentally allowing non-HTTPS token URLs.
- A compatibility branch lingering beyond final cleanup.

---
## Phase 5 — Finalize public namespaces, feature ownership, and one-crate consumption

### Goal

Make the façade the complete documented product surface, make core/extras defaults empty, and migrate all ordinary workspace consumers away from direct implementation crates before generated API redesign.

### Prerequisites

Core/extras implementation boundaries are established.

### Tasks: feature topology

1. Set `concord_core` and `concord_extras` default features to empty.
2. Make `concord` own the selected defaults:

   ```text
   default-tls
   http2
   rate-limit-governor
   ```

3. Implement the final forwarding table in Section 11.
4. Ensure `json`, `csv`, `ndjson`, `oauth2-client`, and governor features point to extras; transport/multipart/dangerous/records features point to core.
5. Add a feature ownership test that parses Cargo metadata and rejects:
   - extras features redefined in core;
   - user-facing runtime features defined only on implementation crates;
   - non-exact internal sibling versions;
   - default feature drift.

### Tasks: public modules

1. Create final domain-oriented façade modules.
2. Reexport traits from the domain where users implement them:
   - codecs from `concord::codec`;
   - body specs from `concord::body`;
   - response specs from `concord::response`;
   - providers/material from `concord::auth`;
   - controllers/limits from `concord::pagination`;
   - limiter contracts/implementations from `concord::rate_limit`;
   - record contracts/decoders from `concord::records`.
3. Keep a curated top level for high-frequency types (`api`, `Bytes`, `Json` when enabled, `Text`, `NoContent`, `PageLimit`, common pagination controllers).
4. Build an optional `concord::prelude` that contains common imports only. It must not be required for generated methods.
5. Build `concord::advanced` as a compatibility/convenience aggregation, not the canonical sole owner.
6. Keep `concord::dangerous` feature-gated and explicit.
7. Do not publicly expose broad `core` or `extras` modules.

### Tasks: workspace consumer migration

1. Change `concord_examples` dependencies to:

   ```toml
   concord = { path = "../concord", default-features = false, ... }
   ```

   Remove direct core/macros/extras dependencies.
2. Change all example imports to façade paths.
3. Change `perf` ordinary product imports to façade paths. It may retain an explicitly unstable test/development bridge only when benchmark probes require it; document that exception.
4. Move public generated integration tests from macros/core into façade as appropriate.
5. Update root README quick start to one dependency and façade imports.
6. Add a policy test asserting no canonical example source contains `concord_core`, `concord_macros`, or `concord_extras` imports.

### Tasks: package documentation and publishing posture

1. Add package descriptions and `package.metadata.docs.rs` where useful.
2. Decide registry publication mechanics:
   - `concord` is the documented package;
   - implementation crates may need publication with exact versions;
   - mark implementation packages in docs as internal entry points even if public on the registry.
3. Do not finalize release metadata until Phase 14, but make the dependency graph publishable in principle.

### Validation

```bash
cargo check --workspace --all-targets
cargo check --workspace --all-targets --all-features
cargo test -p concord --all-features
cargo test -p concord_examples --all-features
cargo check -p concord_examples --all-targets --all-features
cargo tree -p concord --all-features
cargo metadata --format-version 1 --all-features
```

Run no-default and selected feature combinations on the façade.

### Exit criteria

- Ordinary examples and public tests import only `concord`.
- User-facing feature documentation can be generated from one manifest.
- Core/extras have empty defaults.
- Domain module paths are stable enough for the generated surface work.
- Macro remains extras-agnostic.

### Risks

- Reexport ambiguity or duplicate names.
- Hidden feature activation through internal defaults.
- Accidentally documenting implementation crates as alternative user entry points.
- Benchmark code depending on private paths without a narrowly justified development boundary.

---

## Phase 6 — Generate named endpoint request wrappers and remove macro-specific call-site conventions

### Goal

Replace the generic-request-plus-extension-trait public surface with predictable named request wrappers, inherent capabilities, one scope-oriented module tree, and a generic sealed endpoint trait.

### Prerequisites

Façade paths and domain modules are stable. Do this before major macro file restructuring.

### Design requirements

For every endpoint `get_user`, generate at least:

```text
GetUser
GetUserRequest
```

Direct call:

```rust
fn get_user(&self, id: UserId) -> users_api::GetUserRequest;
```

Explicit construction:

```rust
users_api::GetUser::new(id)
```

Binding:

```rust
client.request(endpoint) -> <Endpoint as users_api::Endpoint>::Request
```

### Tasks: surface model

1. Add enough normalized/surface data to represent:
   - endpoint type name;
   - request wrapper name;
   - final module path;
   - required arguments;
   - optional/default setters;
   - legal terminals;
   - endpoint credential storage terminals;
   - pagination capability;
   - raw response capability;
   - clone eligibility;
   - docs/description identity.
2. Resolve and validate every generated public name before token emission.
3. Add collision tests covering client methods, scope methods, endpoint types, request types, setters, reset/clear methods, credential terminals, and reserved methods.

### Tasks: request wrapper implementation

1. Keep the current core `Request<E>` as the internal generic execution carrier.
2. Generate a public wrapper containing it:

   ```rust
   pub struct GetUserRequest {
       inner: concord::__private::Request<GetUser>,
   }
   ```

3. Implement:
   - `IntoFuture` delegating to the inner request;
   - universal inherent methods such as timeout controls and `.response()`;
   - endpoint-specific optional/default setters by mapping the stored endpoint through the private bridge;
   - `.paginate(...)` only for paginated endpoints;
   - `.store_as_*()` only for endpoint credential sources;
   - `.raw_response()` only when response mode and dangerous feature permit it;
   - safe `Debug` without endpoint values/secrets unless already safely represented.
4. Avoid `Deref` to the generic request; it would expose irrelevant methods and weaken capability discovery.
5. The core/private bridge may add narrow generic delegation helpers, but generated wrappers should remain simple explicit Rust.
6. Decide whether `.response()` returns a universal core `ResponseRequest<E>` or a generated named response wrapper. Prefer the smallest ordinary surface that still gives clear rustdoc and capability control. Record the choice in the ledger.

### Tasks: endpoint trait and binding

1. Generate one documented sealed trait under the client module:

   ```rust
   pub trait Endpoint: private::Sealed + Sized {
       type Request;
       fn bind(self, client: &UsersApi) -> Self::Request;
       fn description(&self) -> &EndpointDescription;
   }
   ```

   Exact method visibility may use a hidden implementation method plus public safe methods, but generic callers must be able to express `E::Request`.
2. Implement it for every generated endpoint type.
3. Keep cross-client endpoint rejection at compile time: an endpoint from client A does not implement client B’s endpoint trait.
4. Runtime endpoints enter through a separate core-supported implementation only if their client binding semantics are coherent; do not weaken the generated client-specific trait.

### Tasks: module tree

1. Replace separate public `facades` and `endpoints` trees with one scope tree.
2. Each scope module owns:
   - facade type;
   - child scope modules;
   - endpoint values;
   - request wrappers.
3. Preserve temporary deprecated reexports only if needed to keep phases buildable.

### Tasks: remove call-site conventions

1. Stop generating per-endpoint request extension traits.
2. Stop collecting/exporting generated prelude aliases.
3. Remove compile tests whose desired failure is “trait not in scope.” Replace them with tests proving no import is needed.
4. Remove `cfg(doc)` fake methods that existed only to present trait methods.
5. Update generated docs to describe inherent methods and predictable types.

### Tasks: reuse and clone

1. Generate conditional `Clone` for endpoint values when all stored fields are cloneable.
2. Do not couple clone support to pagination.
3. If automatic conditional derives are not expressible for all generated fields, emit explicit bounded implementations.
4. Named request wrappers need not be cloneable unless the underlying pending execution semantics safely permit it. Endpoint values are the intended reusable description.

### Tests

Add compile-pass/fail and rustdoc tests for:

- direct call returns named wrapper;
- optional/default setter completion without imports;
- reset/clear behavior;
- non-paginated endpoint lacks `paginate`;
- wrong client rejects endpoint;
- endpoint-backed storage exists only on source endpoint;
- dangerous method feature/mode gating;
- explicit endpoint generic binding;
- old generated prelude is not needed;
- module paths are predictable;
- method/type collision diagnostics.

Add deterministic runtime tests proving wrapper delegation preserves all request-local fields and lifecycle behavior.

### Compile-size measurement

Capture before/after:

- emitted token count;
- generated item count;
- clean check time for representative generated APIs;
- rustdoc time.

Do not optimize prematurely; record regressions for later work.

### Validation

```bash
cargo test -p concord --test generated_surface --all-features
cargo test -p concord --test ui --all-features
cargo test -p concord --test deterministic_generated_runtime --all-features
cargo rustdoc -p concord --all-features -- -D warnings
cargo check -p concord_examples --all-targets --all-features
```

Run the manual rust-analyzer protocol for minimal and Riot-scale declarations.

### Exit criteria

- All generated endpoint request capabilities are inherent.
- No generated prelude import is required.
- Direct methods return named request types.
- Explicit endpoint values are public at predictable paths.
- One scope-oriented module tree exists.
- Cross-client and invalid-terminal compile-time protections remain.
- Runtime lifecycle behavior is unchanged.

### Risks

- Token growth and compile time.
- `IntoFuture` associated type complexity.
- Public users explicitly annotating `Request<_>`; accepted breaking change, with temporary conversion aliases only if needed.
- Accidentally exposing inner generic request methods through conversion/deref.

---

## Phase 7 — Redesign client construction and separate runtime policy from contract declaration

### Goal

Make client creation readable, evolvable, and safe for secrets/providers; simplify configuration composition; prepare removal of runtime policy clauses from the DSL.

### Prerequisites

Named request surface is stable enough that generated client code can change independently.

### Tasks: generated builder

1. Generate named storage and setters for:
   - required client values;
   - defaulted client values;
   - secrets;
   - custom credential providers;
   - HTTP configuration;
   - runtime configuration.
2. Generate `Client::new()` only when no required client values, secrets, or providers exist. It may delegate to `builder().build()`.
3. Configured clients expose `Client::builder()` as the canonical constructor.
4. Remove the all-positional constructor from docs immediately and from code in Phase 14. A temporary deprecated compatibility constructor may exist only if needed.
5. Missing input errors:
   - preserve a stable safe error category;
   - may expose the static source field name through `missing_field()` or safe display text;
   - never contain the missing value or secret contents;
   - identify provider versus secret/value where useful.
6. Keep runtime validation fallible at `build()`; do not introduce a large typestate builder unless explicit compile-time completeness is later selected as a separate product change.

### Tasks: configuration composition

1. Replace duplicate-closure flags with deterministic consuming composition.
2. Preferred API:

   ```rust
   Api::builder()
       .configure_http(|http| http.proxy(proxy))
       .configure_http(|http| http.connect_timeout(...))
       .configure_runtime(|runtime| runtime.response_body_limit(...))
       .build()?;
   ```

   Each closure transforms the current builder in call order.
3. Alternatively expose direct builder replacement methods in addition to closures. Do not make a second call an error simply because it is repeated.
4. Preserve explicit limiter precedence over generated defaults.
5. Preserve immutable runtime snapshot construction.

### Tasks: runtime policy relocation

1. Add or document explicit HTTP builder APIs for dangerous custom retry:

   ```rust
   .configure_http(|http| http.dangerous_custom_retry(factory))
   ```

2. Mark DSL `retry custom PATH` as legacy; Phase 8 removes it from the grammar.
3. Concrete limiter, hooks, debug, proxy, TLS, and local limits stay on builders.
4. If the DSL currently names a concrete rate observer type, decide whether it is contract-relevant generated default policy or pure runtime configuration:
   - recommended final state: observer instance/type is runtime builder configuration;
   - lexical remote quota declarations remain DSL.
   Record any retained DSL observer rationale.
5. Client/endpoint timeout defaults may remain in the declaration because they define operation defaults, but the core runtime snapshot must own a genuine client default for runtime endpoints to inherit.

### Tasks: secret/provider ergonomics

- Setter names match DSL names predictably.
- Secret setters accept `SecretString` or narrowly justified conversion APIs; avoid implicit `String` conversions that encourage accidental logging.
- Provider setters accept the concrete declared provider type or a safe boxed/Arc trait object according to the declaration model.
- Credential handles support typed replacement without formatting provider/secret state.

### Tests

- zero-config client has `new()`;
- configured client does not expose ambiguous positional `new()` in final surface;
- every required field produces focused safe missing-field error;
- secrets are never formatted;
- repeated configuration calls compose in order;
- explicit limiter/provider replacement works;
- generated defaults do not override explicit choices;
- runtime snapshot remains immutable across client clones.

### Validation

```bash
cargo test -p concord --all-features
cargo test -p concord_core --all-features
cargo test -p concord_extras --all-features
cargo check -p concord_examples --all-targets --all-features
```

### Exit criteria

- No canonical usage contains positional client secrets/providers.
- Zero-config `new()` remains concise.
- Repeated runtime/HTTP configuration is deterministic and compositional.
- Dangerous custom retry has an explicit runtime API.
- Client build errors remain safe and more actionable.

### Risks

- Generated builder code size.
- Breaking existing constructor call sites.
- Accidentally moving endpoint contract defaults into mutable runtime behavior.
- Field-name diagnostics becoming inconsistent; generate from one surface model.

---

## Phase 8 — Implement DSL v2 and public naming rules

### Goal

Make the declaration resemble the remote/Rust contract rather than parser implementation, and make generated usage mechanically predictable.

### Prerequisites

Generated names, wrapper capabilities, and client builder target are known.

### Grammar changes

1. Endpoint and scope identifiers are snake_case Rust identifiers.
2. Generated endpoint/scope type names are deterministic PascalCase conversions.
3. Named configuration clauses are order-independent.
4. Duplicate clauses are focused errors with both spans.
5. Body role is `#[body]` on a parameter; body may appear at its declared position.
6. Optional parameters are syntactic `Option<T>` or qualified `std::option::Option<T>`.
7. Default parameters use `name: Type = expression`.
8. Add optional stable ID clause:

   ```rust
   id "riot.match_v5.match_by_id";
   ```

   Validate bounded/static format and uniqueness within the client. If omitted, derive deterministic source-path identity.
9. Remove final built-in `oauth2_client` credential source syntax.
10. Remove `retry custom PATH` contract syntax.
11. Keep arbitrary Rust types opaque.
12. Keep exactly one client per invocation and client declaration first unless a separately justified grammar change is accepted.

### AST changes

- Preserve source spans for every clause key and value.
- Represent clauses as named optional fields independent of parse order.
- Preserve parameter declaration order and explicit role.
- Preserve body parameter name/type/position.
- Represent endpoint ID explicitly.
- Do not store generated Rust names in the raw syntax AST.

### Parser implementation

1. Parse endpoint/scope/client bodies as repeated clause-or-child entries.
2. Dispatch by keyword; track duplicates in a local clause accumulator.
3. For list clauses, preserve entry order.
4. Reject unknown clauses with a list of valid contract concepts, not parser stage terminology.
5. Add permutation/property-style unit tests for legal clause orders.
6. Add malformed tests for duplicate clauses, bad body attributes, multiple bodies, alias-based optional ambiguity where detectable, and invalid stable IDs.

### Resolver/surface changes

1. Resolve source snake_case names to method/module and deterministic type names.
2. Validate generated collisions across the complete surface.
3. Calculate direct required argument order from declaration order.
4. Calculate optional/default setters and reset/clear names.
5. Assign stable endpoint and rate-limit identities.
6. Keep lexical scope inheritance and constrained shaping expressions.
7. Generate trait assertions for arbitrary body/response/controller/provider types at their declaration spans.

### Compatibility strategy

Because the project is `0.1.0` and unpublished, prefer a clean conversion of workspace declarations. If a legacy parser branch is retained temporarily:

- it must be isolated;
- it must emit a warning/deprecation where stable Rust permits;
- tests must state the removal phase;
- it must not constrain the normalized v2 model.

Do not preserve fixed clause order as a compatibility requirement.

### Diagnostics target

Examples:

```text
error: duplicate `timeout` clause for endpoint `get_user`
  --> ... first declared here
  --> ... duplicated here
```

```text
error: endpoint `list_users` declares two request body parameters
```

```text
error: generated request method `response` conflicts with endpoint parameter `response`
help: rename the parameter or give it an explicit generated setter name (if such override exists)
```

No diagnostic should say “normative order,” refer to generated seal modules, or require users to understand expansion internals.

### Tests

- all clause permutations;
- duplicates with two spans;
- snake_case/acronym mapping;
- raw identifiers;
- stable ID uniqueness and rename stability;
- explicit body anywhere in signature;
- multiple/invalid body role;
- syntactic Option and qualified Option;
- alias limitation documented/tested;
- direct argument order;
- generated collision matrix;
- removed OAuth/retry syntax behavior;
- ideal minimal/Riot/auth/pagination sketches compile.

### Validation

```bash
cargo test -p concord_macros
cargo test -p concord --test ui --all-features
cargo test -p concord --test generated_surface --all-features
cargo check -p concord_examples --all-targets --all-features
```

Update Trybuild `.stderr` intentionally and inspect every diff; do not blanket-accept diagnostics.

### Exit criteria

- The DSL states the call-site method names directly.
- Clause order is not semantic.
- Body role is explicit.
- Macro has no extras-specific syntax.
- Stable identity policy is implemented.
- Generated names and capabilities are predictable from the declaration.

### Risks

- Large UI fixture churn hiding real diagnostic regressions.
- Identity changes affecting telemetry/rate state; explicit IDs mitigate.
- Ambiguous compatibility parsing.
- Parameter attributes interacting with `syn` parsing/MSRV.

---

## Phase 9 — Add endpoint descriptions, safe planning, and correct runtime endpoint semantics

### Goal

Make endpoint values fully useful for storage/composition/testing and align runtime-defined endpoints with the same public model and lifecycle.

### Prerequisites

DSL identity and generated endpoint/request surface are stable.

### Tasks: endpoint identity and description

1. Introduce `EndpointId` in core:
   - supports allocation-free static generated IDs;
   - supports bounded owned runtime IDs;
   - validates length/format centrally;
   - safe `Debug`/Display behavior;
   - cloneable and hashable as required without exposing secrets.
2. Introduce a safe `EndpointDescription` containing only non-secret contract facts, such as:
   - stable ID;
   - HTTP method;
   - scope/name path;
   - route template or safe shape;
   - body specification category/identity where static;
   - response acquisition category;
   - capability flags (pagination, endpoint credential source, raw eligibility, timeout policy);
   - declared auth/rate presence without material or dynamic keys.
3. Generate one static description from the normalized surface model.
4. Endpoint value `.description()` returns it.
5. The generated client/module can enumerate descriptions only if a bounded/static API is useful; do not add reflection complexity without a use case.

### Tasks: sanitized planning

1. Introduce `SafeRequestPlan` with:
   - endpoint ID;
   - method;
   - safe logical URL;
   - sanitized public headers;
   - timeout selection;
   - page index if relevant;
   - declared auth/rate summary without protected values/keys;
   - response mode/category;
   - static body category, with dynamic metadata marked unavailable.
2. Reuse the real public head construction and validation code. Do not duplicate URL encoding, host/path composition, header validation, or redaction logic.
3. Expose a predictable API such as:

   ```rust
   let plan = client.plan(&endpoint)?;
   ```

   or an equivalent borrowing API that does not consume body input unnecessarily. Record the final signature.
4. Planning must remain inert under the restrictions in Section 7.6.
5. Add a test hook/provider/limiter/body spec with counters and prove all counters remain zero during planning.
6. Add redaction property tests for planned URLs/headers.

### Tasks: runtime endpoint rename and builder

1. Rename `PreparedEndpoint`/builder to `RuntimeEndpoint`/`RuntimeEndpointBuilder`.
2. Add temporary deprecated aliases only for migration; remove in Phase 14.
3. Replace current timeout representation with:

   ```rust
   enum TimeoutSelection {
       InheritClient,
       Explicit(Duration),
       Disabled,
   }
   ```

4. Add a real client-default operation timeout to the immutable runtime snapshot so inheritance has meaning for runtime endpoints.
5. Validate runtime endpoint ID, method, path/query/header values, response/body specifications, and timeout state at build/preparation as appropriate.
6. Keep runtime endpoints inert and execute through the same core request/executor.
7. Keep native transport private.
8. Keep runtime pagination unsupported.
9. Ensure direct generated endpoints and runtime endpoints produce equivalent lifecycle events and completion for equivalent operations.

### Tasks: explicit endpoint reuse

- Generate conditional `Clone` for safe endpoint values.
- Add tests storing endpoints in structs/collections where bounds allow.
- Add generic functions over the generated sealed endpoint trait.
- Add endpoint equality/hash only if semantics are well-defined and values are safe; do not add reflexively.

### Tests

- description contains only expected safe facts;
- description ID remains stable across Rust method rename when explicit ID is set;
- plan and execution public-head results match;
- plan invokes no hooks/body/provider/limiter/transport;
- protected query/header auth never appears;
- `InheritClient`, `Explicit`, and `Disabled` are observably distinct;
- runtime and generated endpoints share lifecycle ordering;
- runtime endpoint is inert;
- runtime endpoint cannot paginate;
- alias deprecation and final target names compile as intended.

### Validation

```bash
cargo test -p concord_core --all-features
cargo test -p concord --all-features
cargo test -p concord_examples --all-features
cargo test -p concord --test ui --all-features
```

Run redaction/security suites and performance smoke for planning/construction.

### Exit criteria

- Explicit endpoint values are useful, describable, and reusable when type bounds permit.
- Safe planning has one semantic source of truth and no effects.
- Runtime endpoint terminology and timeout semantics are correct.
- All endpoint forms converge on one lifecycle.

### Risks

- Planning accidentally invoking extension callbacks.
- Descriptions exposing route values or credential names considered sensitive.
- Owned ID allocations/cardinality becoming unbounded; enforce limits.
- Timeout migration changing generated endpoint defaults.

---
## Phase 10 — Restructure macro internals around the stable language and generated artifacts

### Goal

Improve language evolution, diagnostics, generated-code auditability, test locality, and contributor comprehension without changing the now-stable public contract.

### Prerequisites

DSL v2, named request wrappers, descriptions, and bridge signatures are stable. Public behavior has strong tests.

### Strategy

This phase is a controlled internal migration. Move one semantic area or emitter at a time and keep tests green. Do not combine it with additional public redesign.

### Tasks: syntax layer

1. Create `syntax/` modules by semantic grammar:
   - API/client/scope/endpoint structure;
   - values/parameters;
   - request shaping;
   - credentials/authentication;
   - pagination;
   - rate limits;
   - diagnostics helpers.
2. Move parser code and its unit tests together.
3. Retain a single top-level parse entry and one syntax AST.
4. Avoid separate parser-local models that must later be reconciled.
5. Preserve clause spans and duplicate diagnostics exactly.

### Tasks: semantic layer

1. Create shared:
   - symbol table/reference resolution;
   - lexical inheritance;
   - name generation/collision validation;
   - normalized model.
2. Move auth, pagination, and rate-limit semantic validation into domain modules that operate on the shared model.
3. Create/finish `semantic::surface` as the sole owner of generated names, module paths, docs metadata, and capability sets.
4. Add normalized-model tests that do not depend on token string formatting.
5. Ensure no semantic domain performs token emission.

### Tasks: emit layer

1. Split by generated artifact:
   - clients/builders;
   - scopes;
   - endpoint values;
   - request wrappers;
   - credentials/handles;
   - descriptions;
   - docs;
   - private bridge impls;
   - feature/trait assertions.
2. Each emitter receives resolved surface/semantic data and does not perform new user validation.
3. Centralize common token helpers only where they reduce semantic duplication.
4. Keep generated code auditable. Avoid nested helper-macro expansion that hides public signatures.
5. Isolate generated contract version and bridge calls.

### Tasks: tests

- parser tests live with syntax modules;
- semantic normalization tests live with domain resolvers;
- emitted artifact tests assert public signatures/modules/capabilities;
- façade Trybuild remains owner of user diagnostics;
- snapshot tests, if introduced, normalize spans and avoid brittle whole-file token whitespace;
- compare emitted token/item counts to Phase 6 baseline.

### Tasks: contributor documentation

Document:

```text
source tokens → syntax AST → normalized contract → generated surface → token artifacts → façade bridge → core lifecycle
```

Explain where each validation belongs and how to add a new clause/capability without creating a second source of truth.

### Validation

Run all macro unit tests after each moved domain, then:

```bash
cargo test -p concord_macros
cargo test -p concord --test ui --all-features
cargo test -p concord --test generated_surface --all-features
cargo check --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

### Exit criteria

- Public generated code is unchanged except intentional formatting/internal spans.
- Syntax, semantic, surface, and emit ownership is explicit.
- User validation does not occur in emit modules.
- Every generated public artifact has a local emitter test.
- Macro compilation metrics do not regress without a recorded explanation.

### Risks

- Mechanical movement subtly changing spans or diagnostics.
- Duplicating tree traversal/state across domains.
- Pure vertical slicing causing separate symbol/inheritance logic.
- Snapshot churn obscuring real changes.

---

## Phase 11 — Rebuild examples as importable acceptance modules

### Goal

Turn examples into the executable product narrative described in Section 14 and make tests exercise the real declarations.

### Prerequisites

Public API, DSL, features, and endpoint terminology are stable.

### Tasks: package structure

1. Move every canonical API declaration from `examples/*.rs` into `src/<capability>/...` modules.
2. Export modules from `concord_examples::lib` with feature gates matching the demonstrated capability.
3. Replace `src/main.rs` with a thin minimal binary or move it to `src/bin/minimal.rs`.
4. Keep only thin binaries under `src/bin` or `examples` if Cargo example target behavior is specifically desired.
5. Remove direct implementation crate imports; use only façade paths.

### Tasks: shared extensions

1. Move existing `src/lib.rs` extension types into the relevant example modules:
   - codec examples with request/response specs;
   - credential provider/material with auth examples;
   - page/controller with pagination examples;
   - observer with rate-limit examples.
2. Do not create a generic dumping-ground module.
3. Ensure every extension uses documented façade domain traits.

### Tasks: Riot split

1. Separate domain data/routing types from API declarations.
2. Separate Riot API and Data Dragon API declarations.
3. Separate usage functions from declarations.
4. Keep live environment access in a manual binary behind a clearly named feature.
5. Add deterministic tests for representative:
   - platform and regional host composition;
   - API-key placement/redaction;
   - lexical rate plans;
   - optional/default query setters;
   - pagination binding;
   - explicit endpoint description/plan.
6. Avoid testing every Riot endpoint at runtime; compile the complete declaration and execute representative semantic classes.

### Tasks: records and network behavior

1. Replace external `.example.test` execution with scripted local response chunks.
2. Test one-byte and arbitrary chunk boundaries for CSV/NDJSON through actual example declarations.
3. Keep any live service examples explicitly manual and outside CI.

### Tasks: tests

Create:

```text
construction.rs
local_runtime.rs
riot_contract.rs
```

- `construction` imports every compile-only declaration and constructs representative endpoints/plans.
- `local_runtime` uses shared deterministic test support to execute actual example APIs.
- `riot_contract` compiles the real Riot APIs and tests representative behavior.

Delete `tests/canonical_examples.rs` synthetic `DeterministicApi`.

### Tasks: documentation links

Each example module has module-level docs stating:

- what it teaches;
- required features;
- whether it is construction-only, locally runnable, or manual-live;
- owning guide/reference page;
- proving test.

### Validation

```bash
cargo check -p concord_examples --all-targets
cargo check -p concord_examples --all-targets --all-features
cargo test -p concord_examples --all-features
cargo test -p concord_examples --doc --all-features
```

Run tests with network disabled where the environment supports enforcement.

### Exit criteria

- Every canonical API is importable.
- Every binary is thin.
- Tests import actual example declarations.
- Riot is split into understandable domain/API/usage/test modules.
- No CI example requires external network.
- Every capability in the façade feature narrative has an explicit example or documented reason for omission.

### Risks

- Examples becoming test-support-heavy and unreadable. Keep infrastructure in `concord_test_support`.
- Feature gates making modules invisible to rust-analyzer; document an all-features editor configuration.
- Live Riot behavior contaminating deterministic tests.

---

## Phase 12 — Rebuild tests and fixtures around contracts

### Goal

Apply the proof taxonomy, remove duplicated/obsolete tests, consolidate graph fixtures, and make invariant ownership explicit.

### Prerequisites

Examples are importable; public package/crate graph is final enough to classify tests accurately.

### Tasks: inventory and mapping

1. Create a checked-in test inventory table, likely in contributor docs, mapping every product invariant to its primary test owner.
2. Classify every current test file and fixture using Section 15.
3. Before deleting anything, identify the exact assertion and its replacement test.
4. Do not use test count as a success metric; use invariant coverage and locality.

### Tasks: core tests

1. Regroup integration tests under lifecycle/security/bounds/transport/configuration harnesses.
2. Move governor/OAuth/concrete codec/controller tests out of core.
3. Keep core tests focused on generic contracts and ordering.
4. Replace giant monolithic test modules only when grouping improves navigation; avoid a risky all-at-once rewrite.
5. Preserve deterministic executor usage and exact ordering assertions.

### Tasks: façade tests

1. Own all public generated surface, compile-fail, rustdoc, and downstream extension tests.
2. Ensure integration tests import only `concord`.
3. Move current macro generated tests here and adapt to final named wrappers/modules.
4. Replace broad source scans with compiler proofs.
5. Keep focused rustdoc tests for public paths/methods/privacy.

### Tasks: extras tests

- Ensure moved implementations have direct unit tests and integration tests against public core contracts.
- Add a package metadata assertion that extras has no façade/macro dependency.

### Tasks: fixtures

1. Create one root `fixtures/Cargo.toml` and one lockfile.
2. Consolidate feature absence bins under one feature package where Cargo feature resolution remains correctly isolated.
3. Keep renamed façade fixture.
4. Re-evaluate contract mismatch fixture:
   - retain only if the assertion genuinely requires package replacement;
   - otherwise prove the constant handshake in workspace and remove the graph.
5. Remove:
   - standalone enabled-feature fixtures;
   - extension certification package;
   - missing-core fixture;
   - development-tools enabled package.
6. Put fixture target directories under root `target/fixtures`.
7. Keep fixture source minimal and manifest-focused because it is outside normal editor indexing.

### Tasks: UI tests

1. Delete tests protecting:
   - fixed clause ordering;
   - missing generated prelude imports;
   - old `PreparedEndpoint` name after migration;
   - old OAuth/retry DSL after final removal.
2. Add tests for final public-language guarantees.
3. Inspect every `.stderr` update.
4. Group UI fixtures by malformed syntax, names, auth, pagination, features, privacy, and capabilities.

### Tasks: feature matrix

At minimum prove:

```text
façade default
façade no-default
façade all-features
json only
records only
csv
ndjson
csv+ndjson
multipart
rate-limit-governor
no governor + custom limiter
no governor + declared rate limits (fail closed)
oauth2-client
dangerous raw response
dangerous custom retry
transport compression/TLS/HTTP2 combinations relevant to current support
```

### Editor consequences

- Main root should index all substantive source/tests/examples.
- Only minimal graph fixtures remain outside.
- One fixture lockfile reduces noise.
- Generated Trybuild temp projects stay under ignored target directories.
- Document linked-project configuration only for contributors editing fixtures.

### Validation

```bash
cargo test --workspace
cargo test --workspace --all-features
cargo test -p concord --test ui --all-features
cargo run -p concord-xtask -- fixtures   # once xtask exists; otherwise explicit cargo commands
cargo metadata --format-version 1 --all-features
find . -name Cargo.lock -not -path './target/*'
```

### Exit criteria

- Every remaining standalone package has a graph-dependent justification.
- One fixture lockfile remains.
- No canonical example is recreated in tests.
- Every invariant has one primary owner.
- Main rust-analyzer workspace contains all substantive tests/examples.
- Obsolete behavior tests are removed only after replacement proof exists.

### Risks

- Losing subtle security coverage during consolidation.
- Feature unification making a supposedly absent feature present; validate each fixture graph with metadata/tree.
- Trybuild expected output varying across toolchains; use declared MSRV/stable lanes intentionally.

---

## Phase 13 — Redesign documentation, workspace layout, tooling, and CI

### Goal

Make repository navigation mirror product architecture, establish one documentation learning path and canonical fact ownership, and provide portable repeatable validation.

### Prerequisites

Public API, examples, tests, and fixtures are stable.

### Tasks: documentation

1. Create the target documentation tree in Section 16.
2. Rewrite Guide 1–3 from the ideal public sketches.
3. Link every substantial example to its importable module and proving test.
4. Move lifecycle/security detail to concept pages after initial declaration/usage explanation.
5. Put exact syntax/features/limits/errors under canonical references.
6. Remove stale paths, two-crate instructions, manual export inventory, and duplicate defaults.
7. Keep private bridge/macro implementation language in contributor docs only.
8. Generate or check feature and limit tables.
9. Add link checking and doctests.

### Tasks: workspace move

1. Move packages under `crates/` and tools under `tools/` as shown in Section 13.
2. Perform path moves separately from semantic changes; update manifests/import-independent paths and run checks immediately.
3. Set root workspace members/default members so ordinary `cargo check` includes:
   - façade;
   - core;
   - extras;
   - macros;
   - examples.
4. Decide whether test support/perf/xtask are default members based on ordinary contributor cost; they remain workspace members.
5. Explicitly exclude the root fixture workspace.
6. Keep one root lockfile and one fixture lockfile.

### Tasks: portable tooling

1. Remove global `powershell.exe` shell selection from `justfile`.
2. Use portable direct `cargo` recipes for simple commands.
3. Add a Rust `xtask` only for orchestration that is genuinely awkward in `just`, such as fixture matrices, generated docs tables, or API inventories.
4. Replace missing `scripts/check.py` references with actual commands.
5. Ensure `just --list` matches contributor docs.
6. Keep benchmarks observational and separate from correctness gate where appropriate.

### Tasks: CI lanes

Define explicit lanes:

1. Formatting.
2. MSRV (`1.85` unless intentionally changed) no-default and representative packages.
3. Stable default workspace.
4. Stable all-features workspace.
5. Clippy default/all-features.
6. Core lifecycle/security.
7. Façade UI/generated surface.
8. Extras implementations.
9. Feature/dependency fixtures.
10. Examples/doctests/rustdoc/link checks.
11. Cargo deny/license/advisory policy.
12. Performance smoke (non-blocking initially, blocking only after stable budgets).
13. Platform matrix at least Linux/Windows/macOS for portable commands and transport support as project policy allows.

### Tasks: rust-analyzer guidance

Document:

- open the root workspace for product development;
- optional all-features rust-analyzer Cargo configuration;
- fixtures are intentionally separate and opened only when editing graph tests;
- no nested product workspace is required;
- generated target directories are ignored.

### Tasks: publishing and lockfile policy

1. Finalize `publish`, repository, license, docs, and package metadata.
2. Internal packages use exact sibling versions.
3. Verify `cargo package`/`cargo publish --dry-run` ordering if publication is intended.
4. Root library workspace keeps `Cargo.lock` for reproducible CI; fixture workspace keeps its own lock.
5. Do not check in per-fixture lockfiles.

### Validation

Target commands:

```text
just fmt
just check
just check-all
just lint
just test
just test-all
just test-msrv
just ui
just fixtures
just examples
just docs
just security
just perf-smoke
just verify
```

Underlying commands include:

```bash
cargo +1.85 check --workspace --all-targets
cargo +1.85 test -p concord --no-default-features
cargo check --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo test --workspace --all-features
cargo test --workspace --doc --all-features
cargo rustdoc -p concord --all-features -- -D warnings
cargo deny check
```

### Exit criteria

- Repository top-level architecture is immediately visible.
- Documentation begins with representation/generated usage, not internals.
- Exact facts have one owner.
- Commands are portable and truthful.
- MSRV is explicitly proven.
- Main editor workspace has no nested product workspaces.
- Lockfile policy is simple and documented.

### Risks

- Path move churn obscuring semantic history; make dedicated commits/diffs.
- Generated docs tables drifting if tooling is overcomplicated.
- CI matrix cost; group commands without dropping proof categories.

---

## Phase 14 — Remove compatibility surface, harden release, and complete validation

### Goal

Remove every temporary legacy path/grammar/alias, run the full proof matrix, measure compile/runtime effects, and prepare a coherent release.

### Prerequisites

All earlier phases complete and green.

### Compatibility removals

Remove:

- macro fallback resolving direct `concord_core`;
- documented or code-supported two-crate user setup;
- configured positional client constructors;
- generated per-client preludes and request extension traits if any compatibility aliases remain;
- old public `facades`/`endpoints` paths;
- `PreparedEndpoint` aliases;
- legacy PascalCase endpoint/scope grammar;
- fixed-order parser compatibility;
- inferred final parameter named `body` behavior;
- built-in `oauth2_client` DSL source;
- `retry custom PATH` DSL syntax;
- temporary core OAuth feature/dependencies;
- old feature names and docs;
- obsolete UI/fixture tests that existed only for legacy behavior.

Bump the generated contract version for the final bridge.

### Final API audit

1. Generate and inspect public rustdoc for `concord`.
2. Confirm ordinary user paths never require implementation crate names.
3. Confirm top-level/prelude surface is curated and domain modules are complete.
4. Confirm no private bridge types leak through public signatures.
5. Confirm generated direct methods and request wrappers use predictable names.
6. Confirm every DSL construct has a documented generated-Rust consequence.
7. Confirm examples compile with only façade dependency.

### Security and boundedness audit

Run and review proofs for:

- secret-safe Debug/Display/errors;
- auth query/header redaction;
- reserved auth headers;
- provider HTTP isolation and limits;
- hook-before-body/auth ordering;
- limiter-before-final-auth ordering;
- one challenge recovery;
- sanitized response hooks;
- bounded status bodies;
- first-excess withholding;
- bounded streams/records/pagination/identities/limiter state;
- safe planning with zero effects;
- fixed-result completion;
- dangerous raw response bounded and feature-gated;
- no alternate transport/native escape through extras.

### Compile-time/editor audit

- run all UI tests on MSRV and stable where expected outputs are supported;
- execute rust-analyzer acceptance protocol;
- inspect expansion for minimal and large APIs;
- verify dependency rename;
- verify macro graph lacks extras/core runtime dependencies where selected;
- verify feature-disabled public absence.

### Performance and compile metrics

Measure against Phase 0/6 baselines:

- macro token/item count at 1/10/100/1,000 endpoints;
- clean/incremental `cargo check`;
- rustdoc time;
- endpoint construction;
- request binding;
- public-head planning;
- direct execution overhead;
- explicit/runtime endpoint overhead;
- pagination per page;
- auth ready path;
- limiter ready path;
- record framing smoke.

Record results. Do not invent hard budgets after the fact; establish baselines and document material regressions.

### Full validation

Run every final `just`/`xtask` gate, all feature/dependency fixtures, package dry runs, docs links, and platform CI.

### Release artifacts

- migration guide mapping old imports/types/syntax to final equivalents;
- changelog organized by public contract, runtime semantics, crates/features, examples/tests/docs;
- final package dependency/version table;
- generated contract version note for contributors;
- known limitations and intentionally unsupported capabilities;
- final updated progress ledger with all commands and deviations.

### Exit criteria

- No temporary compatibility behavior remains unless explicitly accepted as permanent product API.
- One façade dependency is the only documented setup.
- Core, macros, and extras boundaries match this document.
- All non-negotiable invariants are proven.
- Examples, docs, and tests use the actual final product surface.
- Workspace and tooling are portable and coherent.
- Performance/compile effects are measured and documented.

### Risks

- Removing a compatibility alias still used internally; find all paths before deletion.
- Package publication order/path dependencies.
- Cross-platform or MSRV-only failures appearing late; CI lanes should have been active earlier.

---

# 18. Cross-phase validation and proof matrix

This section is the final operational checklist. Phase-local commands remain authoritative for each phase; this matrix prevents a later phase from forgetting an earlier product invariant.

## 18.1 Validation principles

1. A proof must execute at the lowest semantic layer that owns the invariant.
2. A passing unit test cannot substitute for a public downstream compile test when the claim concerns generated or reexported API shape.
3. A workspace all-features build cannot prove feature absence; use the isolated feature fixture.
4. A source-text scan cannot prove a privacy or capability boundary; use Rust visibility and compiler failures. Source scans may only supplement dependency-policy checks.
5. A compile-only example does not prove request shaping or lifecycle behavior; pair it with safe-plan or deterministic runtime assertions when those are the teaching claim.
6. No test may call a public internet service. Use the deterministic executor or loopback server from `concord_test_support`.
7. Avoid time-sensitive sleeps. Use paused time, scripted permits, deterministic clocks, or explicit notification points.
8. Every command recorded in the progress ledger must include the exact toolchain and feature selection.
9. A skipped command is a recorded blocker, not a pass.
10. After package/path moves, rerun dependency, rustdoc, doctest, and fixture checks even when runtime code did not change.

## 18.2 Required command families

The implementing agent may refine command spelling after the final package paths exist, but it must preserve the following proof families.

### Formatting and ordinary compilation

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo check --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

### MSRV

The manifest currently declares Rust 1.85. Keep that value unless a separately approved product decision changes it.

```bash
cargo +1.85.0 check --workspace --all-targets
cargo +1.85.0 test -p concord --no-default-features
cargo +1.85.0 test -p concord-core --no-default-features
cargo +1.85.0 test -p concord-extras --no-default-features
```

Any dependency that raises MSRV must be rejected, feature-isolated, pinned, or accompanied by an explicit MSRV decision and documentation update.

### Runtime and package tests

```bash
cargo test --workspace
cargo test --workspace --all-features
cargo test -p concord-core
cargo test -p concord-extras --all-features
cargo test -p concord --all-features
cargo test -p concord-examples --all-features
```

Use `cargo nextest` as an acceleration layer when installed, not as the only documented way to run tests.

### Compiler diagnostics and public surface

```bash
cargo test -p concord-macros --test ui
cargo test -p concord --test ui
cargo test -p concord --test generated_surface
cargo test -p concord --test public_extensions
cargo test -p concord --test public_compile
```

Update `.stderr` files only after checking that the new text reflects the target public language rather than hiding a regression.

### Documentation

```bash
cargo test --workspace --doc
RUSTDOCFLAGS='-D warnings' cargo doc -p concord --all-features --no-deps
RUSTDOCFLAGS='-D warnings' cargo doc -p concord-core --all-features --no-deps
RUSTDOCFLAGS='-D warnings' cargo doc -p concord-extras --all-features --no-deps
```

Run the repository link checker or `xtask docs` once introduced. Generated rustdoc for the façade is the public-path authority.

### Feature and dependency fixtures

```bash
cargo run -p concord-xtask -- fixtures
```

The command must exercise, at minimum:

- renamed façade dependency;
- generated contract mismatch;
- JSON absent;
- CSV absent;
- NDJSON absent;
- multipart absent;
- OAuth2 client helper absent;
- governor absent with generated non-empty plans failing closed;
- dangerous custom retry absent;
- dangerous raw response absent;
- TLS, HTTP/2, and compression absence where public APIs differ;
- no accidental feature unification from another package;
- no direct core/macros/extras dependency in canonical examples.

### Packaging

```bash
cargo package -p concord-core --allow-dirty
cargo package -p concord-extras --allow-dirty
cargo package -p concord-macros --allow-dirty
cargo package -p concord --allow-dirty
```

When registry publication is intended, also inspect each generated package archive and perform a temporary-registry or path-rewritten smoke build in dependency order.

### Dependency policy

Use `cargo metadata`, `cargo tree`, and the repository policy tool to prove:

- `concord-core` has no edge to macros, extras, or façade;
- `concord-extras` has only the intended edge to core among Concord packages;
- `concord-macros` has no normal edge to core, extras, or façade;
- `concord` owns all three implementation dependencies;
- examples depend on `concord`, not directly on implementation crates;
- JSON/CSV/NDJSON/OAuth/governor ecosystem dependencies do not remain in core;
- proc-macro host builds do not compile the runtime dependency graph unnecessarily;
- exact sibling versions are used for publishable Concord packages.

## 18.3 Public declaration-to-usage proof matrix

| Public-language claim | Required proof |
|---|---|
| One dependency is sufficient | Minimal external fixture depending only on renamed `concord` |
| Macro is extras-agnostic | Dependency graph plus APIs using local third-party marker/controller/provider types unknown to macros |
| A type in response position is treated generically | Compile-pass APIs using extras JSON, local response spec, and a renamed marker type |
| A pagination controller is treated generically | Compile-pass APIs using extras controllers and a local custom controller |
| Endpoint method names are predictable | Surface tests for snake_case source to method/PascalCase type mapping, including acronyms |
| Scope topology is predictable | Surface and rustdoc tests for nested scope modules and facade methods |
| Required parameters are positional only at endpoint/scope call sites | Signature assertions and compile-fail missing/extra argument tests |
| Optional/default values are inherent setters | No-import compile-pass tests and rust-analyzer protocol |
| Invalid capability terminals are absent | Compile-fail tests for paginate/store/raw response on incompatible endpoints |
| Explicit endpoint and direct method are equivalent | Deterministic plan/execution equality tests |
| Endpoint values are reusable where semantically safe | Clone/rebind tests for bodyless endpoints and explicit non-Clone behavior where required |
| Configured clients have named construction | Surface tests and absence of multi-argument `new` |
| Secrets are never positional or debug-visible | Builder signature tests and redaction matrix |
| Clause ordering is non-semantic | Parser permutation unit tests and compile-pass declarations |
| Duplicate clauses are rejected precisely | Trybuild diagnostics containing both source spans |
| Body role is explicit | Compile-fail inferred legacy body tests and compile-pass non-final `#[body]` tests |
| Runtime endpoint shares the same executor | Lifecycle event equality tests |
| Safe plan has no effects | Counters proving no hook, provider, limiter, body factory, stream poll, transport, or completion call |

## 18.4 Core lifecycle proof matrix

| Invariant | Required deterministic proof |
|---|---|
| Endpoint/request construction is inert | All effect counters remain zero before first poll |
| Immutable client snapshot | Clone equality and post-build mutation impossibility |
| Public head is built before private auth | Ordered events and safe-head assertions |
| Known finite checks precede hooks | Oversized known body fails with no hook/auth/limiter/transport calls |
| Before-send hook precedes body materialization | Scripted event order |
| Body metadata and prepared body agree | Mismatch produces bounded body error before transport |
| Credential preflight precedes limiter acquisition | Ordered provider/limiter events |
| Limiter acquisition precedes final credential application | Ordered events; no secret in limiter key/context |
| Native request is private | External compile-fail and hook/event type checks |
| Challenge recovery is bounded | At most two transport attempts and one refresh/reacquire path |
| Status handling precedes success decoding | Decoder not called for rejected status |
| Response/status bodies are bounded | Known-length early failure and unknown-length first-excess withholding |
| Streaming remains bounded | Byte/item/error behavior and cancellation tests |
| Pagination is sequential | Maximum in-flight page count is one |
| Pagination is all-or-error | Later page failure returns no partial aggregate |
| Identity cycle detected before repeated send | Send count and repeated identity assertions |
| Completion follows result fixation | Hook cannot change returned result; exactly-once event |
| Cancellation has no detached correctness work | Drop tests and no post-drop event progression |
| Dangerous raw success remains bounded | Raw terminal still uses bounded acquisition and is feature-gated |

## 18.5 Extras proof matrix

| Extra | Required proof |
|---|---|
| `Json<T>` | Request encode, response decode, media validation, size/error reduction, no parser text leak |
| `Csv<T>` | Chunk-boundary framing, headers/options as supported, item and record limits, reduced errors |
| `NdJson<T>` | CRLF/LF handling as specified, chunk boundaries, blank-line policy, item limits, reduced errors |
| Offset/page/cursor controllers | Initial state, continuation, stop conditions, identity behavior, overflow, pending bindings |
| Governor limiter | Plan installation, bounded keys/state, authority, cooldown clamp, cancellation-safe permits, no task-local dependency |
| Retry-After observer | Header parsing, invalid/overflow behavior, cooldown proposal only, no unsafe header exposure |
| OAuth2 client provider | URL validation, form construction, Basic/body client auth policy if supported, JSON token decode, expiry, provider limits, redaction, refresh/reacquire behavior |
| Third-party parity | At least one test-only external-style implementation for every public extension trait family without private imports |

## 18.6 Security review gates

A phase touching authentication, request heads, hooks, errors, endpoint descriptions, planning, rate-limit keys, or debug output must rerun the complete security suite rather than only local unit tests.

The suite must search structured outputs and byte buffers for all seeded secrets and sensitive dynamic values. Seed values should include:

- authorization tokens;
- API-key headers and query values;
- OAuth client ID and secret;
- endpoint-backed credentials;
- protected path/query parameters;
- provider response body sentinels;
- multipart filenames or values marked protected;
- rate-limit identity values when protected;
- URL userinfo sentinels.

The test should cover `Debug`, `Display`, error sources exposed publicly, hooks, completion, plans, descriptions, logs emitted by test observers, and panic-free failure paths.

## 18.7 Rust-analyzer acceptance protocol

This remains a manual release proof because compiler tests do not reproduce editor ranking and navigation.

1. Open only the main workspace with no fixture workspace linked.
2. Use the documented all-features rust-analyzer configuration.
3. Create a minimal API and confirm completion for generated client/scope methods.
4. Hover a direct endpoint call and confirm a named request wrapper appears.
5. Type `.` after a request and confirm valid inherent setters/terminals appear without imports.
6. Confirm an incompatible request does not suggest pagination, acquisition, or dangerous terminals.
7. Go to definition from the client method, endpoint type, request type, and optional setter. Confirm navigation is useful and stable.
8. Trigger a duplicate clause, name collision, unknown credential, invalid body spec, and feature absence. Confirm diagnostics use public concepts and relevant spans.
9. Rename the façade dependency in a sample package and repeat completion/navigation.
10. Confirm fixtures do not add unrelated diagnostics or duplicate lockfile indexing to the main workspace.

Record rust-analyzer and editor versions in the release validation note; do not make the product depend on one editor-specific extension.

## 18.8 Performance and expansion protocol

Create generated API fixtures of approximately 1, 10, 100, and 1,000 endpoints using the same semantic mix. Measure:

- emitted token count and generated item count;
- clean `cargo check` wall time and peak memory when tooling permits;
- incremental check after changing one endpoint;
- rustdoc generation time;
- request construction and binding overhead;
- public planning overhead;
- no-op deterministic execution overhead;
- explicit endpoint versus direct method overhead;
- runtime endpoint overhead;
- pagination per-page overhead;
- ready credential and limiter paths.

Store machine/toolchain details and raw results under `tools/perf/results/` or another explicitly ignored/generated location. The repository should contain the harness and baseline summary, not unbounded build artifacts.

## 18.9 Phase dependency graph

```text
0 baseline/freeze
└─► 1 façade graph
    ├─► 2 pure extras
    │   ├─► 3 governor
    │   └─► 4 OAuth2
    └──────────────► 5 features/namespaces
                       └─► 6 generated request surface
                           ├─► 7 client/runtime policy
                           ├─► 8 DSL v2
                           └─► 9 descriptions/runtime endpoints
                                └─► 10 macro internals
                                     └─► 11 examples
                                          └─► 12 tests/fixtures
                                               └─► 13 docs/workspace/CI
                                                    └─► 14 removal/release
```

Phases 3 and 4 may proceed in either order after Phase 2, but both must finish before Phase 5 is considered complete. Do not begin the macro file reorganization in Phase 10 before Phases 6–9 have stabilized the generated contract and hidden bridge.

---

# 19. Conscious design choices and bounded deferrals

Most design choices are fixed in Section 2. The following require an explicit implementation-time resolution because stable Rust, compatibility, or evidence may affect the exact shape. The agent must choose the recommended default unless evidence requires a deviation record.

## 19.1 Package naming and publication

**Recommended:** package names use conventional hyphens (`concord-core`, `concord-macros`, `concord-extras`) while Rust crate paths remain underscores (`concord_core`, etc.). The user-facing package is `concord`.

Decide and document:

- whether implementation crates are independently documented on docs.rs or marked as implementation details;
- exact sibling dependency versions for publication;
- publication order and ownership metadata;
- whether the current `publish = false` posture is temporary or intentional.

A façade package on crates.io cannot depend on unpublished sibling packages. This is a release blocker, not a documentation detail.

## 19.2 Façade reexport strategy

**Recommended:** selected common types are explicit top-level reexports; complete capability sets live in semantic modules; `prelude` is optional and small; `advanced` is curated rather than a dump.

Avoid glob-reexporting core and extras wholesale. That would create collision risk, make ownership invisible, and prevent future independent evolution.

## 19.3 Macro dependency on core

**Recommended:** no normal dependency. The macro should hold its own compact generated-contract constant and emit a hidden bridge check through `concord::__private`.

A normal core dependency is acceptable only if compile-time evidence demonstrates a necessary shared source model that cannot live in a small dependency-neutral crate. Do not create such a shared crate speculatively.

## 19.4 Named request wrapper implementation

**Recommended:** a transparent generated wrapper around the core generic request with delegated inherent methods and `IntoFuture`.

Evaluate:

- whether `Deref` should be avoided to prevent exposing invalid generic methods;
- conditional `Clone`, `Debug`, `Send`, and `Sync` behavior;
- whether common terminals are emitted directly or inherited through a sealed internal trait invisible in diagnostics;
- expansion size versus clarity.

The public result must not require an extension trait import.

## 19.5 Client builder completeness

**Recommended:** ordinary runtime-validated builder with a focused `ClientBuildError`, not typestate. Secrets and providers are named setters.

Typestate may be reconsidered only with evidence that it does not create prohibitive generated types, compile time, feature interactions, or poor diagnostics for large clients.

## 19.6 Body annotation syntax

**Recommended:** `#[body] input: Json<CreateUser>` because it resembles Rust parameter attributes.

A dedicated `body input: Spec` grammar is acceptable only if parser diagnostics and rustfmt-like readability are demonstrably better. Do not retain name-based inference in the final grammar.

## 19.7 Optional type recognition

**Recommended:** recognize syntactic `Option<T>` and `std::option::Option<T>`; document that aliases are ordinary required types unless an explicit modifier is added in a future language version.

Do not attempt semantic type resolution in the proc macro.

## 19.8 Endpoint description ownership and identity

**Recommended:** static generated descriptions borrow `'static` contract data; runtime endpoints use bounded owned data; both expose one read-only public shape.

Provide an optional explicit stable ID clause. Generated default IDs may follow source topology, but documentation must state rename consequences. Rate-limit/telemetry identity should derive from the contract ID rather than raw Rust type names.

## 19.9 Safe planning depth

**Recommended:** planning includes method, sanitized URL, public sanitized headers, endpoint ID, body metadata, response mode, timeout selection, and lexical rate-limit metadata. It excludes encoded body bytes and all effects.

Do not add an “exact native request preview.” That conflicts with private auth and body materialization boundaries.

## 19.10 Runtime pagination

**Recommended bounded deferral:** runtime-defined endpoints are non-paginated in this refactor unless a complete typed controller/binding/result contract emerges without a parallel executor.

Do not add a partial dynamic pagination API merely for feature parity.

## 19.11 Multipart ownership

**Fixed:** core. Revisit only if core later exposes a fully semantic, transport-neutral multipart specification that still permits core to create the native form, enforce all bounds, and control replayability. Moving the marker alone while retaining a private friend bridge in extras is not useful separation.

## 19.12 Governor default composition

**Recommended:** core defaults to fail-closed; façade installs extras governor when the façade feature is active. Explicit user limiter always wins.

The exact hidden bridge function may differ, but macros must not name the governor and extras must not be a core dependency.

## 19.13 OAuth2 declaration migration

**Recommended:** remove macro-special `oauth2_client`; declare a generic provider type and pass an `OAuth2ClientCredentials` instance through the client builder.

A compatibility parser may lower old syntax to the generic provider model temporarily. It must be removed in Phase 14.

## 19.14 Default façade features

**Recommended initial set:** preserve the current ordinary runtime defaults—TLS, HTTP/2, governor—while keeping JSON opt-in. Record the exact cost of these defaults after extraction.

Do not enable every extra by default merely because the façade aggregates them. One dependency does not mean all transitive ecosystems must compile.

## 19.15 Hook completion budget

**Recommended:** preserve current result-fixation and exactly-once observation semantics. Do not silently detach completion work. A configurable observation budget is a separate product decision and must not alter the returned result.

## 19.16 Compatibility strategy

**Recommended:** because the workspace is version `0.1.0` and marked unpublished, implement a clean target API with short-lived internal compatibility aliases only where they keep intermediate phases green. Avoid permanent dual surfaces.

Before relying on this recommendation, verify repository publication/use history and current downstream commitments. Record contrary evidence.

---

# 20. Final definition of done

The refactor is complete only when all of the following are true.

## 20.1 Product and dependency surface

- A normal user declares one dependency, `concord`.
- Canonical imports and examples never name `concord_core`, `concord_macros`, or `concord_extras`.
- `concord_macros` has no knowledge of JSON, CSV, NDJSON, OAuth2, governor, or built-in pagination controller names.
- A third-party marker/controller/provider implementation works through the same public contracts as extras.
- The package graph contains no forbidden cycle or accidental feature unification.

## 20.2 Public language

- The DSL resembles the remote contract and has order-independent named clauses.
- Generated method/module/type names are predictable from source identifiers.
- Every endpoint has a public endpoint value, named request wrapper, safe description, and exact capability set.
- Ordinary setters and terminals are inherent and rust-analyzer discoverable.
- Configured clients use named builders and secrets are never positional.
- Direct calls, explicit endpoint values, and runtime endpoints converge on the same core lifecycle.
- Invalid operations fail at compile time with public-language diagnostics.

## 20.3 Semantic architecture

- Core owns every lifecycle, transport, security, redaction, boundedness, and completion invariant.
- Extras contains only downstream semantic implementations and has no native transport access.
- The façade owns public paths, user features, defaults, and hidden generated composition.
- The macro owns source compilation, not runtime implementations.
- No public/private type duplicates another semantic phase merely for organizational convenience.

## 20.4 Proof and narrative

- Every product invariant has one primary test owner.
- Every independent fixture has a documented graph-dependent reason.
- Canonical examples are importable and directly tested.
- No CI test or example uses external network access.
- Documentation follows representation → generated Rust → usage → execution → capabilities → reference.
- Exact features, limits, signatures, and commands each have one canonical owner.
- rust-analyzer acceptance, package dry runs, MSRV, all-feature, no-default, security, fixtures, docs, and performance smoke are recorded.

## 20.5 Handoff record

At completion, the progress ledger must contain:

- every phase marked complete;
- the final worktree/commit identifier;
- exact commands run and their result;
- every deviation record;
- unresolved limitations distinguished from defects;
- migration and release-document locations;
- a concise list of intentionally unsupported capabilities.

The final implementing-agent response must summarize changes by product contract, not by file count, and must not claim unexecuted validation.
