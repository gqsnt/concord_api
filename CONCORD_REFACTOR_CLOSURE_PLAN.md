# Concord Refactor Closure Plan

**Document role:** authoritative implementation plan for the work that remains after the Concord v2 architectural refactor
**Companion audit:** `CONCORD_REFACTOR_AUDIT.md`
**Original architectural specification:** `CONCORD_REFACTOR_REFERENCE.md`
**Scope:** close the remaining correctness, boundedness, validation, packaging, and evidence gaps without reopening completed architecture
**Implementation mode:** direct repository work, phase-by-phase, with one evidence record and one intentional commit per phase
**Push policy:** do not push unless the repository owner explicitly requests it

---

## 0. How to use this document

This plan supersedes the original Phase 0–14 plan **only for remaining work**. The original architecture reference remains authoritative for the product thesis, public-language model, crate ownership, runtime lifecycle, security invariants, examples, tests, and documentation journey.

The implementation agent must read, in order:

1. `CONCORD_REFACTOR_CLOSURE_PLAN.md`;
2. `CONCORD_REFACTOR_AUDIT.md`;
3. Sections 0–7 and 18–20 of `CONCORD_REFACTOR_REFERENCE.md`;
4. the source and tests relevant to the current closure phase;
5. all prior closure evidence and deviation records.

After any context compaction, interruption, or handoff, re-read:

- Sections 1–5 of this document;
- the complete active closure phase;
- the progress ledger;
- unresolved deviation and blocker records.

Do not treat the previous completion summary as source truth. Verify the current Git checkout, manifests, source, tests, workflow files, and command results directly.

---

# 1. Closure mission

Finish the remaining work required to make the following statement true:

> Concord v2 has one façade dependency, an extras-agnostic contract compiler, ordinary generated Rust request types, one bounded and redaction-aware managed executor, incrementally bounded runtime construction, executable validation ownership, a technically package-verifiable product graph, and evidence whose claims exactly match the source and observed results.

This is a closure assignment, not a second broad refactor.

The implementation must:

- repair the finite-construction defects;
- complete rate-limit and OAuth2 first-party bounds;
- finish small public and macro semantic boundaries;
- remove test-support leakage from the public package graph;
- add executable CI and a passing MSRV diagnostic strategy;
- make documentation and validation claims truthful;
- perform all locally possible proofs;
- explicitly retain external blockers instead of claiming completion.

---

# 2. Completed architecture that is frozen

The following work is accepted as the baseline and must not be redesigned unless a closure change directly requires a narrow adjustment.

## 2.1 Crate architecture

Keep:

```text
concord ───────────► concord-core
   ├──────────────► concord-macros
   └──────────────► concord-extras ───► concord-core

concord-examples ─► concord
```

Also retain the internal support packages:

```text
concord-test-support
concord-xtask
concord-perf
```

Do not:

- move JSON, CSV, NDJSON, built-in pagination, governor, or OAuth2 back into core;
- add a macro dependency on extras;
- make examples depend directly on core or macros;
- add an alternate executor;
- expose Reqwest through extras.

## 2.2 Public language and generated API

Keep:

- one documented `concord` application dependency;
- macro paths through the caller-visible façade and `concord::__private`;
- DSL v2 snake_case source identifiers;
- order-independent clauses;
- explicit `#[body]`;
- syntactic `Option<T>` optionality;
- stable optional contract IDs;
- named client builders;
- zero-input-only `Client::new()`;
- named endpoint request wrappers;
- inherent optional/default setters and legal terminals;
- one scope-oriented generated module tree;
- explicit generated endpoint values;
- safe endpoint descriptions and planning;
- `RuntimeEndpoint`;
- one sealed generated endpoint capability trait;
- no generated request extension traits or generated preludes.

Do not reintroduce compatibility branches removed during the v2 refactor.

## 2.3 Runtime model

Keep every established lifecycle and security property:

1. endpoint and request construction are inert;
2. built clients are immutable snapshots shared by clones;
3. direct, explicit, and runtime endpoints use one executor;
4. public head construction precedes private authentication;
5. hooks, body work, credentials, limiting, authentication, status handling, acquisition, result fixation, and completion retain their documented order;
6. authentication recovery remains bounded to one additional attempt;
7. bodies, responses, streams, status capture, records, pagination, identities, limiter state, cooldowns, and provider HTTP remain finite;
8. first excess is withheld;
9. pagination remains sequential, cycle-detecting, and all-or-error;
10. native requests, responses, and authentication material remain private;
11. plans, descriptions, errors, hooks, completion, `Debug`, and `Display` remain redacted;
12. dangerous raw-response behavior remains explicit, feature-gated, and bounded;
13. cancellation does not detach correctness work.

No closure task may weaken these invariants to simplify an implementation.

## 2.4 Examples, fixtures, and documentation journey

Keep:

- importable example modules;
- thin binaries;
- tests importing actual example definitions;
- split Riot domain/API/usage narrative;
- one isolated fixture workspace for genuine Cargo-graph proofs;
- the declaration → generated Rust → construction/use → execution → capabilities → reference documentation order.

The closure may correct facts, links, commands, generated tables, and evidence, but must not return to duplicated tutorials or synthetic canonical APIs.

---

# 3. Remaining issue registry

The audit findings are mapped to closure phases below.

| Audit finding | Closure phase | Priority | Closure requirement |
|---|---:|---:|---|
| F-01 Runtime endpoint construction validates after allocation/collection | C1–C2 | Blocker | Incremental budgets and no-overconsumption |
| F-02 Rate-limit semantic values are structurally unbounded | C1–C3 | Blocker | Core-owned hard limits and fallible incremental constructors |
| F-03 CI workflow absent | C7 | Blocker for validation claim | Workflow in source, linted, required jobs defined |
| F-04 Final release proof incomplete | C8 | Release blocker | Honest final status and completed local/external gates |
| F-05 Public package graph depends on test support | C6 | High | Remove normal public product edge |
| F-06 MSRV diagnostic proof has no passing owner | C7 | High | Stable exact snapshots plus passing semantic MSRV proof |
| F-07 Rust-analyzer acceptance unexecuted | C7–C8 | High | Execute manually where possible; otherwise retain explicit external blocker |
| F-08 OAuth2 pre-transport amplification is unbounded | C4 | High | Bounded configuration and checked single-allocation encoding |
| F-09 Panic-prone `EndpointId: From<&'static str>` | C5 | Medium-high | Remove infallible public conversion |
| F-10 Macro emitter imports source AST vocabulary | C5 | Medium | Emit depends only on normalized semantic/surface model |
| F-11 Client timeout name/semantics are misleading | C5 | Medium | Rename runtime-only setting and correct docs |
| F-12 `dangerous-dev-tools` is public | C6 | Medium | Remove public feature and public test-support edge |
| F-13 `just verify` is not reproducible/complete | C7 | Medium-high | Locked commands and precise gate scopes |
| F-14 Evidence contains false or unchecked claims | C7 | Medium | Correct evidence and strengthen policy checks |
| F-15 README installation is not currently executable | C7 | Medium | State unpublished status before future registry syntax |
| F-16 Runtime structural limits lack canonical owner | C1, C3, C7 | Medium | Public constants/reference generated or mechanically checked |
| F-17 Residual direct-core bridge helpers are obsolete | C5–C6 | Low-medium | Remove or establish intentional owner |
| F-18 Broader formatted-value boundedness is accidental | C1, C4 | Medium design requirement | Bound Concord-owned capture; document user-code trust boundary |

---

# 4. Binding closure decisions

These decisions prevent the agent from spending another phase re-litigating the architecture.

## 4.1 Runtime endpoint builder API

Preserve fluent consuming methods and the final `build() -> Result<_, ConfigurationError>` shape.

Implement an **error-latching builder**:

```rust
pub struct RuntimeEndpointBuilder<R> {
    state: Result<RuntimeEndpointParts<R>, RuntimeEndpointBuildFailure>,
}
```

Equivalent internal representations are acceptable, but the observable rules are fixed:

- every mutator rejects before retaining first-excess input;
- after the first error, later mutators do not format values or consume iterators;
- `build()` returns one redacted structural error;
- the rejected value is not retained;
- iterator methods consume no more than the allowed remaining count plus one first-excess item;
- construction remains free of HTTP, hooks, credentials, limiting, body materialization, body factories, streams, operation IDs, and completion.

Do not change every fluent method to return `Result<Self, _>` unless a concrete implementation constraint makes the latched design impossible and a deviation is recorded.

## 4.2 Bounded formatting

Add one internal capped formatting primitive based on `fmt::Write`.

Concord-owned output buffering must stop at a documented byte cap. An arbitrary user `Display` implementation may still do arbitrary CPU work, block, panic, or ignore formatting errors; this remains an advanced trust boundary and must be documented. Concord must not retain or grow a complete oversized formatted output.

Apply the primitive to:

- runtime endpoint path/query/header values;
- generated public-head dynamic host/path/query/header values;
- pagination-shaped dynamic query/header values;
- other public request-shaping capture discovered during implementation.

Static declaration size remains macro-validated. Dynamic capture uses core limits.

## 4.3 Canonical structural limits

Core owns and documents all hard structural limits. The exact numeric values must be selected once, justified in the closure evidence, and mechanically reflected in documentation.

At minimum define canonical owners for:

- runtime endpoint component count;
- runtime endpoint aggregate public bytes;
- runtime endpoint auth placement count;
- maximum one formatted public value;
- rate-limit ID bytes;
- rate-limit key-part count;
- key-part name bytes;
- hard aggregate key bytes;
- windows per rule;
- rules per plan.

Use public constants or one public non-instantiable limits type. Reexport them from the façade’s appropriate domain modules.

Operational limits selected by a concrete limiter may be lower than the hard structural cap, never higher. Core validates the selected limiter’s preparation limit during client construction.

## 4.4 Rate-limit constructors

Public runtime rate-limit constructors are fallible and incremental.

Target shape:

```rust
RateLimitId::new(...)
RateLimitKeyPart::new(...) -> Result<_, RateLimitError>
RateLimitKey::from_parts(...) -> Result<_, RateLimitError>
RateLimitRule::new(...) -> Result<_, RateLimitError>
RateLimitPlan::try_from_rules(...) -> Result<_, RateLimitError>
```

`RateLimitPlan::empty()` remains infallible.

An internal `from_validated_rules` may exist only after the same semantic validations have already run. Generated code must not bypass dynamic key-value limits.

Do not reveal IDs, key names, or key values in errors.

## 4.5 OAuth2 bounds

OAuth2 remains in extras and remains an ordinary `CredentialProvider`.

The provider must:

- reject oversized token URL text before parsing;
- validate client ID, secret, and scope lengths before retaining a new copy where possible;
- provide explicit setters for already-owned `SecretString` values if needed;
- calculate percent-encoded and Base64 lengths with checked arithmetic;
- compare the request body against the actual provider request limit exposed semantically by `ProviderHttp`;
- allocate each final encoded output at most once;
- avoid combined full-size intermediate strings;
- keep every build/acquisition error secret-safe.

Provider HTTP should expose a safe request-limit accessor. It must not expose native transport.

## 4.6 Endpoint identity

Remove:

```rust
impl From<&'static str> for EndpointId
```

Public runtime callers use `EndpointId::new` or `TryFrom`. Generated literals use a hidden static constructor through the generated bridge. No ordinary infallible conversion may panic.

## 4.7 Timeout naming

Retain the current semantic distinction:

- generated endpoints use their resolved DSL timeout selection and request overrides;
- runtime endpoints with `InheritClient` inherit the runtime-endpoint default in the immutable client snapshot.

Rename the public runtime builder methods and internal fields to state that scope explicitly:

```rust
runtime_endpoint_timeout(...)
disable_runtime_endpoint_timeout()
```

Do not silently change all generated endpoints to inherit a runtime configuration closure setting. Update guides, examples, rustdoc, migration text, and tests.

## 4.8 Macro semantic boundary

`emit` must import only normalized semantic/surface types.

Source grammar enums remain syntax-owned. Resolution converts them once to semantic equivalents. Add a policy test that fails if production code under `src/emit` imports `crate::syntax` or if semantic public fields use syntax AST enums.

This is a narrow normalization completion, not another macro architecture rewrite.

## 4.9 Development tooling and package graph

Remove `dangerous-dev-tools` from the façade and core public feature topology.

Preferred implementation:

1. split operation-exact benchmark helpers from deterministic test transport;
2. keep tiny benchmark helpers in a hidden, unstable core/façade bridge that has **no dependency on `concord-test-support`**;
3. make generated wrappers implement a hidden sealed conversion used by internal performance tooling without a user-facing feature;
4. let `concord-perf` depend directly on `concord-test-support` for local scripted transport;
5. keep all test-support dependencies in `[dev-dependencies]` or unpublished tooling packages;
6. delete the public feature, docs row, feature fixture, and UI tests that exist only to certify it.

The public runtime package graph must not require publishing `concord-test-support`.

## 4.10 Diagnostic policy

Use:

- stable Rust as the owner of exact Trybuild `.stderr` snapshots;
- Rust 1.85 MSRV as the owner of semantic compile-fail behavior.

Implement an MSRV harness that consumes compiler JSON diagnostics and checks:

- compilation fails;
- the expected semantic diagnostic category/message fragments are present;
- at least one expected primary source span is present;
- no internal bridge or secret text appears.

Do not maintain failing exact snapshots as an accepted final state. Do not blindly overwrite stable snapshots.

## 4.11 Publication and external decisions

Do not invent:

- a license;
- copyright ownership;
- repository/homepage URLs;
- registry ownership;
- a new public package name.

The agent must make the package graph technically verifiable and record unresolved owner decisions separately.

Keep `publish = false` until the owner approves metadata and package naming. “Not enabled for publication” is acceptable; “cannot package because of internal graph design” is not.

## 4.12 Completion language

The final report must distinguish:

- implementation complete;
- local automated validation complete;
- editor acceptance complete or pending;
- remote multi-platform CI complete or pending;
- publication enabled or externally blocked.

No phase is `Complete` when its own exit criteria are failing. Use `Blocked` or `Complete except external owner action` precisely.

---

# 5. Closure progress ledger

The agent must update this table in place after every phase.

| Phase | Status | Commit | Evidence | Remaining blockers |
|---:|---|---|---|---|
| C0 Truth reset and baseline | Complete | `f5ce4bb2ed58215055b6a986fa76d97c4951728d` | [`closure-c0-baseline.md`](docs/contributor/evidence/closure-c0-baseline.md) | None for C0; every observed baseline defect is assigned to C1–C8 |
| C1 Bounded-construction foundation | Complete | `ee2efc9cba310f2952d5c180d30ee3f08260d315` | [`closure-c1-bounded-foundation.md`](docs/contributor/evidence/closure-c1-bounded-foundation.md) | None for C1; C2–C4 must apply the shared primitives to their assigned consumers |
| C2 Runtime endpoint incremental bounds | Complete | `1fe6351b61aa8eba96f5ae63fc2859189f88b23e` | [`closure-c2-runtime-endpoint-bounds.md`](docs/contributor/evidence/closure-c2-runtime-endpoint-bounds.md) | None for C2; C3 completed the remaining public rate-limit producer bounds |
| C3 Rate-limit structural bounds | Complete | Commit containing this ledger update; exact hash will be backfilled by C4 | [`closure-c3-rate-limit-bounds.md`](docs/contributor/evidence/closure-c3-rate-limit-bounds.md) | None for C3; C4 must bound OAuth2 configuration/encoding and remaining generated/public-head formatted capture |
| C4 OAuth2 and formatted-value bounds | Not started | — | — | Owner: implementation agent — C1–C3 must complete first |
| C5 Public API and macro semantic cleanup | Not started | — | — | Owner: implementation agent — C1–C4 must complete first |
| C6 Internal tooling and package graph | Not started | — | — | Owner: implementation agent — C1–C5 must complete first |
| C7 Executable validation and truthful docs | Not started | — | — | Owner: implementation agent — C1–C6 must complete first |
| C8 Final closure certification | Not started | — | — | Owner: implementation agent — C1–C7 must complete first; external editor/platform/release checks may remain owner-blocked |

Allowed statuses:

```text
Not started
In progress
Blocked
Complete
Complete except external owner action
```

For every non-`Complete` row, state the precise blocker and owner.

---

# 6. Closure Phase C0 — Reset source truth and establish the remaining baseline

## Goal

Replace the overstated completion state with an evidence-backed closure baseline and prevent completed architecture from being accidentally reopened.

## Findings addressed

- F-04;
- inaccurate phase ledger state;
- unverifiable Git claims.

## Required tasks

1. Confirm the actual repository:
   - `git rev-parse --show-toplevel`;
   - `git branch --show-current`;
   - `git rev-parse HEAD`;
   - `git status --short`;
   - `git log --oneline --decorate -20`;
   - `git remote -v`.
2. Confirm whether the expected final refactor commit exists.
3. Copy or retain at the repository root:
   - `CONCORD_REFACTOR_AUDIT.md`;
   - `CONCORD_REFACTOR_CLOSURE_PLAN.md`;
   - `CONCORD_REFACTOR_CLOSURE_AGENT_PROMPT.md`.
4. Correct the original progress ledger without rewriting historical evidence:
   - Phase 9: incomplete because finite runtime construction is open;
   - Phase 13: incomplete because executable CI and proof accuracy are open;
   - Phase 14: blocked;
   - phases 3, 4, 10, and 12: implementation substantially complete with closure issues.
5. Add a short note to `CONCORD_REFACTOR_REFERENCE.md` directing future agents to this closure plan.
6. Create:
   - `docs/contributor/evidence/closure-c0-baseline.md`.
7. Record:
   - current tool versions;
   - current OS/target;
   - current Git state;
   - the exact failing or missing gates;
   - whether `.github/workflows/ci.yml` exists;
   - package preparation results;
   - MSRV UI results;
   - rust-analyzer availability.
8. Run focused baseline commands without changing snapshots:
   - format check;
   - stable workspace check;
   - current core/extras/façade tests;
   - current MSRV command;
   - current docs/fixtures command.
9. Do not run stable and fixture jobs concurrently on Windows when they share a target executable.
10. Do not edit product code in this phase except minimal documentation/ledger truth corrections.

## Validation

```bash
cargo fmt --all -- --check
cargo metadata --format-version 1 --all-features --locked
cargo check --workspace --all-targets --all-features --locked
cargo test -p concord-core --lib --all-features --locked
cargo test -p concord-extras --all-features --locked
cargo test -p concord --all-features --locked
cargo +1.85.0 check --workspace --all-targets --no-default-features --locked
cargo run --locked -p concord-xtask -- fixtures
cargo run --locked -p concord-xtask -- docs
```

Record failures honestly. C0 can complete with expected baseline failures because its purpose is truth capture.

## Exit criteria

- current Git/tool/source facts are recorded;
- the completion ledger no longer overstates Phases 9, 13, or 14;
- every remaining audit finding is assigned to one closure phase;
- no completed architectural surface was modified.

## Evidence and commit

Evidence:

```text
docs/contributor/evidence/closure-c0-baseline.md
```

Suggested commit:

```text
docs: establish Concord refactor closure baseline
```

---

# 7. Closure Phase C1 — Build the finite-construction foundation

## Goal

Create one reusable internal mechanism for byte/component budgets and capped formatting, and establish one canonical public owner for exact structural limits.

## Findings addressed

- F-01 foundation;
- F-02 foundation;
- F-16;
- F-18 foundation.

## Required design

Introduce narrow internal primitives rather than duplicating counting logic in each builder.

Recommended concepts:

```rust
struct StructuralBudget {
    components: usize,
    public_bytes: usize,
}

struct BoundedFormatError;

fn format_bounded(
    value: &impl Display,
    max_bytes: usize,
) -> Result<String, BoundedFormatError>;
```

Exact names and module paths are implementation details. Required properties are not.

## Required tasks

1. Inventory every public/runtime path that:
   - calls `Display::to_string`;
   - collects an arbitrary iterator;
   - appends variable-length public structure;
   - stores a runtime rate-limit object.
2. Select and document hard limits.
3. Place the canonical values in core under a semantically appropriate public module.
4. Reexport endpoint limits through `concord::endpoint`.
5. Reexport rate-limit limits through `concord::rate_limit`.
6. Implement checked arithmetic helpers:
   - addition cannot overflow;
   - replacement can subtract old retained bytes and add new bytes safely;
   - first-excess detection is explicit.
7. Implement a capped formatter:
   - never grows beyond the selected value limit;
   - returns a structural error on first excess;
   - does not retain partial protected output in a public error;
   - uses UTF-8 byte counts consistently.
8. Implement deterministic test iterators:
   - finite exact-boundary;
   - first excess;
   - infinite iterator;
   - panic if consumed beyond the first excess.
9. Add a documentation statement:
   - user `Display` work is a trust boundary;
   - Concord-owned capture is finite.
10. Extend the canonical limits reference so exact values have one owner:
    - generated from source; or
    - verified by a test/xtask parser against the source constants.
11. Do not yet refactor all consumers in one giant commit. C1 creates the foundation and tests it.

## Required tests

- exact maximum formatted value succeeds;
- first excess fails without retaining the complete input;
- checked arithmetic overflow becomes a safe structural error;
- iterator helper consumes at most `remaining + 1`;
- errored state prevents further iterator conversion;
- no sentinel value appears in `Display` or `Debug` of errors;
- public constants are reachable through the façade;
- reference limit table matches code.

## Validation

```bash
cargo test -p concord-core --lib --all-features --locked
cargo test -p concord-core --test bounds --all-features --locked
cargo test -p concord --test public_compile --no-default-features --locked
cargo run --locked -p concord-xtask -- docs
cargo rustdoc --locked -p concord --all-features -- -D warnings
```

## Exit criteria

- one bounded formatter exists;
- one checked budget mechanism exists;
- exact public structural limits have a canonical owner;
- no consumer behavior has regressed;
- all new primitive tests pass.

## Risks

- accidentally classifying a protected value as public metadata;
- retaining partial secret output in errors;
- counting characters instead of UTF-8 bytes;
- silently truncating rather than rejecting.

## Evidence and commit

Evidence:

```text
docs/contributor/evidence/closure-c1-bounded-foundation.md
```

Suggested commit:

```text
core: add canonical finite construction budgets
```

---

# 8. Closure Phase C2 — Make `RuntimeEndpointBuilder` incrementally bounded

## Goal

Ensure runtime endpoint construction rejects before excess allocation/retention and never consumes an unbounded iterator.

## Findings addressed

- F-01;
- runtime portion of F-16;
- runtime portion of F-18.

## Required tasks

### C2.1 Builder state

1. Replace the raw fields-only builder with an error-latching state.
2. Preserve response type transitions without losing a prior failure.
3. Ensure every mutator checks for an existing failure before:
   - invoking `Display`;
   - invoking `IntoIterator::into_iter`;
   - cloning names;
   - appending auth/rate structures.
4. `build()` returns the stored structural category without protected details.

### C2.2 Path and query

1. `path_segment`:
   - format through the bounded formatter;
   - reserve component and aggregate bytes before push.
2. `query`:
   - validate/count name bytes before copying;
   - format value through the bounded formatter;
   - reserve one component and both name/value bytes before push.
3. `query_optional`:
   - `None` performs no budget work;
   - `Some` follows `query`.
4. `query_values`:
   - validate/copy the name once;
   - consume only until first excess;
   - preserve iterator order;
   - stop immediately on component, name/value, or aggregate-byte excess;
   - do not inspect the iterator tail.

### C2.3 Headers

1. Count header-name and retained header-value bytes.
2. Replacement is not a new component:
   - subtract the old retained value;
   - validate the new bounded value;
   - commit only after the new total is valid.
3. Reserved/public header validation remains centrally owned.
4. An oversized replacement must not retain the oversized replacement.

### C2.4 Authentication and rate limits

1. Add safe internal structural introspection for `Auth`:
   - number of placements;
   - public placement-name bytes;
   - no credential material.
2. Check auth count and public bytes before push.
3. Accept only already structurally validated `RateLimitRule` values from C3.
4. Check runtime rule count and public rule-shape bytes before push.
5. Do not count secret key values as safe description text; count them for memory budgets without rendering them in errors.

### C2.5 Body and response

1. Keep body input inert.
2. Do not call body metadata, encoders, factories, or streams during builder mutation/build.
3. Document that arbitrary application body input state is an extension trust boundary.
4. Response selection must preserve latched failures.

### C2.6 Build

1. `build()` performs only validations that genuinely require the completed structure.
2. It must not be the first place that discovers:
   - component excess;
   - aggregate byte excess;
   - auth placement excess;
   - rate-rule excess.
3. Preserve immutable boxes/Arcs and safe descriptions.
4. Preserve `RuntimeEndpoint` request and planning semantics.

## Required tests

Add deterministic tests with explicit names equivalent to:

```text
runtime_endpoint_query_values_stops_at_first_excess
runtime_endpoint_infinite_query_values_returns_configuration_error
runtime_endpoint_first_excess_component_is_not_retained
runtime_endpoint_first_excess_public_byte_is_not_retained
runtime_endpoint_errored_builder_does_not_consume_later_iterator
runtime_endpoint_header_replacement_updates_budget_exactly
runtime_endpoint_auth_first_excess_is_rejected_before_push
runtime_endpoint_rate_rule_first_excess_is_rejected_before_push
runtime_endpoint_bounded_format_does_not_retain_sentinel
runtime_endpoint_build_remains_inert
runtime_endpoint_plan_remains_zero_effect
```

Use a sentinel iterator whose `next()` panics beyond the required first-excess element.

Re-run existing:

- runtime endpoint lifecycle;
- endpoint planning;
- redaction;
- completion;
- authentication ordering;
- limiter ordering.

## Validation

```bash
cargo test -p concord-core --lib --all-features --locked
cargo test -p concord-core --test bounds --all-features --locked
cargo test -p concord-core --test lifecycle --all-features --locked
cargo test -p concord-core --test security --all-features --locked
cargo test -p concord --test endpoint_planning --all-features --locked
cargo test -p concord --test runtime_endpoint_lifecycle --all-features --locked
cargo test -p concord --all-features --locked
cargo bench -p concord-perf --bench concord_overhead --no-run --locked
```

## Exit criteria

- no runtime endpoint iterator is fully collected before validation;
- an infinite `query_values` source returns a bounded structural failure;
- first excess is detected and not retained;
- later builder calls after failure perform no user formatting/iteration;
- safe planning and execution remain behaviorally identical after successful construction;
- all errors remain redacted.

## Risks

- consuming two excess elements instead of one;
- header replacement double-counting;
- type-state response transitions dropping the stored error;
- adding side effects to `build()`.

## Evidence and commit

Evidence:

```text
docs/contributor/evidence/closure-c2-runtime-endpoint-bounds.md
```

Suggested commit:

```text
core: bound runtime endpoint construction incrementally
```

---

# 9. Closure Phase C3 — Bound rate-limit semantics and governor preparation

## Goal

Make every public rate-limit semantic value finite before a limiter sees it.

## Findings addressed

- F-02;
- rate-limit portion of F-16;
- governor portion of F-01/F-18.

## Required tasks

### C3.1 Hard structural contracts

1. Add hard limits for:
   - ID bytes;
   - key-part count;
   - key-part name bytes;
   - hard aggregate key bytes;
   - windows per rule;
   - rules per plan.
2. Keep `DEFAULT_RATE_LIMIT_KEY_BYTES` as the normal operational default.
3. Require every selected limiter preparation-key limit to be:
   - zero or positive according to existing global-key semantics;
   - no greater than the hard aggregate cap.
4. Reject an invalid limiter configuration during client construction.

### C3.2 IDs and key parts

1. `RateLimitId::new`:
   - validate nonempty and maximum bytes before owning a copy where possible.
2. `RateLimitKeyPart::new`:
   - become fallible;
   - validate nonempty name;
   - validate name/value structural byte caps;
   - retain no value in errors.
3. Preserve no automatic `Debug`/`Display` for key-containing types.

### C3.3 Keys

1. Make `RateLimitKey::from_parts` incremental.
2. Stop at the first excess part.
3. Detect duplicate names without consuming the tail.
4. Use checked aggregate byte arithmetic.
5. Preserve declaration order.
6. Test global key separately.

### C3.4 Rules

1. Make window collection incremental.
2. Reject empty windows.
3. Stop at first excess window.
4. Preserve cost ≤ every capacity.
5. Keep IDs and keys immutable.

### C3.5 Plans

1. Replace the public infallible `RateLimitPlan::from_rules` with a fallible canonical constructor.
2. Validate:
   - rule count;
   - duplicate rule IDs;
   - total structural budget if selected;
   - each rule already validated.
3. Keep `empty()` infallible.
4. An internal unchecked finalizer is permitted only when all public/generated push methods have already run the same validations.

### C3.6 Generated head construction

1. Change `HeadBuilder::push_rate_limit_rule` and generated-rule methods to:
   - construct key parts with fallible validation;
   - consume windows incrementally;
   - enforce rule count before push;
   - return the existing safe `ApiError` category.
2. Do not use `.collect::<Vec<_>>()` on a caller-provided iterator before limits.
3. Keep macro resolver static validation aligned with runtime hard limits.
4. Add compile-fail diagnostics for declarations exceeding static rule/window limits if not already present.

### C3.7 Runtime endpoint and governor

1. Integrate C3 types into C2 runtime endpoint accounting.
2. Ensure governor preparation cannot multiply an unbounded plan.
3. Test the maximum valid plan through:
   - core plan validation;
   - façade runtime endpoint;
   - extras governor.
4. Test first excess at every layer.
5. Preserve fail-closed behavior when a nonempty plan has no limiter.

## Required tests

At minimum:

```text
rate_limit_id_max_and_first_excess
rate_limit_key_part_max_and_first_excess
rate_limit_key_iterator_stops_at_first_excess
rate_limit_key_duplicate_stops_without_tail_consumption
rate_limit_rule_window_iterator_stops_at_first_excess
rate_limit_plan_rule_iterator_stops_at_first_excess
rate_limit_plan_rejects_duplicate_ids
rate_limiter_rejects_preparation_limit_above_hard_cap
governor_accepts_maximum_valid_plan
governor_rejects_first_excess_without_sensitive_output
generated_rate_limits_obey_same_structural_limits
runtime_endpoint_rate_limits_obey_same_structural_limits
```

Every error rendering test must use sentinel IDs/key values and prove absence.

## Validation

```bash
cargo test -p concord-core --lib --all-features --locked
cargo test -p concord-core --test bounds --all-features --locked
cargo test -p concord-core --test lifecycle --all-features --locked
cargo test -p concord-extras --all-features --locked
cargo test -p concord-extras --test governor --all-features --locked
cargo test -p concord --test runtime_endpoint_lifecycle --all-features --locked
cargo test -p concord --test generated_surface --all-features --locked
cargo test -p concord-examples --all-features --locked
```

## Exit criteria

- semantic plan construction is hard-bounded independently of a limiter;
- no rate-limit iterator is fully collected before validation;
- the limiter operational cap cannot exceed the core hard cap;
- governor work is bounded by plan structure;
- generated and runtime plans use the same semantic validation;
- no rate key content leaks.

## Risks

- breaking many internal call sites after `RateLimitKeyPart::new` becomes fallible;
- accidentally exposing key values in a new error variant;
- allowing generated code to use an unchecked path with dynamic values;
- mismatch between macro static caps and core runtime caps.

## Evidence and commit

Evidence:

```text
docs/contributor/evidence/closure-c3-rate-limit-bounds.md
```

Suggested commit:

```text
core: enforce finite rate limit structures
```

---

# 10. Closure Phase C4 — Bound OAuth2 and all Concord-owned formatted capture

## Goal

Finish finite first-party extension behavior and close the broader formatted-value decision.

## Findings addressed

- F-08;
- F-18.

## Required tasks

### C4.1 Provider HTTP semantic limit

1. Add a safe accessor on `ProviderHttp` for the active request-body byte limit.
2. The accessor exposes only a finite number, not native client state.
3. Update public docs for `CredentialRequest` and `ProviderHttp`.

### C4.2 OAuth2 builder inputs

1. Add exact public limits for:
   - token URL text;
   - client ID;
   - client secret;
   - scope;
   - final Authorization header if separately normative.
2. Validate token URL byte length before `Url::parse`.
3. Prefer setters that can validate borrowed text before allocation:
   - borrowed/plain text setter;
   - separately named owned-secret setter if needed.
4. Do not keep an oversized `SecretString` or scope in a failed builder.
5. Add safe build error categories for oversized fields without exposing:
   - URL text;
   - client ID;
   - client secret;
   - scope.
6. Preserve existing missing-field and unsafe-URL categories.

### C4.3 OAuth2 encoding

1. Compute form-encoded lengths with checked arithmetic.
2. Compute Basic credential source length and Base64 output length before allocation.
3. Compare:
   - body output against `context.http.request_body_limit()`;
   - header output against the selected hard header cap.
4. Allocate the final Authorization value once.
5. Allocate the final form body once.
6. Do not create:
   - full encoded username string;
   - full encoded password string;
   - combined `username:password` string;
   - Base64 string;
   - formatted `Basic ...` string;
   as four simultaneously retained full-size intermediates.
7. Preserve exact protocol behavior and token-response parsing.
8. Keep acquisition errors reduced to safe categories.

### C4.4 Generated/public-head formatting

Apply the C1 bounded formatter to dynamic request shaping in `HeadBuilder`:

- host labels;
- safe/redacted host labels;
- path segments;
- query values;
- pagination query values;
- public headers;
- pagination-sensitive headers.

Rules:

- generated declaration structure is statically bounded by the macro;
- every one dynamic formatted value has a hard capture cap;
- existing URL/header grammar validation remains;
- first-excess output is rejected, not truncated;
- safe values and protected values map to the existing redaction policy;
- user `Display` CPU behavior remains explicitly outside Concord’s automatic bound.

Search the entire product source for remaining ordinary `Display::to_string()` request-shaping paths and either:
- convert them; or
- record why they are not Concord-owned capture.

## Required tests

OAuth2:

```text
oauth_token_url_max_and_first_excess
oauth_client_id_max_and_first_excess
oauth_client_secret_max_and_first_excess
oauth_scope_max_and_first_excess
oauth_encoded_authorization_checked_before_allocation
oauth_form_body_checked_against_provider_limit
oauth_boundary_request_is_protocol_correct
oauth_first_excess_errors_are_secret_safe
oauth_debug_and_display_remain_redacted
```

Head shaping:

```text
generated_path_formatted_value_first_excess
generated_query_formatted_value_first_excess
generated_header_formatted_value_first_excess
pagination_formatted_value_first_excess
bounded_formatter_does_not_retain_protected_sentinel
```

Re-run all auth/provider, request-head, planning, redaction, and pagination tests.

## Validation

```bash
cargo test -p concord-core --lib --all-features --locked
cargo test -p concord-core --test bounds --all-features --locked
cargo test -p concord-core --test security --all-features --locked
cargo test -p concord-core --test lifecycle --all-features --locked
cargo test -p concord-extras --all-features --locked
cargo test -p concord --test generated_authentication --all-features --locked
cargo test -p concord --test endpoint_planning --all-features --locked
cargo test -p concord --test generated_lifecycle --all-features --locked
```

## Exit criteria

- OAuth2 retains no unbounded first-party configuration;
- encoded lengths are checked before output allocation;
- provider request limit is available semantically;
- all Concord-owned request-shaping formatting is capped or consciously documented;
- no secret or protected dynamic value appears in errors.

## Risks

- changing OAuth percent-encoding behavior;
- off-by-one Base64 length calculations;
- exposing provider limits as transport internals rather than semantic bounds;
- accidentally capping safe redacted placeholders differently from logical values.

## Evidence and commit

Evidence:

```text
docs/contributor/evidence/closure-c4-oauth-and-format-bounds.md
```

Suggested commit:

```text
extras: bound OAuth2 configuration and encoding
```

If generated head formatting is large enough to deserve a separate commit, use two commits within C4 and record both.

---

# 11. Closure Phase C5 — Finish public API and macro semantic boundaries

## Goal

Close the smaller API correctness and contributor-architecture gaps without changing the accepted product model.

## Findings addressed

- F-09;
- F-10;
- F-11;
- F-17;
- documentation part of F-16.

## Required tasks

### C5.1 Endpoint identity

1. Remove `From<&'static str> for EndpointId`.
2. Add/retain:
   - `EndpointId::new`;
   - `TryFrom<String>`;
   - `TryFrom<&str>` if useful and unambiguous.
3. Move generated static construction to a hidden bridge function or unmistakably hidden constructor.
4. Ensure generated code still obtains allocation-free static IDs.
5. Add compile tests proving:
   - invalid `.into()` is unavailable;
   - fallible public construction works;
   - generated declarations still compile.

### C5.2 Timeout naming

1. Rename:
   - `RuntimeConfigBuilder::operation_timeout` → `runtime_endpoint_timeout`;
   - `disable_operation_timeout` → `disable_runtime_endpoint_timeout`;
   - internal field/accessor names accordingly.
2. Update macro-generated runtime baseline calls.
3. Preserve:
   - generated DSL timeout behavior;
   - per-request timeout override;
   - runtime endpoint `InheritClient`, `Explicit`, `Disabled`.
4. Update:
   - guide 03;
   - production configuration recipe;
   - rustdoc;
   - examples;
   - migration/changelog if the old method was documented.
5. Add tests showing generated endpoints are not silently affected by runtime-only closure configuration unless their resolved contract says so.

### C5.3 Macro normalization

1. Define semantic equivalents for every source AST enum used by emit:
   - method;
   - challenge policy;
   - auth placement;
   - defaults;
   - retry selection;
   - value kind;
   - any other syntax-owned form found by source search.
2. Convert exactly once in semantic resolution.
3. Make the surface model own all emitted names/capabilities.
4. Remove production imports from `emit` to `syntax`.
5. Add policy tests:
   - no `crate::syntax` import under production `src/emit`;
   - semantic public structs do not expose syntax AST types.
6. Keep parser and UI behavior unchanged.
7. Run a normalized-model snapshot or equivalence test before/after.

### C5.4 Residual bridge cleanup

1. Inventory hidden core-generated bridge macros/helpers.
2. Remove copies that cannot be reached after façade-only generation.
3. Retain only helpers with a documented semantic owner.
4. Ensure every generated feature diagnostic names façade features, never `concord_core/...`.
5. Add a source/expansion policy test for implementation-crate feature names.

### C5.5 Limits documentation

1. Expose new endpoint/rate/OAuth limits through rustdoc.
2. Make `docs/reference/limits.md` mechanically match code.
3. Avoid exact values existing only in closure evidence.

## Validation

```bash
cargo test -p concord-macros --lib --locked
cargo test -p concord --test generated_surface --all-features --locked
cargo test -p concord --test public_compile --no-default-features --locked
cargo test -p concord --test ui --no-default-features --locked
cargo test -p concord --test ui --all-features --locked
cargo test -p concord-core --test ui --no-default-features --locked
cargo test -p concord-core --test public_types --all-features --locked
cargo rustdoc --locked -p concord --all-features -- -D warnings
cargo run --locked -p concord-xtask -- docs
```

Inspect every Trybuild change manually. Do not blanket-run `TRYBUILD=overwrite`.

## Exit criteria

- no panic-prone public endpoint ID conversion;
- timeout API states its actual scope;
- emit is grammar-independent;
- obsolete direct-core bridge residue is removed;
- generated diagnostics and exact limits use façade/public terminology.

## Risks

- accidentally changing DSL diagnostics while normalizing enums;
- losing source spans during semantic conversion;
- changing endpoint ID generation;
- removing a hidden bridge still used by a feature fixture.

## Evidence and commit

Evidence:

```text
docs/contributor/evidence/closure-c5-api-macro-cleanup.md
```

Suggested commit:

```text
refactor: finish Concord semantic API boundaries
```

---

# 12. Closure Phase C6 — Remove test-support leakage and make packaging technically coherent

## Goal

Ensure public product packages do not depend normally on internal test support and can be package-verified in intended publication order.

## Findings addressed

- F-05;
- F-12;
- package portion of F-04;
- tooling portion of F-17.

## Required tasks

### C6.1 Remove the public feature

Delete:

- `concord-core/dangerous-dev-tools`;
- `concord/dangerous-dev-tools`;
- feature forwarding;
- user feature documentation;
- feature fixtures and UI tests whose only purpose is this feature;
- generated development-method feature assertions.

Do not remove operation-exact performance coverage.

### C6.2 Split benchmark and test-support ownership

1. Move benchmark-only production-operation helpers to a hidden unstable bridge that:
   - depends only on core;
   - does not reexport test-support types;
   - performs no alternate transport.
2. Add a hidden sealed conversion from named generated request wrappers to the internal request carrier for workspace performance tooling.
3. Keep the hidden bridge out of normal user rustdoc.
4. `concord-perf` may import:
   - hidden façade/core benchmark helpers;
   - `concord-test-support` directly for scripted local transport.
5. Core normal dependencies must not include `concord-test-support`.
6. All test-support use in core/façade must be dev-only or test cfg.
7. Preserve benchmark categories and rerun measurements.

### C6.3 Manifest and dependency policy

1. Update package manifests and feature tables.
2. Update dependency-policy tests to assert:
   - no normal product dependency on test support;
   - examples still depend only on façade;
   - macros remain runtime/extras independent.
3. Update fixture metadata and lockfile.
4. Keep `concord-test-support` unpublished/internal.

### C6.4 Technical package verification

1. Add an `xtask package` or equivalent script that verifies packaging with local sibling patches/source replacement.
2. Verify intended order:
   - `concord-core`;
   - `concord-macros`;
   - `concord-extras`;
   - `concord`.
3. Ensure packaged manifests contain registry-version dependencies, not workspace path-only requirements.
4. Run package verification without requiring publication of test support.
5. Retain `publish = false` until owner metadata/name decisions are approved.
6. Do not claim registry publication readiness if package name/legal metadata remains unresolved.

## Validation

```bash
cargo metadata --format-version 1 --all-features --locked
cargo test -p concord --test dependency_policy --all-features --locked
cargo test -p concord-core --all-features --locked
cargo test -p concord --all-features --locked
cargo bench -p concord-perf --bench concord_overhead --no-run --locked
cargo bench -p concord-perf --bench concord_overhead -- --smoke
cargo run --locked -p concord-xtask -- fixtures
cargo run --locked -p concord-xtask -- package
```

Also attempt direct package commands with the local package-verification configuration:

```bash
cargo package --locked -p concord-core
cargo package --locked -p concord-macros
cargo package --locked -p concord-extras
cargo package --locked -p concord
```

If ordinary commands cannot resolve unpublished siblings, the xtask/local-source verification must prove the exact packaged crates. Record the distinction.

## Exit criteria

- no public product feature named `dangerous-dev-tools`;
- no normal core/façade dependency on test support;
- performance probes remain usable;
- product packages are package-verifiable in principle;
- unresolved registry name/license decisions are external blockers only.

## Risks

- performance helpers accidentally becoming a second public execution API;
- hiding benchmark support behind another public feature under a different name;
- package verification accidentally testing workspace paths rather than packaged manifests;
- removing deterministic development transport used by a genuine runtime test.

## Evidence and commit

Evidence:

```text
docs/contributor/evidence/closure-c6-package-graph.md
```

Suggested commit:

```text
build: remove test support from Concord product graph
```

---

# 13. Closure Phase C7 — Add executable CI, passing MSRV proof, and truthful validation/docs

## Goal

Make validation ownership executable and make every documentation/evidence claim match what is actually checked.

## Findings addressed

- F-03;
- F-06;
- F-07;
- F-13;
- F-14;
- F-15;
- validation/documentation part of F-04.

## Required tasks

### C7.1 GitHub Actions workflow

Add:

```text
.github/workflows/ci.yml
```

Required characteristics:

- triggers for pull requests, pushes, and manual dispatch;
- least-privilege permissions;
- action versions pinned to immutable revisions;
- Cargo cache with package/toolchain-sensitive keys;
- no concurrent processes sharing the same Windows target executable;
- explicit timeouts;
- `--locked` correctness commands;
- artifacts/logs retained for failed UI/package jobs.

Required logical lanes:

1. format and metadata;
2. stable default/no-default/all-features check;
3. stable Clippy;
4. stable unit/integration tests;
5. lifecycle/security/bounds;
6. macro and stable exact UI;
7. MSRV compilation and semantic UI;
8. extras;
9. fixtures/public compile;
10. examples and docs;
11. dependency policy and `cargo deny`;
12. package verification;
13. platform smoke matrix: Linux, Windows, macOS;
14. performance compile/smoke as non-threshold observational lane.

Jobs may combine compatible work, but every owner must be visible.

Add `actionlint` in CI and a repository policy check that verifies the workflow and required job IDs exist.

### C7.2 Just/xtask scopes

Replace the overloaded final gate with explicit scopes:

```text
just verify-local
just verify-msrv
just verify-package
just verify-release-local
```

`verify-local` must be entirely reproducible and `--locked`.

Document that it cannot prove:

- remote GitHub execution;
- other operating systems;
- interactive editor behavior;
- registry ownership/legal approval.

Add `--locked` to correctness recipes. Formatting mutation commands need not use it.

### C7.3 MSRV diagnostic harness

1. Stable Trybuild remains exact.
2. Add a semantic MSRV harness using compiler JSON.
3. Reuse source fixtures where possible.
4. Store expected semantic fragments separately from unstable rendering.
5. The harness must assert:
   - expected failure;
   - expected user-language message;
   - expected primary span/file;
   - absence of bridge internals and sentinels.
6. Add it to:
   - `just test-msrv`;
   - CI MSRV lane;
   - final closure gate.
7. Delete the practice of reporting known red MSRV snapshots as acceptable completion.

### C7.4 Rust-analyzer acceptance

Automated supplementary proof:

- run `rust-analyzer analysis-stats` or equivalent on the root workspace;
- capture diagnostics;
- verify fixture workspace does not pollute root diagnostics.

Manual acceptance protocol:

1. completion after a direct endpoint call;
2. hover shows named request wrapper;
3. optional setters and terminals appear without imports;
4. non-paginated endpoint does not offer `paginate`;
5. go-to-definition reaches declaration or stable generated item;
6. renamed façade dependency works;
7. feature-disabled methods are absent;
8. diagnostics point to DSL spans;
9. fixture workspaces do not pollute root analysis.

If the agent environment has no editor, create a filled evidence template containing automated results and mark the manual rows `Pending external editor session`. Do not mark them passed.

### C7.5 Documentation and evidence truth

1. Correct Phase 13/14 evidence claims.
2. Include the evidence directory in link checking.
3. Validate Markdown anchors, or clearly separate:
   - local file checks;
   - anchor checks;
   - external URL checks.
4. Fix all known broken evidence links.
5. Correct PowerShell wording:
   - the justfile may select PowerShell on Windows;
   - recipes themselves must remain portable.
6. Make xtask recipe checks execute or parse their real command ownership rather than only checking names.
7. Update README before the registry snippet:
   - state that packages are not currently published;
   - show a truthful checkout/path development form;
   - label registry syntax as intended future release syntax.
8. Update feature docs after C6.
9. Update limits docs after C1–C4.
10. Keep external URL checks network-optional and report their scope accurately.

### C7.6 Worktree/release truth checks

Add a CI release-policy job that runs after generation/docs commands and checks:

```bash
git diff --check
git diff --exit-code
test -z "$(git status --porcelain)"
```

Do not put the clean-worktree assertion in an ordinary developer command that is expected to run with intentional changes.

## Required local validation

```bash
cargo fmt --all -- --check
cargo metadata --format-version 1 --all-features --locked
cargo check --workspace --all-targets --locked
cargo check --workspace --all-targets --no-default-features --locked
cargo check --workspace --all-targets --all-features --locked
cargo +stable clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --locked
cargo test --workspace --all-features --locked
cargo +1.85.0 check --workspace --all-targets --no-default-features --locked
cargo +1.85.0 test -p concord-core --lib --no-default-features --locked
cargo +1.85.0 test -p concord-extras --all-features --locked
cargo +1.85.0 test -p concord --test ui_msrv_semantic --no-default-features --locked
cargo +1.85.0 test -p concord-core --test ui_msrv_semantic --no-default-features --locked
cargo run --locked -p concord-xtask -- fixtures
cargo run --locked -p concord-xtask -- docs
cargo run --locked -p concord-xtask -- package
cargo test --workspace --doc --locked
cargo test --workspace --doc --all-features --locked
cargo rustdoc --locked -p concord --all-features -- -D warnings
cargo rustdoc --locked -p concord-core --all-features -- -D warnings
cargo rustdoc --locked -p concord-extras --all-features -- -D warnings
cargo deny check
actionlint .github/workflows/ci.yml
```

Run the commands sequentially on Windows if target-file contention is possible.

## Exit criteria

- workflow exists and is linted;
- all local automated lanes pass;
- stable exact UI passes;
- MSRV semantic UI passes;
- docs/evidence claims match actual checker scope;
- README installation status is truthful;
- rust-analyzer manual status is recorded honestly;
- remote platform CI is either passed or explicitly pending because no push was authorized.

## Risks

- hiding real diagnostics behind over-broad message substrings;
- action workflows depending on unpinned moving tags;
- CI command duplication drifting from Just/xtask;
- claiming editor behavior from command-line analysis only.

## Evidence and commit

Evidence:

```text
docs/contributor/evidence/closure-c7-validation-ci.md
docs/contributor/evidence/rust-analyzer-acceptance.md
```

Suggested commit:

```text
ci: make Concord closure validation executable
```

---

# 14. Closure Phase C8 — Final certification and honest handoff

## Goal

Run the complete proof matrix, verify repository truth, and publish a precise completion or blocker statement.

## Findings addressed

- F-04;
- every remaining audit finding.

## Required tasks

1. Re-read:
   - audit findings;
   - all closure evidence;
   - progress ledger;
   - deviation records.
2. Confirm every audit finding has one of:
   - fixed and proven;
   - externally blocked with owner/action;
   - consciously deferred with approved rationale.
3. Run `just verify-release-local` from a clean tree.
4. Run performance measurements and compare with Phase 14 baseline:
   - endpoint construction;
   - request binding;
   - safe planning;
   - public-head preparation;
   - direct/explicit/runtime execution;
   - credential ready path;
   - limiter ready path;
   - pagination;
   - record framing;
   - expansion 1/10/100/1,000.
5. Investigate material regressions. Do not invent thresholds after seeing results. Record observational changes and causes.
6. Verify package inventories and package-local verification.
7. Verify migration/changelog mention new closure changes:
   - rate-limit constructor fallibility;
   - endpoint ID conversion;
   - timeout rename;
   - removed dev-tools feature;
   - new exact limits.
8. Run source-policy checks:
   - no normal product dependency on test support;
   - no macro extras names;
   - no emit syntax imports;
   - no direct generated core paths;
   - no user-facing implementation feature names.
9. In the real Git repository:
   - check status;
   - check diff whitespace;
   - commit the phase;
   - rerun clean-tree checks;
   - record exact commit.
10. Do not push.
11. Update the closure ledger.
12. Produce:
   - `docs/contributor/evidence/closure-c8-final.md`;
   - a concise final status in `CHANGELOG.md` or contributor release documentation.

## Complete local proof matrix

```bash
just verify-release-local

cargo fmt --all -- --check
cargo metadata --format-version 1 --all-features --locked

cargo check --workspace --all-targets --locked
cargo check --workspace --all-targets --no-default-features --locked
cargo check --workspace --all-targets --all-features --locked
cargo +stable clippy --workspace --all-targets --all-features --locked -- -D warnings

cargo test --workspace --locked
cargo test --workspace --all-features --locked

cargo test -p concord-core --test lifecycle --all-features --locked
cargo test -p concord-core --test security --all-features --locked
cargo test -p concord-core --test bounds --all-features --locked
cargo test -p concord --test generated_surface --all-features --locked
cargo test -p concord --test public_extensions --all-features --locked
cargo test -p concord --test endpoint_planning --all-features --locked
cargo test -p concord --test runtime_endpoint_lifecycle --all-features --locked
cargo test -p concord --test public_compile --no-default-features --locked
cargo test -p concord --test rustdoc --all-features --locked

cargo test -p concord-macros --lib --locked
cargo test -p concord --test ui --no-default-features --locked
cargo test -p concord --test ui --all-features --locked
cargo test -p concord-core --test ui --no-default-features --locked

cargo +1.85.0 check --workspace --all-targets --no-default-features --locked
cargo +1.85.0 test -p concord-core --lib --no-default-features --locked
cargo +1.85.0 test -p concord-extras --all-features --locked
cargo +1.85.0 test -p concord --test ui_msrv_semantic --no-default-features --locked
cargo +1.85.0 test -p concord-core --test ui_msrv_semantic --no-default-features --locked

cargo run --locked -p concord-xtask -- fixtures
cargo run --locked -p concord-xtask -- docs
cargo run --locked -p concord-xtask -- package

cargo test -p concord-examples --all-features --locked
cargo test --workspace --doc --locked
cargo test --workspace --doc --all-features --locked
cargo rustdoc --locked -p concord --all-features -- -D warnings
cargo rustdoc --locked -p concord-core --all-features -- -D warnings
cargo rustdoc --locked -p concord-extras --all-features -- -D warnings

cargo deny check
actionlint .github/workflows/ci.yml

cargo bench -p concord-perf --bench concord_overhead --no-run --locked
cargo bench -p concord-perf --bench concord_overhead -- --smoke
cargo run --locked -p concord-perf --bin expansion_scale

git diff --check
git status --short
```

After the final commit:

```bash
git diff --exit-code
test -z "$(git status --porcelain)"
git rev-parse HEAD
```

## Final classification rules

### “Refactor closure complete”

Allowed only when:

- C1–C7 exit criteria pass;
- local proof matrix passes;
- no product invariant blocker remains;
- all source/evidence claims are accurate;
- manual rust-analyzer protocol has passed;
- required remote platform CI has passed;
- package graph is technically verifiable.

### “Implementation and local validation complete; external release checks pending”

Use when code and all local automated proofs pass, but one or more of these requires external action:

- GitHub Actions cannot run because pushing is not authorized;
- interactive editor session is unavailable;
- legal/license metadata is not approved;
- public package name/registry ownership is unresolved.

List each external blocker, owner, and exact next action.

### “Blocked”

Use when any finite-behavior, security, lifecycle, compile-time surface, local test, package-graph, or documentation-truth criterion remains failing.

## Evidence and commit

Evidence:

```text
docs/contributor/evidence/closure-c8-final.md
```

Suggested commit:

```text
release: close remaining Concord v2 refactor gaps
```

---

# 15. Required deviation process

A deviation is required when the implementation cannot follow a binding decision.

Create:

```text
docs/contributor/evidence/deviations/closure-<number>-<slug>.md
```

Every deviation contains:

1. affected closure phase and audit finding;
2. original required behavior;
3. observed implementation constraint;
4. alternatives considered;
5. selected behavior;
6. invariant impact;
7. public API impact;
8. migration impact;
9. exact tests;
10. approval still required, if any.

A deviation cannot waive:

- finite runtime construction;
- redaction;
- one executor;
- transport/auth privacy;
- deterministic completion;
- bounded recovery;
- sequential/all-or-error pagination;
- truthful validation evidence.

---

# 16. Test design rules for closure work

1. Test the first excess, not merely a large completed value.
2. Use panic-after-limit iterators to prove no tail consumption.
3. Verify exact maximum success and maximum-plus-one failure.
4. Verify failed builders do not inspect later inputs.
5. Verify errors do not contain dynamic sentinel values.
6. Keep external network access out of deterministic tests.
7. Use the lowest test layer that proves the contract.
8. Do not add independent Cargo packages unless the assertion depends on Cargo resolution.
9. Do not recreate example APIs.
10. Stable exact diagnostic snapshots and MSRV semantic diagnostics have different owners.
11. Do not update Trybuild snapshots without inspecting every diff.
12. Do not rely on elapsed sleeps for lifecycle ordering.
13. Keep Windows fixture/package commands serialized or isolated by target directory.
14. Performance results are observational unless a pre-existing threshold exists.

---

# 17. Documentation ownership after closure

| Fact | Canonical owner |
|---|---|
| Product/crate architecture | `docs/contributor/architecture.md` |
| Public DSL | `docs/reference/dsl.md` |
| Generated Rust mapping | `docs/reference/generated-api.md` |
| Runtime endpoint limits | public core constants + `docs/reference/limits.md` mechanical check |
| Rate-limit structural limits | public core constants + `docs/reference/limits.md` mechanical check |
| OAuth2 limits | extras rustdoc + `docs/reference/limits.md` mechanical check |
| User feature topology | `crates/concord/Cargo.toml` + generated/checked `docs/reference/features.md` |
| Local validation commands | `justfile` and `xtask`; docs link rather than duplicate |
| CI job ownership | `.github/workflows/ci.yml` |
| MSRV diagnostic policy | `docs/contributor/testing.md` |
| Rust-analyzer protocol | `docs/contributor/testing.md` and acceptance evidence |
| Publication blockers | `docs/contributor/release.md` |
| Historical phase proof | `docs/contributor/evidence/` |

No exact limit, feature, command, or package relationship should be normative only in historical evidence.

---

# 18. External owner decisions that the agent must not invent

These may remain after technical closure:

1. approved SPDX license or license-file;
2. repository/homepage/documentation URLs;
3. registry account/ownership;
4. final Cargo package name if `concord` is unavailable;
5. permission to push and run GitHub Actions;
6. access to a graphical editor for manual rust-analyzer acceptance.

For each unresolved item, the final evidence must state:

```text
Decision:
Current technical state:
Owner:
Required action:
What is blocked:
What is not blocked:
```

Do not use an unresolved external decision to leave a technical package-graph defect unfixed.

---

# 19. Final definition of done

The remaining refactor work is technically complete when all of the following are true:

1. runtime endpoint methods reject incrementally and do not overconsume iterators;
2. rate-limit IDs, keys, windows, rules, and plans are hard-bounded in core;
3. governor preparation is bounded by validated semantic structures;
4. OAuth2 validates configuration and encoded output before unbounded allocation;
5. Concord-owned formatted capture stops at finite limits;
6. exact structural limits have one public/canonical owner;
7. public endpoint ID conversion cannot panic through `From`;
8. runtime-only timeout APIs are named truthfully;
9. macro emit depends only on semantic/surface types;
10. obsolete direct-core bridge residue is removed;
11. no normal public product dependency points to test support;
12. `dangerous-dev-tools` is absent from the user feature topology;
13. product packages are technically package-verifiable;
14. an executable, linted CI workflow exists;
15. stable exact UI and MSRV semantic UI each pass;
16. local correctness commands are locked and accurately scoped;
17. documentation/evidence links and claims match actual checks;
18. README installation status is truthful;
19. all original lifecycle, security, redaction, pagination, completion, and transport-isolation tests remain green;
20. the final project statement distinguishes local completion from external release/editor/platform blockers.

Runtime-defined pagination remains the approved bounded deferral and is not part of this closure.

---

# 20. Agent handoff summary template

At every phase boundary report:

```text
Phase:
Commit:
Files changed:
Audit findings closed:
Public API changes:
Invariant impact:
Tests added:
Commands run:
Results:
Unrun checks:
Deviations:
Next phase:
Worktree status:
```

The final report must include:

- exact HEAD;
- exact branch;
- clean/dirty status;
- all phase commits;
- all passing local gates;
- all failed or unexecuted gates;
- performance comparison;
- external blockers;
- explicit statement that nothing was pushed, if true.
