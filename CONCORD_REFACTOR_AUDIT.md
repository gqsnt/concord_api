# Concord architectural refactor audit

**Reviewed input:** `concord_api(65).zip`
**Reference:** `CONCORD_REFACTOR_REFERENCE.md` supplied for the refactor
**Comparison baseline:** `concord_api(64).zip`
**Review type:** static source, manifests, tests, fixtures, examples, documentation, tooling, and recorded evidence
**Implementation changes made during this audit:** none

---

## 1. Executive verdict

The refactor is a substantial architectural success, but the statement **“the full Concord architectural refactor is complete” is not accurate under the refactor document's own definition of done**.

The intended product architecture is visibly implemented:

```text
concord ───────────► concord-core
   ├──────────────► concord-macros
   └──────────────► concord-extras ───► concord-core

concord-examples ─► concord
```

The façade/extras/compiler/kernel separation is real. The generated public surface has been materially redesigned around named request wrappers and inherent methods. The DSL v2 work is present. Examples, tests, fixtures, documentation, migration material, and performance tooling are all much closer to the desired product model than the original repository.

However, the repository is not complete in four areas that the reference treats as completion criteria rather than optional polish:

1. **Finite runtime construction is not fully enforced.** `RuntimeEndpointBuilder` and the public runtime rate-limit constructors accumulate unbounded input before checking their advertised limits. This is a direct contradiction of the product's boundedness thesis and makes Phase 9 incomplete.
2. **The CI claim is not represented by the supplied source.** No `.github/workflows/ci.yml` or any YAML workflow exists in the archive, despite Phase 13 evidence claiming that it does. Phase 13 and the platform portion of Phase 14 are therefore incomplete.
3. **Release validation is knowingly incomplete.** Full package preparation fails for core, extras, and the façade; exact MSRV UI suites fail; the rust-analyzer acceptance protocol and non-Windows lanes were not run. Those are explicitly required Phase 14 proofs.
4. **The progress ledger overstates completion.** Phases 13 and 14 are marked `Complete` while their own evidence records unmet exit criteria. Phase 9 is also marked complete despite a substantive finite-construction defect.

A more accurate project status is:

> **The v2 crate architecture, public language, generated API, examples, compatibility removal, and most deterministic runtime proofs are substantially implemented. The refactor remains incomplete with respect to bounded runtime construction, executable CI/platform evidence, MSRV diagnostic proof, rust-analyzer acceptance, and publication graph hardening.**

### Readiness summary

| Area | Audit result |
|---|---|
| Product/crate architecture | **Implemented correctly in substance** |
| Façade-only application model | **Implemented correctly** |
| Extras boundary | **Implemented correctly in structure; two boundedness issues remain in first-party implementations/contracts** |
| DSL v2 and predictable generated API | **Implemented correctly by static inspection** |
| One managed executor and transport isolation | **Preserved by static inspection and represented by extensive tests** |
| Generated request discoverability | **Implemented statically; interactive editor proof remains open** |
| Runtime endpoint design | **Semantics mostly implemented; finite construction is incomplete** |
| Macro internal restructuring | **Substantially implemented; emit still depends on source AST vocabulary** |
| Examples and test architecture | **Largely implemented correctly** |
| Documentation information architecture | **Largely implemented; several truthfulness and link issues remain** |
| Workspace/tooling | **Mostly implemented; CI workflow is absent and correctness recipes are not locked** |
| Release hardening | **Incomplete / blocked** |

---

## 2. Review scope, evidence, and limitations

### 2.1 What was inspected

The audit inspected:

- all workspace and fixture manifests;
- the façade, core, extras, macro, examples, test-support, xtask, and performance sources;
- generated-surface and UI tests;
- deterministic lifecycle/security/bounds tests;
- example module and binary organization;
- documentation and phase evidence;
- migration and changelog material;
- the progress ledger;
- a no-index comparison between the original and refactored archives.

The old-to-new no-index comparison contains approximately **649 changed files, 36,341 insertions, and 25,421 deletions**, which is consistent with a broad implementation rather than a superficial repository reshuffle.

### 2.2 What could not be independently verified

The review environment does not contain Cargo, Rust, Just, rust-analyzer, or the Windows toolchain used in the evidence. Therefore, the listed test results are treated as **recorded project evidence**, not as independently reproduced results.

The supplied archive also contains no `.git` directory. Consequently, this audit cannot independently verify:

- branch `master`;
- HEAD `22667bc9314e3e47f324d9c5685bf6219f00ef46`;
- a clean worktree;
- the stated phase commit boundaries;
- whether anything was pushed;
- the actual Git diff between the stated commits.

The archive permits static source comparison only. `git diff --check` in an evidence document proves whitespace validity for the worktree where it was run; it does not prove that the final archive corresponds to the stated commit or that its worktree was clean.

### 2.3 Severity rubric

| Severity | Meaning |
|---|---|
| **Blocker** | Contradicts a non-negotiable product invariant or an explicit final exit criterion; completion should not be claimed until corrected. |
| **High** | Material correctness, validation, release, or product-truthfulness gap. |
| **Medium** | Important API, maintainability, documentation, or proof-quality defect that should be resolved before a public release. |
| **Low** | Cleanup or evidence-quality issue that does not invalidate the main architecture. |

---

## 3. What was implemented correctly

The following claims in the supplied summary are supported by the source.

### 3.1 Final crate direction

The root workspace exposes the intended packages under `crates/`, with `fixtures` explicitly excluded (`Cargo.toml:1-20`). The normal dependencies are:

- `concord` → `concord-core`, `concord-extras`, `concord-macros` (`crates/concord/Cargo.toml:27-30`);
- `concord-extras` → `concord-core` (`crates/concord-extras/Cargo.toml:17-27`);
- `concord-examples` → `concord` (`crates/concord-examples/Cargo.toml:25-30`);
- `concord-macros` has only compiler/proc-macro dependencies and no normal Concord runtime dependency (`crates/concord-macros/Cargo.toml:12-18`).

This is the right product graph.

### 3.2 Façade feature ownership

The façade defaults are exactly:

```text
default-tls
http2
rate-limit-governor
```

and JSON remains opt-in (`crates/concord/Cargo.toml:9-25`). Core and extras each have an empty default feature set (`crates/concord-core/Cargo.toml:9-20`, `crates/concord-extras/Cargo.toml:9-15`).

This correctly makes `concord` the application-facing feature owner while retaining implementation isolation.

### 3.3 Extras/compiler separation

`concord-extras` owns the concrete first-party extensions anticipated in the reference: JSON, CSV, NDJSON, pagination controllers, governor/default observer, and OAuth2 client credentials. Searches of the macro source show no concrete knowledge of those extras types, and searches of extras show no native Reqwest dependency or alternate executor.

This is an important success: extras is a real downstream proof of the public core extension contracts rather than a privileged friend crate.

### 3.4 Generated façade bridge

The macro resolves the caller-visible façade package, including dependency renaming, and generates through the façade bridge. The direct-core application fallback is removed, and the generated contract version is consistently set to 23.

This satisfies the one-dependency product model and prevents generated code from relying on a transitive implementation crate.

### 3.5 Public generated surface

Static inspection supports the main Phase 6 claims:

- named endpoint request wrappers exist;
- optional/default setters and legal terminals are inherent methods;
- generated per-client preludes and request extension traits are gone;
- endpoint values and request wrappers live in one scope-oriented module tree;
- the generated endpoint capability trait is sealed;
- configured clients use named builders;
- zero-input clients retain `new()` while configured clients do not expose positional secret/provider constructors.

This directly improves rust-analyzer completion and ordinary Rust diagnostics, even though the interactive acceptance protocol remains unexecuted.

### 3.6 DSL v2

The new syntax and resolver structure support:

- snake_case endpoint and scope source names;
- order-independent named clauses;
- explicit `#[body]` parameters;
- syntactic `Option<T>` optionality;
- defaulted values;
- stable contract IDs;
- generic custom providers rather than macro-special OAuth2;
- removal of the legacy parser branches listed in the summary.

The source therefore supports the claimed public language migration.

### 3.7 Runtime model preservation

The repository retains the central one-executor design. Generated direct requests, explicit endpoint values, and runtime endpoints converge on core request preparation and execution. Core continues to own authentication phases, rate-limit contracts, bounded response acquisition, pagination execution, hooks, redaction, errors, completion, and native transport isolation.

No alternate Reqwest path was introduced through extras.

### 3.8 Examples, tests, and fixtures

The examples are importable modules; binaries are thin; the Riot material is split by domain/API/usage; deterministic tests import the actual example definitions. The fixture workspace is isolated at the root, has one lockfile, and is focused on Cargo-graph assertions. This is a substantial correction of the original repository's test fragmentation.

### 3.9 Documentation and performance narrative

The documentation follows the requested declaration → generated Rust → construction/use → execution → capabilities → reference progression. Migration, changelog, contributor evidence, and performance measurements exist. The scale harness covers 1/10/100/1,000 endpoints and records runtime categories without presenting observational timings as hard thresholds.

These are all meaningful improvements.

---

## 4. Findings requiring correction

## F-01 — Runtime endpoint construction is not actually bounded

**Severity:** Blocker
**Affected phase:** 9, and therefore 14
**Invariant:** finite ordinary behavior

### Observed implementation

`RuntimeEndpointBuilder` defines structural limits:

```rust
const MAX_RUNTIME_PUBLIC_COMPONENTS: usize = 256;
const MAX_RUNTIME_PUBLIC_BYTES: usize = 64 * 1024;
const MAX_RUNTIME_AUTH_PLACEMENTS: usize = 64;
```

at `crates/concord-core/src/request/prepared.rs:23-25`.

However, every mutator eagerly allocates or appends before those limits are checked:

- `path_segment` calls `value.to_string()` and pushes it (`:89-94`);
- `query` converts both name and value and pushes (`:99-104`);
- `query_values` consumes and extends the full arbitrary iterator (`:121-131`);
- `header` formats and stores the complete value (`:137-150`);
- `auth` pushes without a cap (`:181-185`);
- `rate_limit` pushes without a cap (`:209-213`).

The first structural validation occurs only in `build()` (`:246-277`), after all allocations and iterator consumption have already happened. The boundary test at `:652-702` proves only that the completed oversized vector is rejected; it does not prove bounded construction.

### Concrete failure modes

1. `query_values("x", std::iter::repeat(value))` does not return. It consumes an infinite iterator even though the endpoint has a 256-component cap.
2. A very large `Display` result is fully allocated before the 64 KiB limit is checked.
3. The builder can hold arbitrarily many auth placements or rate-limit rules before `build()` rejects them.
4. The 64 KiB accounting excludes auth placement names and the internal structure of rate-limit plans.

This is not merely an optimization concern. Concord claims that difficult runtime behavior is centrally bounded and deterministic. A public first-party builder that accepts an unbounded iterator and eagerly collects it violates that claim before execution begins.

### Required correction

Use an incrementally checked, error-latching builder or make mutators fallible. A preserving-chain design is possible:

```rust
pub struct RuntimeEndpointBuilder<R> {
    state: Result<RuntimeEndpointParts<R>, ConfigurationError>,
    components: usize,
    public_bytes: usize,
}
```

Each mutator should:

1. calculate the next component and byte count with checked arithmetic;
2. reject before pushing or allocating beyond the cap;
3. consume collection iterators only through the first excess item;
4. retain a safe structural error without retaining the rejected value;
5. include auth placement names and rate-limit plan shape in its accounting.

For formatted values, use a capped `fmt::Write` sink so Concord-owned output buffering stops at the first excess byte. An adversarial `Display` implementation can still perform arbitrary user work; that should be documented as an advanced trust boundary, but Concord need not allocate its entire output.

### Required proof

Add deterministic tests such as:

- `runtime_endpoint_query_values_stops_at_first_excess`;
- a `PanicAfter(limit + 1)` iterator proving no tail consumption;
- first-excess component, byte, auth-placement, and rate-rule tests at mutation time;
- a large formatted value test proving no complete oversized allocation is retained;
- a plan/accounting test that includes auth names and nested rate-limit structure.

Until this is corrected, Phase 9 cannot be classified as complete.

---

## F-02 — Runtime rate-limit values have unbounded structural constructors

**Severity:** Blocker
**Affected phases:** 3, 9, 14
**Invariant:** bounded limiter key preparation and state

### Observed implementation

`crates/concord-core/src/rate_limit/types.rs` contains several public constructors with no structural cap:

- `RateLimitId::new` accepts any nonempty string (`:15-22`);
- `RateLimitKeyPart::new` accepts arbitrary owned strings and is infallible (`:41-45`);
- `RateLimitKey::from_parts` first collects the entire iterator (`:81-104`);
- `RateLimitRule::new` first collects every window (`:163-183`);
- `RateLimitPlan::from_rules` accepts any vector (`:225-231`).

`RateLimitPlan::validate_key_limit` only checks aggregate key bytes later (`:246-250`). It does not cap:

- number of key parts;
- bytes in an ID;
- number of windows per rule;
- number of rules per plan;
- iterator consumption before validation.

The governor then expands rules/windows into contribution/state structures. A runtime user can therefore cause large pre-acquisition allocations even though governor state and key bytes are otherwise described as bounded.

### Why this matters

The core contract is the semantic boundary used by both first-party extras and third parties. Bounds that exist only in one limiter implementation do not bound construction of the semantic plan itself. Runtime endpoints compound the problem because they accept already-built rules while counting only `rate_limits.len()`.

### Required correction

Core should own and document finite structural limits, for example:

```rust
pub const MAX_RATE_LIMIT_ID_BYTES: usize = ...;
pub const MAX_RATE_LIMIT_KEY_PARTS: usize = ...;
pub const MAX_RATE_LIMIT_WINDOWS_PER_RULE: usize = ...;
pub const MAX_RATE_LIMIT_RULES_PER_PLAN: usize = ...;
```

Then:

- make `RateLimitKeyPart` construction fallible or provide a fallible canonical constructor;
- make `RateLimitKey::try_from_parts` consume incrementally and stop at first excess;
- make `RateLimitRule::new` cap windows while consuming;
- make plan construction validated and fallible;
- preserve hidden generated constructors only for compiler-proven static shapes, with the macro resolver enforcing the same limits;
- keep the runtime-selected limiter key-byte limit as an additional operational cap rather than the only structural guard.

### Required proof

Add first-excess and no-overconsumption tests for key parts, windows, and rules. Test a maximum valid plan through the governor and first excess through both core construction and runtime endpoint binding. Ensure errors reveal no key values.

---

## F-03 — The claimed CI workflow is absent

**Severity:** Blocker for Phase 13/14 completion
**Affected phases:** 13 and 14

### Observed contradiction

`docs/contributor/evidence/refactor-phase-13-docs-tooling.md:48-50` states:

```text
.github/workflows/ci.yml defines explicit ... lanes
```

The supplied archive contains no `.github` directory and no `.yml` or `.yaml` file anywhere in the repository.

The Phase 13 reference explicitly requires defined CI lanes for format, stable, all-features, no-default, MSRV, Clippy, lifecycle/security, generated/UI, extras, fixtures, examples/docs, deny, performance, and Linux/Windows/macOS. Phase 14 requires running platform CI.

### Consequence

The source does not support the statement that the lanes are defined. This is stronger than “the lanes were not executed locally”: the executable CI definition is missing from the reviewed artifact.

### Required correction

Restore or add `.github/workflows/ci.yml` and make it part of the repository artifact. At minimum:

- pin action versions;
- define Windows, Linux, and macOS jobs;
- separate stable, MSRV, all/no-default, UI, security, docs, fixtures, and package checks coherently;
- serialize the Windows fixture/public-compile jobs that target the same executable to avoid the recorded `LNK1104` collision;
- add `actionlint` or another parser/linter;
- make `xtask docs` or a dedicated repository-policy test fail if the required workflow or job names are absent.

Then execute the workflow and retain links/results as release evidence.

---

## F-04 — Phase 14 release validation is explicitly incomplete

**Severity:** High / release blocker
**Affected phase:** 14

The reference requires full proof matrix execution, package dry runs, docs links, platform CI, MSRV UI where expected outputs are supported, and the rust-analyzer acceptance protocol.

The final evidence records all of the following as unresolved:

- core package preparation fails because exact unpublished `concord-test-support` is absent (`refactor-phase-14-release.md:180`);
- extras and façade package preparation fail because exact unpublished core is absent (`:181-182`);
- exact MSRV façade UI has 19 snapshot failures (`:149`);
- exact MSRV core UI has 14 snapshot failures (`:150`);
- interactive rust-analyzer checks were not executed (`:156-158`);
- Linux/macOS and GitHub Actions were not executed (`:158-160`);
- publication metadata and package name ownership remain unresolved (`:190-195`).

These may be understandable release blockers, but they are not passing exit criteria. `cargo package --list` verifies an inventory; it is not a substitute for package creation and verification.

### Required correction

Change the ledger status to `Blocked` or `Implementation complete; release validation incomplete` until these gates are resolved. Do not use `Complete` as an umbrella for implementation plus known failed final proofs.

---

## F-05 — Public release graph depends on an internal unpublished test-support package

**Severity:** High
**Affected phases:** 5, 13, 14

### Observed implementation

Core has a normal optional dependency on `concord-test-support`:

```toml
dangerous-dev-tools = ["dep:concord_test_support"]
```

at `crates/concord-core/Cargo.toml:20-24`. The façade forwards it as a public feature (`crates/concord/Cargo.toml:25`). All packages are `publish = false`, and test support is described as internal.

This optional normal dependency is why `cargo package -p concord-core` cannot prepare against the registry until test support is published. The release evidence therefore concludes that publication order begins with `concord-test-support`.

### Why the design is undesirable

An internal deterministic testing/instrumentation crate should not become a registry prerequisite of the public runtime merely because a public feature exposes development probes. This:

- leaks internal test architecture into the package graph;
- forces publication of a package described as private/internal;
- exposes `dangerous-dev-tools` in the façade feature table;
- weakens the claim that product crate boundaries reflect semantic ownership rather than test needs.

### Required correction

Choose one intentional design:

1. **Preferred:** remove `dangerous-dev-tools` from the public façade/core feature topology and move performance/test instrumentation to dev-only tooling, public hooks, or an unpublished benchmark harness.
2. Publish a deliberately named and documented `concord-dev-tools` package as a supported but advanced package. Do not call it internal and do not use the test-support package identity.

Afterward, prove package construction in a temporary local registry or source-replacement setup in the intended publication order.

---

## F-06 — MSRV diagnostics are not proven by `just verify`

**Severity:** High
**Affected phases:** 12, 13, 14

### Observed implementation

`just test-msrv` runs only:

```text
cargo +1.85.0 check --workspace --all-targets --no-default-features
cargo +1.85.0 test -p concord-core --lib --no-default-features
cargo +1.85.0 test -p concord-extras --all-features
```

(`justfile:30-33`). It does not run the façade or core MSRV UI suites.

The final evidence confirms those suites fail only because compiler rendering differs, but they still fail (`refactor-phase-14-release.md:145-154`). Therefore, `just verify` does not establish the full compile-time diagnostic claim on MSRV.

### Required correction

Adopt and document a cross-compiler diagnostic policy. Reasonable options are:

- stable owns exact Trybuild `.stderr` snapshots; MSRV runs a normalized compile-fail harness that checks failure category and key semantic fragments;
- maintain compiler-specific snapshots;
- use structured compiler JSON diagnostics and normalize unstable rendering fields.

Add the chosen MSRV diagnostic proof to `just test-msrv` and CI. The current state is not wrong because snapshots differ; it is incomplete because the required proof has no passing owner.

---

## F-07 — Rust-analyzer acceptance remains unexecuted

**Severity:** High
**Affected phases:** 6 and 14

Rust-analyzer discovery, completion, hover, navigation, dependency-renaming behavior, and diagnostics were central evaluation criteria. The final evidence records only `rust-analyzer --version`; it explicitly says the interactive protocol was not run.

Static API design strongly suggests an improvement over generated extension traits, but the product claim requires actual editor acceptance.

### Required correction

Execute and record the reference protocol in a real editor session:

1. completion after a direct endpoint call;
2. hover showing the named request wrapper;
3. optional setters and terminals available without imports;
4. absent `paginate` on a non-paginated endpoint;
5. go-to-definition from generated method/type to declaration or stable generated item;
6. dependency rename behavior;
7. feature-disabled method absence;
8. diagnostics at relevant DSL spans;
9. no fixture-workspace diagnostic pollution.

A command-line `rust-analyzer analysis-stats` run is a useful supplementary check, but not a substitute.

---

## F-08 — OAuth2 performs unbounded pre-transport amplification

**Severity:** High
**Affected phase:** 4

### Observed implementation

The first-party OAuth2 provider acknowledges that callers must bound builder strings (`crates/concord-extras/src/auth/oauth2_client.rs:13-21`). The builder accepts unconstrained client ID, secret, and scope (`:132-150`). During acquisition, `exchange_parts`:

- form-encodes the full client ID;
- form-encodes the full secret;
- creates another combined `username:password` string;
- base64-encodes it into another string;
- formats a final Authorization value;
- separately builds a complete scope form body (`:52-68`, `:302-334`).

Core's provider request-body limits apply after those allocations.

### Consequence

This is not a native-transport escape, and secret redaction appears strong, but a first-party ordinary extension can amplify unbounded configuration before the central transport bound applies. That is inconsistent with the product's finite ordinary behavior unless it is explicitly classified as an advanced trust boundary. OAuth2 is presented as a first-party ordinary capability, so the better design is to bound it.

### Required correction

- define finite client ID, secret, scope, Authorization, and form-body limits;
- validate length at provider build time without echoing values;
- compute encoded lengths with checked arithmetic before allocation;
- avoid multiple full-size intermediates where possible;
- expose the provider request limit semantically from core if the extras implementation should align with the configured bound.

Add boundary/first-excess tests and confirm every error remains secret-safe.

---

## F-09 — `EndpointId` exposes a panic-prone infallible conversion

**Severity:** Medium-high
**Affected phase:** 9

`EndpointId::new` is correctly fallible and bounded (`crates/concord-core/src/endpoint.rs:35-50`). `EndpointId::from_static` uses a const assertion (`:52-65`), which is reasonable for generated literals. However, the public implementation:

```rust
impl From<&'static str> for EndpointId {
    fn from(value: &'static str) -> Self {
        Self::from_static(value)
    }
}
```

at `:88-92` means ordinary runtime code can write an invalid static string with `.into()` and panic. `From` is conventionally infallible.

### Required correction

Remove `From<&'static str>`. Keep `EndpointId::new`/`TryFrom` for public runtime use. Give generated code a hidden validated-static constructor through `concord::__private`, or retain `from_static` only with unmistakable generated/static documentation and no infallible trait conversion.

Add a public compile test proving invalid `.into()` is unavailable.

---

## F-10 — Macro emission still depends directly on source AST vocabulary

**Severity:** Medium
**Affected phase:** 10

The macro was successfully reorganized into `syntax`, `semantic`, and `emit` directories, and a surface model exists. However, the separation is not complete:

- `semantic/model.rs:6-8` publicly reuses source AST enums such as `AuthChallenge`, `AuthPlacementKind`, `DefaultExpr`, `MethodKind`, `RetrySelection`, and `ValueKind`;
- `emit/mod.rs:14-16` imports those enums directly from `crate::syntax::ast`;
- emit helpers and docs pattern-match those source grammar forms.

This means emission still understands source-language representation instead of depending exclusively on normalized semantic/surface types. A future grammar change can therefore propagate into code generation even when semantics are unchanged.

### Required correction

Define normalized equivalents in the semantic model, for example `ResolvedMethod`, `ResolvedChallengePolicy`, `ResolvedAuthPlacement`, `ResolvedDefault`, `ResolvedRetry`, and `ResolvedValueKind`. Convert once during resolution. Make `emit` import only semantic/surface modules.

Add a source-policy test that rejects non-test `emit` imports from `syntax`. This is a contributor-comprehension and language-evolution improvement, not a reason to rewrite the pipeline again.

---

## F-11 — Client operation timeout naming and generated semantics are surprising

**Severity:** Medium
**Affected phases:** 7 and 9

`RuntimeConfigBuilder::operation_timeout` is documented as “client-wide” but inherited only by runtime endpoints (`crates/concord-core/src/runtime/config.rs:80-93`). Generated endpoints without an explicit declaration timeout are normalized to `TimeoutSelection::Disabled` (`crates/concord-core/src/request/head.rs:34-45`, `crates/concord-macros/src/emit/descriptions.rs`).

The main client guide demonstrates `runtime.operation_timeout(...)` during ordinary generated-client construction without immediately emphasizing that it has no effect on generated endpoints (`docs/guide/03-build-a-client-and-make-requests.md:7-14`).

This is internally consistent but not predictable from the name “client-wide operation timeout.”

### Required correction

Choose one:

1. rename the setting to `runtime_endpoint_timeout`;
2. make generated endpoints with no explicit DSL timeout inherit the client timeout;
3. retain the design but document the exception prominently in the guide and generated builder docs.

The decision should be explicit because it affects ordinary safety defaults.

---

## F-12 — `dangerous-dev-tools` is exposed as a user feature

**Severity:** Medium
**Affected phases:** 5, 13, 14

The façade exposes `dangerous-dev-tools` (`crates/concord/Cargo.toml:25`) and documentation generation includes it as “Internal development instrumentation.” An internal feature should not normally appear in the user façade's feature contract.

This is both a product-surface leak and the cause of the publication dependency on test support.

### Required correction

Remove it from the published façade and user feature reference, or consciously rename/document it as a supported advanced package capability. Do not simultaneously describe it as internal and require users/registries to resolve it.

---

## F-13 — `just verify` is not a complete reproducible final gate

**Severity:** Medium-high
**Affected phases:** 13 and 14

### Observed implementation

The recipes in `justfile` omit `--locked` on correctness commands (`justfile:7-87`). As a result, `just verify` can update `Cargo.lock` and still pass. It also:

- does not run the exact MSRV UI suites;
- does not perform package dry runs;
- cannot perform interactive rust-analyzer checks;
- does not run platform CI;
- does not check that the Git worktree is clean;
- does not verify that a workflow file exists.

`git diff --check` in evidence checks whitespace errors, not worktree cleanliness.

### Required correction

- add `--locked` to CI and final correctness commands;
- separate `verify-local` from `verify-ci` or document exact scope;
- add a final `git diff --exit-code`/`git status --porcelain` check in CI, not indiscriminately in a developer command that may run with intentional local changes;
- make the final report state precisely what `just verify` does and does not prove;
- include MSRV semantic UI proof and workflow policy checks.

---

## F-14 — Documentation/release evidence contains false or unchecked claims

**Severity:** Medium
**Affected phases:** 13 and 14

### Specific issues

1. Phase 13 evidence claims `.github/workflows/ci.yml` exists, but it is absent.
2. Phase 13 evidence says the justfile “no longer selects PowerShell” (`refactor-phase-13-docs-tooling.md:45-46`), while `justfile:2` explicitly sets the Windows shell to `powershell.exe`. This may be a deliberate platform-specific shell choice rather than a portability defect, but the evidence wording is inaccurate.
3. The xtask link checker intentionally skips the entire `docs/contributor/evidence` tree (`tools/xtask/src/main.rs:362-371`), does not validate anchors, and ignores external URLs. Therefore “link checks PASS” means only selected local file targets pass.
4. Two broken links are present in the skipped evidence:
   - `docs/contributor/evidence/refactor-phase-0-baseline.md:4` points two levels up to a nonexistent `docs/CONCORD_REFACTOR_REFERENCE.md`; it needs three levels;
   - `docs/contributor/evidence/refactor-phase-5-namespaces-features.md:137` points to missing `packaging-and-publication.md`.
5. The xtask recipe check verifies recipe names, not that the recipes execute (`tools/xtask/src/main.rs:293-320`).

### Required correction

Make evidence claims precise. Expand link checking to evidence and anchors, or run a separate evidence checker. Add workflow existence/lint checks. Correct the two broken links and the PowerShell wording.

---

## F-15 — README installation instructions are not presently executable

**Severity:** Medium
**Affected phases:** 13 and 14

The README begins with:

```toml
concord = { version = "0.1.0", features = ["json"] }
```

(`README.md:5-10`). The same file only later explains that all product packages are `publish = false` (`:87-89`), and the release evidence says the public package name is occupied.

For a repository that is not published, the first installation instruction is currently false for users.

### Required correction

Before publication, either:

- show a path/Git dependency appropriate to the repository;
- place a prominent “not yet published” statement before the registry snippet;
- mark the registry snippet as the intended release form rather than a current installation command.

After a package-name decision, ensure the Cargo package name used by `proc_macro_crate` and the displayed dependency are consistent.

---

## F-16 — Public runtime structural limits lack one canonical reference owner

**Severity:** Medium
**Affected phases:** 9 and 13

The 256-component, 64 KiB, and 64-auth-placement runtime endpoint limits are private constants in `request/prepared.rs`. They are public behavior because exceeding them returns a configuration error. They are not present in the canonical generated limits reference; their most explicit explanation appears in phase evidence.

Rate-limit structural limits are absent entirely because they have not been implemented.

### Required correction

Move finite public structural values to documented public constants or a `RuntimeEndpointLimits`/`ContractLimits` reference type. Generate/check the reference table from that owner. Avoid placing exact normative limits only in historical evidence.

---

## F-17 — Residual direct-core generated bridge helpers appear obsolete

**Severity:** Low-medium
**Affected phase:** 14

Core still defines and reexports hidden generated raw/dev helper macros in `crates/concord-core/src/lib.rs` and `src/__private.rs`. The façade defines its own equivalents, macro expansion now goes exclusively through the façade, and static search finds no product use of the core copies.

This looks like compatibility residue after direct-core generation was removed. A hidden multipart feature assertion also still names `concord_core/multipart` in a stale diagnostic path, although the façade emits the correct user-facing assertion.

### Required correction

Remove unused core bridge helpers or document their remaining semantic owner. Add a source/expansion test ensuring generated code cannot emit implementation-crate feature names.

---

## F-18 — Broader formatted-value boundedness remains an explicit design decision

**Severity:** Medium design decision
**Affected areas:** core request shaping and runtime endpoint construction

Generated/public head shaping often calls `Display::to_string()` before validating the result. An arbitrary downstream `Display` implementation can allocate, loop, or panic. Concord cannot make arbitrary user code finite, but it can bound its own capture of formatted output.

### Required decision

Document the distinction:

- arbitrary work inside user trait/`Display` implementations is an advanced trust boundary;
- Concord-owned buffering must stop at a finite cap.

Adopt capped formatting where practical for host/path/query/header construction. This is less urgent than F-01/F-02 because it also exists in generated request shaping and may be classified as user-code trust, but it should not remain accidental.

---

## 5. Phase-by-phase assessment

| Phase | Audit status | Summary |
|---:|---|---|
| 0 | **Documented, not independently verifiable** | Reference/evidence/ledger are present, but archive lacks Git metadata and runtime tools. The baseline history and commands cannot be reproduced here. |
| 1 | **Complete by static inspection** | Façade, hidden bridge, final dependency direction, renamed façade handling, and one-crate compile ownership are present. |
| 2 | **Complete by static inspection** | JSON/CSV/NDJSON and built-in pagination moved to extras; compiler remains concrete-type agnostic. |
| 3 | **Substantially complete** | Governor/default observer extraction and façade composition are correct; public rate-plan construction remains structurally unbounded. |
| 4 | **Substantially complete** | OAuth2 is an ordinary extras provider and macro-special syntax is gone; first-party OAuth configuration/encoding is not bounded before transport. |
| 5 | **Architecture complete; release posture incomplete** | Namespaces/features/one-crate consumers are correct; dev-tools/test-support leaks into the public package graph and metadata remains unfinished. |
| 6 | **Statically complete; editor proof open** | Named wrappers, inherent capabilities, endpoint trait, and module tree are present; rust-analyzer acceptance was not executed. |
| 7 | **Substantially complete** | Named client builders and runtime-policy configuration are present; operation-timeout semantics/naming remain surprising. |
| 8 | **Complete by static inspection** | DSL v2 and legacy grammar removal appear implemented. |
| 9 | **Incomplete** | Descriptions, planning, runtime endpoint, and timeout tri-state exist, but runtime construction violates finite-bound requirements. |
| 10 | **Substantially complete** | Physical/pipeline restructuring is present; emission still imports source AST concepts. |
| 11 | **Complete by static inspection** | Importable modules, thin binaries, deterministic local proofs, and split Riot narrative are present. |
| 12 | **Structurally complete; proof incomplete** | Contract-oriented tests/fixtures are reorganized; exact MSRV diagnostic proof remains red. |
| 13 | **Incomplete** | Docs/workspace/xtask/just structure exists; workflow is absent, publication metadata is not finalized, commands are not fully locked, and evidence claims are inaccurate. |
| 14 | **Incomplete / blocked** | Compatibility removal and measurements are present, but package gates, MSRV UI, rust-analyzer, platform CI, and Phase 9 boundedness are unresolved. |

### Phase 0 — Baseline and contract freeze

**Correctly present:** root reference, progress ledger, phase evidence, baseline inventories, target sketches in the reference, and invariant ownership narrative.

**Not independently proven:** starting branch/commit/worktree, recorded command outputs, and phase commit boundaries because `.git` and toolchain are absent from the supplied archive.

**Required action:** no product-code action solely for Phase 0; retain a source archive or Git bundle with metadata when asking for independent completion certification.

### Phase 1 — Façade and crate graph

**Result:** passes static audit.

The façade is the only documented user dependency, macro expansion resolves it, internal versions are exact, and extras/macros do not form forbidden edges. The direct-core fallback is absent.

**Residual:** resolve the eventual public package-name decision without changing the user-facing Rust crate name unless necessary.

### Phase 2 — Pure extras extraction

**Result:** passes static audit.

Concrete codecs and built-in pagination controllers reside in extras and use public core contracts. The macro contains no concrete extras names. No native request/response escape is visible.

### Phase 3 — Governor extraction

**Result:** mostly correct.

The problematic private downcast/task-local ownership from the old design appears to have been replaced by public semantic contracts and façade composition. The remaining failure is not the crate move; it is the unbounded construction of `RateLimitId`, keys, windows, and plans described in F-02.

**Close phase only after:** adding core-owned structural limits and first-excess tests.

### Phase 4 — OAuth2 extraction

**Result:** mostly correct.

OAuth2 is an ordinary `CredentialProvider` in extras, the macro is generic, provider HTTP remains isolated, and secret-safe errors/debug are tested. The missing finite configuration/encoding bound in F-08 should be corrected before calling the extraction fully aligned with the product thesis.

### Phase 5 — Public namespaces and features

**Result:** public architecture is correct; package/release architecture is not final.

The façade owns common, prelude, advanced, dangerous, feature forwarding, and extras composition. Core/extras defaults are empty. Examples depend on the façade.

The public `dangerous-dev-tools` feature and normal test-support dependency are contrary to the intended semantic package boundary. Publication metadata was explicitly deferred even though Phase 5 required a graph publishable in principle.

### Phase 6 — Generated request surface

**Result:** source implementation appears correct.

The main architectural objective—ordinary inherent completion with named wrappers—was achieved. Static tests and rustdoc checks are extensive.

**Open proof:** execute the rust-analyzer protocol. Until then, describe this as “compiler/rustdoc surface complete; editor acceptance pending.”

### Phase 7 — Client construction/runtime policy

**Result:** substantially correct.

Named client builders, zero-input `new`, deterministic repeated configuration, and client runtime closure are present. Positional secrets/providers are removed.

**Open design:** clarify or rename the runtime-only client timeout behavior.

### Phase 8 — DSL v2

**Result:** passes static audit.

The expected syntax changes and compatibility removals appear in source and tests. No evidence was found of the retired fixed-order/PascalCase/body-name grammar remaining in the product path.

### Phase 9 — Descriptions, planning, runtime endpoints

**Result:** incomplete.

The phase implemented the visible types and semantics:

- `RuntimeEndpoint` replaces `PreparedEndpoint`;
- endpoint descriptions are public/safe;
- planning uses public-head preparation and has dedicated zero-effect tests;
- timeout selection is tri-state;
- generated and runtime endpoints share core execution.

But the phase's bounded-runtime-endpoint requirement is not met because collection and allocation happen before validation. F-01 and F-02 are phase-closing defects, not post-release polish.

### Phase 10 — Macro architecture

**Result:** substantially complete.

The repository now has syntax, semantic, and emit domains, resolved surface data, artifact-oriented emit modules, and localized tests. This is materially better than the original flat parse/resolve/codegen files.

The direct `emit → syntax::ast` dependency should be removed so grammar evolution is isolated from generation. This does not require another broad restructuring; it requires completing normalization ownership.

### Phase 11 — Examples

**Result:** passes static audit.

Examples are importable, binaries are thin, deterministic tests own runtime behavior, and Riot is split. The earlier synthetic “canonical” API problem is corrected.

### Phase 12 — Tests and fixtures

**Result:** structural objective achieved, final proof objective incomplete.

The fixture consolidation, one lockfile, actual example imports, and test-owner taxonomy are represented well. The exact MSRV UI suites still fail and lack an accepted passing proof strategy. That should be recorded as an open test-architecture decision rather than hidden behind stable-only snapshots.

### Phase 13 — Documentation, workspace, tooling, CI

**Result:** incomplete.

Correct work includes the repository tree, docs journey, xtask, Just recipes, fixture isolation, lockfile simplification, and generated fact tables.

Incomplete or incorrect work includes:

- no CI workflow in the supplied source;
- publication metadata not finalized;
- correctness recipes not consistently `--locked`;
- evidence link checking deliberately excludes evidence;
- documented workflow/PowerShell claims do not match the artifact;
- MSRV is only partially proven.

### Phase 14 — Compatibility removal and release hardening

**Result:** incomplete / blocked.

The compatibility removals, migration guide, changelog, contract-version bump, public surface audit, and performance measurements appear complete.

The phase cannot close while:

- Phase 9 boundedness is false;
- package preparation fails for three product packages;
- exact MSRV UI proof is red;
- rust-analyzer acceptance is unexecuted;
- Linux/macOS/Actions are unexecuted and the workflow is absent;
- release metadata/package-name strategy is unresolved.

---

## 6. Claim-by-claim audit of the supplied completion summary

| Supplied claim | Audit result | Qualification |
|---|---|---|
| Full refactor complete and committed | **Not independently verifiable; completion overclaimed** | No Git metadata; substantive and release gaps remain. |
| Worktree clean on stated master commit | **Unverifiable** | Archive contains no `.git`. |
| Nothing pushed | **Unverifiable** | Requires remote/Git metadata. |
| Five-crate architecture | **Confirmed** | Test support/tools also remain, appropriately. |
| Façade sole documented dependency | **Confirmed** | README's registry snippet is not currently installable. |
| Core owns semantic runtime kernel | **Confirmed** | `dangerous-dev-tools` creates an undesirable test-support normal edge. |
| Extras only public-contract implementations | **Mostly confirmed** | OAuth boundedness needs strengthening; no native escape found. |
| Macros no runtime dependency/no extras knowledge | **Confirmed** | Emit still knows source AST forms, not concrete extras. |
| DSL/order/naming/body/IDs complete | **Confirmed by static inspection** | Interactive editor behavior remains unproven. |
| Each endpoint has predictable inherent surface | **Confirmed by static inspection** | Rust-analyzer protocol remains open. |
| Runtime timeout tri-state | **Confirmed** | Generated endpoints default to `Disabled`, not client inheritance. |
| Safe planning has zero effects | **Represented by implementation/tests** | Not independently executed in this environment. |
| Preserved runtime invariants | **Strongly represented** | Runtime construction bounds are a significant exception. |
| No alternate executor/native escape through extras | **Confirmed by static inspection** | No Reqwest use in extras found. |
| Examples/fixtures/docs narrative complete | **Largely confirmed** | CI file absent; evidence links and README truthfulness need correction. |
| `just verify` is authoritative final correctness command | **Too broad** | It omits exact MSRV UI, packages, RA, platform CI, workflow existence, and locked reproducibility. |
| All five package inventories pass | **Plausible recorded evidence** | Inventories do not prove package preparation. |
| Publication limitations are the only remaining release issues | **False** | Finite construction, CI definition, MSRV/RA proofs also remain. |
| No architectural deviation record required | **Too strong** | At minimum, the changed validation strategy and deferred release gates require explicit blocker/deviation treatment rather than `Complete`. |

---

## 7. Required remediation plan

The remaining work should be treated as completion phases rather than another broad refactor.

### Completion A — Restore finite runtime construction

**Priority:** first; blocks the product invariant.

1. Introduce core-owned runtime endpoint structural limits with one public/canonical owner.
2. Refactor `RuntimeEndpointBuilder` to reject incrementally and stop iterator consumption at first excess.
3. Introduce bounded/capped formatting for Concord-owned storage.
4. Bound auth placement names and nested rate-limit plan structure.
5. Redesign public rate-limit constructors around finite IDs, parts, windows, and plans.
6. Add no-overconsumption tests with deterministic sentinel iterators.
7. Bound OAuth2 configuration and encoded intermediates.
8. Re-run all lifecycle, security, redaction, governor, runtime endpoint, and planning tests.

**Exit proof:** no public first-party iterator constructor consumes more than its documented maximum plus one detection element; no oversized value is completely retained before rejection; all errors are redacted.

### Completion B — Make validation evidence truthful and executable

**Priority:** second.

1. Add the missing `.github/workflows/ci.yml`.
2. Lint it with `actionlint`.
3. Add repository-policy checks that verify workflow presence and required jobs.
4. Run Windows/Linux/macOS lanes and retain actual run evidence.
5. Correct Phase 13/14 evidence and ledger statuses.
6. Make all final Cargo correctness commands `--locked`.
7. Distinguish `verify-local`, `verify-ci`, package, editor, and platform gates.
8. Add a clean-worktree assertion in CI release jobs.

### Completion C — Resolve MSRV and editor proof ownership

**Priority:** third.

1. Select a stable/MSRV diagnostic snapshot policy.
2. Add a passing semantic compile-fail proof on Rust 1.85.
3. Include that proof in `test-msrv`, CI, and the ledger.
4. Execute the rust-analyzer acceptance protocol in a real editor.
5. Record toolchain, OS, project configuration, observed completions, navigation, and diagnostics.

### Completion D — Make the package graph publishable in principle

**Priority:** fourth, unless publication is intentionally out of scope.

1. Remove the public normal dependency on `concord-test-support`, or consciously replace it with a publishable supported dev-tools package.
2. Remove internal dev instrumentation from the normal façade feature reference.
3. Add approved SPDX license/license-file, repository, homepage, documentation, and readme metadata.
4. Decide the Cargo package name and preserve the desired Rust crate name if needed.
5. Test package order using a temporary local registry/source replacement.
6. Run full `cargo package` verification for every intended public package.
7. Keep `publish = false` until legal/name ownership is approved, but distinguish “not enabled for release” from “architecturally unpackageable.”

### Completion E — Finish semantic and documentation cleanup

1. Remove `emit → syntax::ast` dependencies.
2. Remove panic-prone `EndpointId: From<&'static str>`.
3. Decide the operation-timeout inheritance contract.
4. Fix evidence links and workflow/PowerShell wording.
5. Expand link checks to anchors and evidence.
6. Correct the unpublished README quick start.
7. Add runtime endpoint and rate-plan caps to the canonical limits reference.
8. Remove obsolete hidden core bridge helpers.

---

## 8. Exact validation required after remediation

The following is a target proof set. Commands may be grouped in CI, but no category should disappear.

### 8.1 Formatting, lockfile, and package graph

```bash
cargo fmt --all -- --check
cargo metadata --format-version 1 --all-features --locked
cargo check --workspace --all-targets --locked
cargo check --workspace --all-targets --no-default-features --locked
cargo check --workspace --all-targets --all-features --locked
cargo +stable clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo deny check
```

### 8.2 Core runtime and boundedness

```bash
cargo test -p concord-core --lib --all-features --locked
cargo test -p concord-core --test lifecycle --all-features --locked
cargo test -p concord-core --test security --all-features --locked
cargo test -p concord-core --test bounds --all-features --locked
```

Add focused filters or dedicated tests proving:

- runtime endpoint first-excess rejection before storage;
- iterator no-overconsumption;
- rate-limit ID/key/window/plan caps;
- governor maximum valid plan and first excess;
- OAuth2 encoded-size caps and redaction;
- plan zero effects;
- timeout tri-state behavior.

### 8.3 Public façade/generated surface

```bash
cargo test -p concord --no-default-features --locked
cargo test -p concord --all-features --locked
cargo test -p concord --test generated_surface --all-features --locked
cargo test -p concord --test public_extensions --all-features --locked
cargo test -p concord --test endpoint_planning --all-features --locked
cargo test -p concord --test runtime_endpoint_lifecycle --all-features --locked
cargo test -p concord --test public_compile --no-default-features --locked
cargo test -p concord --test rustdoc --all-features --locked
```

### 8.4 Macro language and diagnostics

```bash
cargo test -p concord-macros --lib --locked
cargo test -p concord --test ui --no-default-features --locked
cargo test -p concord --test ui --all-features --locked
cargo test -p concord-core --test ui --no-default-features --locked
```

On MSRV, run the selected passing semantic diagnostic proof rather than knowingly red exact snapshots:

```bash
cargo +1.85.0 check --workspace --all-targets --no-default-features --locked
cargo +1.85.0 test -p concord-core --lib --no-default-features --locked
cargo +1.85.0 test -p concord-extras --all-features --locked
cargo +1.85.0 test -p concord --test ui_msrv_semantic --no-default-features --locked
cargo +1.85.0 test -p concord-core --test ui_msrv_semantic --no-default-features --locked
```

### 8.5 Extras and examples

```bash
cargo test -p concord-extras --all-features --locked
cargo check -p concord-examples --all-targets --no-default-features --locked
cargo test -p concord-examples --all-features --locked
```

### 8.6 Fixtures, documentation, and workflow

```bash
cargo run -p concord-xtask --locked -- fixtures
cargo run -p concord-xtask --locked -- docs
cargo test --workspace --doc --locked
cargo test --workspace --doc --all-features --locked
cargo rustdoc -p concord --all-features --locked -- -D warnings
cargo rustdoc -p concord-core --all-features --locked -- -D warnings
cargo rustdoc -p concord-extras --all-features --locked -- -D warnings
actionlint .github/workflows/ci.yml
```

The docs task should validate evidence links and Markdown anchors or invoke a separate checker that does.

### 8.7 Package validation

After correcting package metadata/dependencies, use a local registry or equivalent controlled source to prove exact sibling resolution, then run:

```bash
cargo package -p concord-core --locked
cargo package -p concord-extras --locked
cargo package -p concord-macros --locked
cargo package -p concord --locked
```

Only package packages intended for publication. Test-support packaging should not be required merely to publish core unless that dependency is consciously accepted as public architecture.

### 8.8 Editor and platform proof

Record:

- rust-analyzer interactive protocol on the root workspace;
- Linux CI result;
- Windows CI result;
- macOS CI result;
- renamed façade completion/navigation behavior;
- feature-disabled completion behavior.

### 8.9 Final repository truth check

In the actual Git repository:

```bash
git status --short
git diff --check
git diff --exit-code
```

Record the exact commit only after these pass and all required CI checks are associated with it.

---

## 9. Corrected definition of done

The refactor may be marked complete only when all of the following are true:

1. Runtime endpoint and rate-limit construction enforce limits while consuming input, not only after collection.
2. OAuth2 first-party configuration cannot amplify unbounded inputs before provider transport limits.
3. The CI workflow exists in source, is linted, and has passed on the required platforms.
4. Stable and MSRV each have a passing, intentionally owned diagnostic proof.
5. The rust-analyzer acceptance protocol has been executed and recorded.
6. The public package graph is package-verifiable, or publication is explicitly removed from the phase definition rather than marked complete with failed package commands.
7. `just verify` and documentation accurately state their proof scope.
8. Progress ledger states match actual exit criteria and blockers.
9. The supplied source artifact includes Git metadata or a verifiable commit bundle when commit cleanliness/history is part of the claim.
10. All existing lifecycle, redaction, security, completion, pagination, and transport-isolation tests remain green.

---

## 10. Recommended corrected project statement

Use the following wording until the remaining work is closed:

> The Concord v2 architectural implementation is substantially complete. The façade/core/extras/macros split, DSL v2, named generated request surface, explicit/runtime endpoint convergence, examples, tests, documentation, migration material, and compatibility removal are implemented. Final completion remains blocked by incremental boundedness for runtime endpoint/rate-limit construction, the absent executable CI workflow and unrun platform lanes, an unresolved MSRV diagnostic-proof policy, unexecuted rust-analyzer acceptance, and package/publication graph hardening.

That statement accurately credits the large amount of successful work without claiming proofs or guarantees that the reviewed artifact does not yet provide.

---

## Appendix A — Source evidence map

| Topic | Principal source |
|---|---|
| Workspace members and MSRV | `Cargo.toml` |
| Façade features/dependencies | `crates/concord/Cargo.toml` |
| Core dev-tools dependency | `crates/concord-core/Cargo.toml` |
| Extras feature/dependency boundary | `crates/concord-extras/Cargo.toml` |
| Macro runtime independence | `crates/concord-macros/Cargo.toml` |
| Example façade-only dependency | `crates/concord-examples/Cargo.toml` |
| Runtime endpoint construction | `crates/concord-core/src/request/prepared.rs` |
| Rate-limit semantic constructors | `crates/concord-core/src/rate_limit/types.rs` |
| Endpoint identity | `crates/concord-core/src/endpoint.rs` |
| OAuth2 allocation path | `crates/concord-extras/src/auth/oauth2_client.rs` |
| Runtime timeout | `crates/concord-core/src/runtime/config.rs`, `src/request/head.rs` |
| Macro semantic/source coupling | `crates/concord-macros/src/semantic/model.rs`, `src/emit/mod.rs` |
| Final local recipes | `justfile` |
| Documentation checker scope | `tools/xtask/src/main.rs` |
| Phase 13 claims | `docs/contributor/evidence/refactor-phase-13-docs-tooling.md` |
| Phase 14 failures/deferrals | `docs/contributor/evidence/refactor-phase-14-release.md` |
| Completion ledger | `CONCORD_REFACTOR_REFERENCE.md:34-54` |

## Appendix B — Claims that remain unverifiable from the archive

- exact branch and commit;
- clean Git worktree;
- nothing pushed;
- actual phase commit ancestry;
- any command result recorded in evidence;
- current registry ownership/name availability;
- interactive rust-analyzer behavior;
- Windows/Linux/macOS CI outcomes;
- package behavior against a registry.

These may all be true as reported. They should not be presented as independently audited facts without the required repository/tool evidence.
