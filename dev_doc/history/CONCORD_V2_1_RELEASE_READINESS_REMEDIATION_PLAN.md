# Concord v2.1 — release-readiness remediation and local merge plan

**Status:** authoritative remediation plan for the uploaded `concord_api2` workspace.  
**Objective:** preserve the implemented v2.1 prototype, correct the remaining truth/security/API defects, close the mechanical, cognitive, performance, documentation, and packaging gates, then merge the finished work into the local `main` branch with a merge commit.  
**Non-objective:** another broad v2.2 redesign.

---

# 1. Decision

The current workspace is a strong v2.1 implementation prototype. It should be preserved immediately on a local branch, but it must not be represented as stable `2.1.0` or merged as the authoritative implementation until the remaining audit findings and release gates are genuinely closed.

The work is complete only when all of the following are true:

```text
one active public language
+ one canonical semantic graph
+ one managed executor
+ no known information-flow contradiction
+ explicit pagination completeness
+ executable normative documentation
+ exercised representation inheritance
+ truthful observer/deadline claims
+ measured stream/compile cost
+ resolved human-DX decisions
+ clean-clone/package verification
+ local no-ff merge into main
```

A green `just verify` is necessary but not sufficient if the checks fail to cover a known public or security claim.

---

# 2. Source-of-truth order

Use this order when materials disagree:

1. This remediation plan and the companion implementation prompt.
2. `concord_v2_1_commit_merge_release_audit.md`.
3. `concord_v2_1_workspace_static_evidence.md`.
4. The selected v2.1 language/runtime documents in `docs/` and `dev_doc/`, after they are made internally consistent.
5. `Systems Thinking for Software Engineers, Third Edition`, especially claim typing, evidence matching, negative-space testing, and stop/commit rules.
6. Current code and tests as observed implementation evidence—not as authority to preserve an accidental public behavior.
7. Earlier v2/v2.1 design studies as rationale/history only.

Do not resolve a conflict by silently retaining two forms. Record the selected owner and delete or reject the other form.

---

# 3. Observed starting state to re-check

The uploaded archive was observed with:

- `main` at commit `f7464c5`, ahead of `origin/main` by three local commits;
- a very large uncommitted v2.1 change set;
- workspace version `2.1.0` despite open release gates;
- Gate 3 blocked and Gates 8/10 provisional in `dev_doc/release_gate_status.md`;
- direct static evidence of active parser/docs/API drift.

The implementation agent must re-run `git status`, branch/log/version inspection, and the maintained verification commands before relying on this state.

---

# 4. Non-negotiable v2.1 invariants to preserve

The remediation is convergence, not conceptual replacement. Preserve:

## 4.1 DSL and generated usage

- one client-contained domain tree;
- exact opaque Rust payload types;
- postfix `?` as the sole Concord-owned optional-presence layer;
- independent `#[default]` and `#[sensitive]` axes;
- exact owner-qualified references (`client.x`, named-scope, `endpoint.x`);
- `RequestAtom` for canonical scalar protocol text;
- `OrderedValues` plus explicit `each` for repeated occurrences;
- word-visible patch operations (`when_present`, `append`, `remove`, family `replace`/`off`);
- exact caller body/output types with request/response representation as a separate contract;
- endpoint-owned finite status and effect;
- capability-specific terminals;
- isomorphic direct and unbound generated trees;
- dynamic runtime endpoints remain more explicit and no more authoritative than generated endpoints.

## 4.2 Core runtime

- one executor;
- one absolute operation deadline;
- public shaping before private auth placement;
- monotonic dispatch knowledge;
- separate application retry, auth recovery, transport-private recovery, and pagination continuation;
- body replayability separate from remote effect semantics;
- rate admission before each visible remote attempt and response feedback affecting future admission only;
- pagination as one bounded aggregate with page operations;
- stream acquisition and stream consumption as separate lifecycle boundaries;
- plans/descriptions/observation remain inert and credential-free;
- no native request/response escape.

## 4.3 Security and evidence

- secret/private values never enter plans, general observers, rate keys, debug output, or errors;
- sensitive public values remain behaviorally active but redacted in automatic observations;
- every growing dimension remains bounded;
- unsupported operations remain absent or explicitly rejected;
- claims remain provisional until the appropriate evidence exists.

---

# 5. Git and version-control protocol

The agent is explicitly authorized to create local branches, create local commits, and merge locally into `main`. It is **not** authorized to push, publish crates, create a remote release, or rewrite existing history.

## 5.1 Preserve the current prototype first

From the current dirty tree:

1. Inspect untracked/generated/large files and ensure `target`, temporary benchmark output, installed tools, editor state, and logs are not accidentally committed.
2. Create a preservation branch from the current `main` HEAD while retaining the working tree:

```bash
git switch -c backup/v2.1-prototype-unreviewed
```

3. Run at least:

```bash
git diff --check
git status --short
```

4. Commit the exact prototype checkpoint:

```text
chore: checkpoint Concord v2.1 implementation prototype

Unreviewed release state. Known open areas include active language drift,
response-header observation, pagination coercion, executable docs,
representation-inheritance evidence, stream/compile/DX gates.
Do not tag or publish this commit.
```

5. Create the working branch from that checkpoint:

```bash
git switch -c release/v2.1-readiness
```

Keep the backup branch after the final merge.

## 5.2 Use a prerelease version while gates remain open

Change workspace/public crate version to `2.1.0-rc.1` (or the next local RC if already used) while the release-readiness branch is in progress. Update lockfiles and docs consistently.

Only restore final `2.1.0` in the last release-finalization commit after every mandatory gate passes.

## 5.3 Commit by semantic slice

Preferred commit sequence:

1. `fix!: align v2.1 public language and pagination semantics`
2. `fix: narrow response observation and rate feedback authority`
3. `test: exercise inherited representations in real contracts`
4. `docs: make normative v2.1 examples executable`
5. `fix: make observer deadline claims and evidence truthful`
6. `perf: close stream and large-contract evidence gates`
7. `test: close v2.1 DX and documentation review gates`
8. `chore: finalize Concord 2.1.0 release readiness`

Fewer commits are acceptable if a split would create artificial non-building intermediate states. Each accepted commit should be coherent and pass focused checks.

## 5.4 Merge protocol

Merge only after the definition of done in section 17 is met:

```bash
git switch main
git status --short       # must be clean
git merge --no-ff release/v2.1-readiness \
  -m "merge: finalize Concord v2.1"
```

Then run the complete verification suite on the merged `main` tree. If the post-merge run fails, repair on `release/v2.1-readiness`, merge the repair, and re-run. Do not leave `main` in a known failing state.

Do not delete the release or backup branches. Do not push or tag.

---

# 6. P0 workstream A — establish one active language

## 6.1 Remove endpoint/runtime `accept` overrides

### Problem

The parser, semantic model, emitters, and runtime request builder still contain an endpoint-level `accept` override, while the selected v2.1 model assigns `Accept` negotiation to the response representation.

This creates two public authorities and an undocumented language branch.

### Selected decision

- Remove active DSL `accept ...` and `accept off` syntax.
- Remove its AST/IR/semantic/emitter/description branches.
- Remove public generated/runtime endpoint/request `.accept(...)`/suppression controls.
- Retain only representation-owned `ResponseRepresentation<Output>::accept()` (or its final equivalent) and core application of that representation metadata.
- If a different media contract is needed, the endpoint selects another response representation/adapter.
- Keep old syntax only in migration documentation and compile-fail rejection tests.

### Expected implementation locations

At minimum inspect and update:

```text
crates/concord-macros/src/syntax/{ast,declaration,keyword,diagnostics}.rs
crates/concord-macros/src/semantic/{model,mod,inheritance}.rs
crates/concord-macros/src/emit/{assertions,descriptions,endpoints,shaping}.rs
crates/concord-core/src/request/{prepared,prepare,head}.rs
crates/concord-core/src/__private.rs
crates/concord-core/src/auth/placement.rs
crates/concord/tests and UI snapshots
docs/spec.md, docs/responses.md, docs/migration_v2_1.md
scripts/check_docs_examples.ps1
```

Do not remove representation-owned `accept` metadata by accidental broad search.

### Required evidence

- parser rejects endpoint `accept` with a focused v2.1 diagnostic;
- generated requests expose no public accept override;
- runtime endpoints expose no accept override;
- response representation still sets the expected `Accept` header;
- public header syntax still rejects `Accept` ownership;
- semantic descriptions no longer contain endpoint accept override fields;
- migration audit reports old `accept` syntax without rewriting it incorrectly.

## 6.2 Remove active `clear_*` vocabulary

### Problem

Active façade docs still teach `clear_*` although the canonical generated operation is `omit_*`.

For `x?: Option<T>`, `set_x(None)` and `omit_x()` are intentionally different. “Clear” obscures which layer is being cleared.

### Selected decision

- Active generated APIs and docs use `set_*`, optional `with_*` while provisional, `omit_*`, and `reset_*` only.
- `clear_*` appears only in migration/history/rejection contexts clearly labelled as retired.
- Add a retired-vocabulary check that fails when old vocabulary appears in active façade/docs/examples/generated Rustdoc.

### Required evidence

- active public docs and crate-level docs contain no `clear_*` recommendation;
- compile-fail tests prove no `clear_x()` method exists;
- migration docs still explain `clear_x() -> omit_x()`;
- vocabulary checker distinguishes historical references from active guidance.

## 6.3 Remove implicit numeric pagination

### Problem

`impl From<u32> for PageLimit` and generated `Into<PageLimit>` usage allow:

```rust
request.paginate(10_u32)
```

This silently selects prefix/truncation success and erases the difference between:

```rust
PageLimit::Prefix(10)
PageLimit::CompleteWithin(nonzero!(10))
```

### Selected decision

- Remove `From<u32> for PageLimit` and any public `Into<PageLimit>` terminal parameter.
- `.paginate()` accepts exactly `PageLimit`.
- Update every test/example to state `Prefix` or `CompleteWithin` explicitly.
- Preserve `Prefix(0)` no-work semantics through the named variant.

### Required evidence

- `.paginate(10_u32)` is a compile-fail case on a genuinely paginated endpoint;
- every successful test names completeness semantics;
- docs explain partial-by-request versus complete-or-error;
- no generic conversion route reintroduces numeric coercion.

---

# 7. P0 workstream B — redesign automatic response observation

## 7.1 Problem

`SanitizedHeaders` currently reveals text for headers not identified as sensitive by name. A remote service can place sensitive material in any syntactically valid header, including an unknown custom name, `Location`, or `Link`.

A header-name denylist is defense in depth, not a sound information-classification model.

The automatic surfaces affected include operation events/debug and rate feedback. Explicit caller access through `.response()` has different authority and must not be conflated with automatic observation.

## 7.2 Selected authority model

### Explicit caller response

`ApiResponse<T>::headers()` may continue exposing the actual bounded accepted response headers because the caller explicitly selected the enriched response terminal. Its `Debug`/`Display` must remain sanitized and never print arbitrary values.

### General operation observer/debug

Remove arbitrary response-header values from `OperationObserver` events and built-in debug output.

General response evidence may contain only purpose-specific typed facts such as:

- status;
- content length when safely parsed and bounded;
- coarse media-type category or validated representation identity;
- parsed server delay/not-before fact;
- bounded rate-feedback outcome category;
- operation/page/attempt IDs and timing;
- dispatch knowledge and result category.

Unknown response headers are absent from automatic observation by default. If header names are shown structurally, all values remain redacted and count/size is bounded.

`DebugLevel::Headers` must not mean “print every non-denylisted response header value.” Rename or narrow it if necessary.

### Response representation

A response representation may receive the actual accepted head required for media/contract validation because it is the semantic owner of conversion. Its trait documentation must classify this as trusted response-content authority and prohibit general logging.

### Pagination controller

Keep declared exact response-header interests. Core must:

- validate a finite static interest set;
- reject reserved private/cookie/authorization fields;
- copy only requested bounded values;
- expose no `HeaderMap` or arbitrary iteration;
- preserve cross-origin restrictions for Link-based pagination;
- include sentinel tests for requested and non-requested unknown headers.

### Rate limiter/feedback

Do not pass a generic `SanitizedHeaders` map to the general limiter/observer API.

Minimum preferred design:

1. Core parses standard `Retry-After` once into the existing typed server-delay fact.
2. `RateLimiter::observe` receives a typed `RateLimitFeedback` containing status, server delay, plan identity, attempt context, and bounded outcome fields—not a complete header view.
3. Remove the separate broad `RateLimitObserver`/`RateLimitResponseHead` path if it exists only to inspect headers and generate cooldowns.
4. The first-party local limiter consumes typed server delay directly.
5. If a real first-party provider-specific rate format needs additional headers, introduce a narrow parser with a static finite interest declaration and a core-filtered `RateFeedbackHeaders` view. It must not receive a `HeaderMap`, cannot request forbidden fields, and cannot affect the current result.

Do not solve the leak by expanding the sensitive-name denylist alone.

## 7.3 Required tests

Use unique sentinel values under unknown names, not only familiar secret names.

Prove:

- an unknown header sentinel never appears in operation events, built-in debug, general errors, plans, descriptions, rate keys, or tracing extras;
- `Location`, `Link`, cookies, authorization, and arbitrary `X-*` values are absent/redacted automatically;
- explicit `.response().headers()` can retrieve the actual sentinel while `Debug` cannot reveal it;
- response representations receive only their documented trusted head view;
- pagination receives only declared interests;
- rate feedback receives only typed/declared facts;
- no current accepted/rejected result can be replaced by response observation;
- event/value counts and copied bytes remain bounded;
- panic and blocking obligations remain documented.

## 7.4 Gate consequence

Gate 9 remains reopened until these negative-space tests pass and the public API no longer exposes arbitrary response-header values through automatic observation.

---

# 8. P0 workstream C — prove representation inheritance as a product feature

## 8.1 Problem

Representation inheritance is central to v2.1's reduction of leaf repetition, but active real contracts predominantly use inline `via`. Unit tests can prove nearest-owner resolution; they do not prove that authors/reviewers can discover the effective contract.

## 8.2 Required active examples

Use at least two compiled contracts:

### Real large contract

Migrate a material part of the Riot contract to:

- a client-level `response Json;` default;
- a client- or scope-level `request Json;` where body endpoints make this real;
- leaf endpoints that omit redundant `via Json`;
- one nested response override;
- one request override independent of response;
- one `request explicit;` or `response explicit;` fence in a heterogeneous branch;
- a leaf that fails clearly if a representation is missing after a fence.

Do not add a meaningless fence only to satisfy the test. Choose a real heterogeneous branch or add a dedicated compiled realistic companion contract when Riot cannot honestly express it.

### Focused contract

Add a small `representation_inheritance` fixture that makes every inheritance transition obvious and stable for UI/snapshot tests.

## 8.3 Required projections

For each relevant endpoint, assert:

- effective request representation type;
- effective response representation type;
- declaration owner/provenance;
- local override versus inherited/fenced status;
- caller output/delivery capability;
- accepted-bytes/pagination compatibility;
- generated Rustdoc near the endpoint signature;
- `contract()`/description output;
- semantic diff when a parent representation changes.

A parent change must report every affected leaf, including media/acquisition/capability changes, without requiring reviewers to infer the tree manually.

## 8.4 Gate consequence

Gate 2 may be marked mechanically passed after this work. Its human discoverability remains part of Gate 3 until the task study is complete.

---

# 9. P0 workstream D — make normative documentation executable

## 9.1 Problem

The current docs checker validates structure and a small number of source-anchored examples while allowing many public Rust-looking examples to remain pseudocode. Some can drift from actual generated signatures without failing `just docs`.

## 9.2 Classification rule

Every fenced block must be one of:

- `rust` or `concord` with a source anchor into a compiled fixture;
- `compile_fail`/a UI fixture that fails for the documented reason;
- `text`/`concord-pseudocode` for genuinely non-executable grammar, lifecycle, or conceptual notation, with no claim that it is copy-paste code;
- generated output snapshot compared mechanically.

A normative usage example may not be downgraded to pseudocode merely to make docs checks pass.

## 9.3 Minimum compile-checked set

Compile the canonical examples in:

- root `README.md`;
- `docs/quick_start.md`;
- canonical contract and key grammar examples in `docs/spec.md`;
- `docs/generated_client.md`;
- `docs/requests_and_values.md`;
- `docs/request_bodies.md` and `docs/responses.md`;
- `docs/auth.md`;
- `docs/retry.md` and `docs/rate_limits.md`;
- `docs/pagination.md`;
- `docs/runtime_endpoints.md`;
- representation inheritance/effective-contract docs;
- `docs/migration_v2_1.md`;
- Riot walkthrough.

Use source anchors into actual example/test crates so the docs display precisely what Cargo compiles.

## 9.4 Checker requirements

Extend `scripts/check_docs_examples.ps1` or replace/augment it with `xtask docs-check` so it fails on:

- unanchored normative `rust`/`concord` examples;
- a Rust-looking canonical usage block labelled pseudocode;
- stale retired vocabulary in active docs;
- missing/duplicate anchors;
- source drift;
- dead local links/headings;
- active `accept`, `clear_*`, or numeric `.paginate(...)` examples;
- docs source not included in a compiled target.

Run under default, all-features, and MSRV where relevant.

## 9.5 Gate consequence

Gate 10 remains provisional until the normative examples compile and generated Rustdoc is reviewed in Gate 3/10.

---

# 10. P0 workstream E — make observer/deadline claims truthful

## 10.1 Exact temporal contract

A synchronous observer is a cooperative trusted callback. Core can bound event count and event data and can check time before/after the call. It cannot preempt arbitrary synchronous Rust code.

Required semantics:

- attempt/intermediate observer time consumes remaining operation time after the callback returns;
- if the deadline expired during an intermediate callback, no additional remote work begins and the operation is classified accordingly;
- final operation/stream completion observation runs after semantic outcome fixation;
- a slow final observer may delay delivery beyond the nominal operation deadline but cannot change the fixed result;
- observer panic is contained if that remains the selected implementation, and the containment/diagnostic policy is documented;
- observer implementations must be prompt, finite, nonblocking, and non-panicking;
- asynchronous export belongs to an application-owned bounded handoff, not a Concord detached task.

Remove language claiming a hard preemptible bound over callback execution.

## 10.2 Tests

Prefer deterministic/injected time rather than wall-clock sleeps.

Test:

- intermediate observer advances test time beyond deadline; after return, no transport work starts and timeout is observed;
- final observer advances time after a successful result is fixed; success is still returned after callback;
- observer panic is contained exactly as documented;
- no core lock/private material is held/exposed during callbacks;
- event count remains bounded and pagination emits the intended aggregate/page vocabulary only.

## 10.3 Gate consequence

Gate 5 can be closed after runtime tests and docs/claim ledger use the enforceable wording.

---

# 11. P0 workstream F — rate semantics and gate truth

## 11.1 Whole-plan admission

Document and test whether one `RateLimiter::acquire` receives the complete effective policy plan and must decide it as one admission. The first-party local limiter must not partially debit one policy/window then reject another without a documented conservative-accounting rule.

Required tests:

- conjunctive windows;
- all-or-none local state update for a complete plan where feasible;
- no request dispatch until every effective policy is admitted;
- policy retry/auth recovery/page attempt reacquisition;
- transport-private recovery does not reacquire;
- future-only feedback never replaces current result;
- high-cardinality key pressure and first-excess behavior;
- no arbitrary response headers after workstream B.

## 11.2 Release gate reclassification

Update `dev_doc/release_gate_status.md` and `dev_doc/claim_evidence_ledger.md` after each workstream. Do not mark a gate passed because an adjacent gate is supposed to cover the missing evidence.

Initial audit status:

| Gate | Required status before work |
|---|---|
| 1 representation trait/driver | mechanically passed; reverify |
| 2 representation inheritance | reopened/provisional |
| 3 DSL/generated DX | blocked |
| 4 query/header/atom | conditional on language cleanup |
| 5 runtime trace | conditional on observer truth |
| 6 pagination | reopened |
| 7 rate/server delay | conditional |
| 8 streams | provisional |
| 9 information-flow security | reopened, merge blocker |
| 10 large contract/docs/hygiene | provisional |

A final `Passed` entry must name the exact tests, measurements, or human review that establishes the gate.

---

# 12. P1 workstream G — close Gate 3 with real cognitive evidence

## 12.1 Required study

Create a self-contained study package in `dev_doc/dx_study/` containing:

- participant instructions;
- the tested API/DSL variants;
- task sheets;
- answer rubric;
- timing/error/context-opened/confidence capture;
- anonymized results;
- decision record.

Use at least **three** relevant Rust developers unfamiliar with Concord; five is preferred. The project owner may participate but cannot be the only participant. Independent AI agents may be used as additional cold-read probes but do not replace human evidence.

## 12.2 Tasks

At minimum ask participants to:

1. predict generated types/methods for `x?: Option<T>` and a defaulted optional value;
2. distinguish `set_x(None)` from `omit_x()`;
3. decide whether `with_x(T)` helps or adds ambiguity/noise;
4. add repeated tags and one scalar joined collection;
5. explain assignment, `when_present`, append, and remove under inheritance;
6. find the effective request/response representation and its owner;
7. add an endpoint inside a nested inherited/fenced scope;
8. review a parent representation change affecting descendants;
9. select `Prefix` versus `CompleteWithin` for two scenarios;
10. diagnose one generated compile error;
11. explain `ByteStream` versus `ItemStream<T>` and when request success occurs;
12. identify what an operation observer may and may not see.

Measure correctness, task time, documentation/source opened, corrections, confidence calibration, and recurring misconceptions. Preference alone is secondary.

## 12.3 Decision rule for `with_x`

The current `with_x` method is provisional.

- Keep it only if the study shows a clear ordinary-task benefit and participants do not confuse it with outer omission or exact optional assignment.
- If evidence is mixed or unavailable, choose the smaller stable surface: remove `with_x` and retain exact `set_x(Option<T>)`, `omit_x()`, and `reset_x()`.
- Whichever choice is made must be applied consistently to generated client/scope/request APIs, docs, UI tests, migration, method-count/expansion measurements, and ADR.

## 12.4 Representation inheritance review

Gate 3 must include a human review of generated Rustdoc/effective contract. A participant should be able to locate inherited representation and explain a parent-level change without reading macro/core internals.

## 12.5 External-block rule

Do not fabricate human evidence. If no relevant humans can be reached, all machine work may be completed and committed on the release branch, but Gate 3 remains blocked and the branch must not be merged as stable-ready. Produce the ready-to-run study package and report the single external blocker.

A project-owner waiver may authorize a prerelease merge, but it does not turn the cognitive claim into `Supported` and must not produce final stable `2.1.0` status.

---

# 13. P1 workstream H — close Gate 8 with stream implementation evidence

## 13.1 Compare private strategies

Keep public `ByteStream` and `ItemStream<T>` stable. Compare at least:

1. current private boxed erasure;
2. monomorphized/static strategy;
3. private enum-erased strategy where technically viable.

Use benchmark-only/private implementations; do not publish three APIs.

## 13.2 Workloads

Measure:

- byte streams at small, medium, and large chunk sizes;
- NDJSON/CSV item streams with cheap and moderate decoding;
- stream creation and first item latency;
- steady-state throughput;
- backpressure;
- early drop/cancellation;
- terminal error path;
- allocations at construction and per item/chunk;
- clean compile time, incremental compile time, binary/code size.

## 13.3 Precommitted acceptance budget

Use these default decision thresholds unless baseline noise makes them impossible; record any adjustment before examining final results:

- no per-item/per-chunk heap allocation caused solely by erasure;
- at most one unavoidable allocation at stream construction for boxed erasure;
- steady-state throughput within 10% of the fastest practical strategy for bytes and 15% for decoded items;
- no >15% representative binary-size regression versus the smallest practical strategy without a compensating compile-time or maintainability gain;
- cancellation, error, bounds, and completion behavior identical across strategies.

If current boxed erasure misses the budget, change only the private driver implementation. Do not leak decoder/driver types into public carriers to win a benchmark.

## 13.4 Evidence artifact

Commit a concise `dev_doc/evidence/stream_driver_comparison.md` with environment/toolchain, benchmark code, raw-result location, summary, decision, and limitations. Avoid committing enormous raw benchmark directories.

---

# 14. P1 workstream I — close Gate 10

## 14.1 Clean/incremental build and memory

Use clean local worktrees for the v2.0 baseline commit and the candidate branch where possible. Measure:

- clean `cargo check`/test build wall time and peak memory;
- incremental no-change and one-leaf endpoint edit;
- one parent representation edit affecting many leaves;
- default and all-features;
- MSRV where practical;
- 1/10/100/500/1,000 endpoint fixtures.

Record OS, CPU, memory, Rust/Cargo versions, command, cache state, and repeated-run variance.

## 14.2 Binary/static metadata size

Measure representative binaries/libraries and static contract/description growth. Track generated method count and expanded lines, but do not treat line count alone as cost.

Investigate any superlinear growth or trait-solver/diagnostic blow-up.

## 14.3 IDE/rust-analyzer

On the 500/1,000 endpoint fixtures, record:

- initial analysis time;
- completion/hover latency on representative endpoints;
- memory if available;
- diagnostic quality for one missing representation, one bad atom, and one invalid pagination binding.

## 14.4 Human documentation review

At least one relevant reviewer other than the primary implementer must inspect:

- generated client Rustdoc;
- effective representation provenance;
- optional/default controls;
- status/effect/retry language;
- pagination completeness;
- stream lifecycle;
- security/observer boundary;
- one parent semantic diff.

Record issues and corrections, not only approval.

## 14.5 Documentation gate

All normative examples from workstream D must compile in the final tree, and `cargo doc`/rustdoc with `-D warnings` must pass under the maintained feature matrix.

---

# 15. Clean-clone, packaging, and downstream verification

Before final versioning/merge:

1. Commit every intended branch change.
2. Create a new local clone or clean worktree from the release branch commit.
3. Verify no untracked local tools or build products are required.
4. Run the complete maintained suite, including `just verify`.
5. Run critical commands with `--locked` where applicable.
6. Build documentation with warnings denied.
7. Package every publishable crate locally in dependency order.
8. Run downstream fixture crates against the packaged artifacts/tarballs or a local registry-equivalent setup—not only workspace path dependencies.
9. Run `cargo publish --dry-run` only if useful and guaranteed not to publish; never run an actual publish.
10. Verify migration tooling is read-only unless an explicit output path is supplied.
11. Run retired-vocabulary, placeholder, unsafe, generated-artifact, and `git diff --check` scans.

Record exact command lines, tool versions, and exit codes in `dev_doc/release_verification_2_1.md`.

---

# 16. Final version and documentation status

Only after all Gates 1–10 are closed:

- set workspace/public crates to stable `2.1.0`;
- update lockfiles;
- update README, migration, SemVer/stability policy, and release-gate table;
- remove words such as prototype, provisional, blocked, candidate, and RC from active release claims where evidence actually resolved them;
- keep historical ADR/ledger entries that explain prior provisional status;
- create the final branch commit:

```text
chore: finalize Concord 2.1.0 release readiness
```

Do not create a tag or publish remotely unless separately requested.

---

# 17. Definition of done

## 17.1 Public language

- no active endpoint/runtime `accept` override;
- no active `clear_*` guidance or method;
- no implicit numeric pagination conversion;
- one documented syntax per semantic action;
- old forms appear only in migration/rejection/history;
- parser, IR, emitters, generated API, descriptions, semantic diff, docs, and tests agree.

## 17.2 Information flow

- general observers/debug/errors do not expose arbitrary response-header values;
- explicit caller response access remains deliberate and Debug-safe;
- pagination/rate/representation receive only purpose-specific authority;
- unknown-header sentinel tests pass;
- Gate 9 is honestly passed.

## 17.3 Representation and docs

- real compiled multi-scope inheritance, override, and fence examples exist;
- effective provenance appears in Rustdoc/description/diff;
- every normative public example compiles or intentionally compile-fails;
- pseudocode is clearly conceptual;
- documentation checks fail on future drift.

## 17.4 Runtime

- observer timing contract is enforceable and accurately documented;
- one executor/deadline/dispatch model remains intact;
- pagination requires explicit `PageLimit` semantics;
- rate feedback is future-only and typed;
- stream strategies preserve lifecycle and meet selected budget.

## 17.5 Evidence

- Gate 3 has actual unfamiliar-human evidence or remains blocked with no stable merge;
- Gate 8 comparative stream evidence is committed;
- Gate 10 compile/binary/IDE/Rustdoc evidence is committed;
- all ten gates say `Passed` with exact evidence links;
- no public method/type remains intentionally provisional.

## 17.6 Repository and merge

- branch history consists of coherent local commits;
- release branch is clean;
- clean-clone `just verify` passes;
- local packaging/downstream checks pass;
- stable version is `2.1.0` only after gates close;
- `release/v2.1-readiness` is merged with `--no-ff` into local `main`;
- post-merge `just verify` passes on `main`;
- no remote push, publication, or tag was made;
- backup and release branches remain available locally.

---

# 18. Final report required from the implementation agent

The final response must include:

## Git outcome

- branch names;
- starting and final commit IDs;
- checkpoint, remediation, finalization, and merge commit IDs;
- confirmation that no push/tag/publish occurred;
- final `git status --short --branch` for `main`.

## Changes by system

- DSL/generated usage;
- core execution and information flow;
- rate/auth/pagination/streams;
- extras;
- docs/tooling/evidence.

## Gate table

For each Gate 1–10:

- final status;
- claim established;
- exact evidence files/tests/commands;
- residual limitation, if any.

## Verification

List every command actually run, toolchain version, and result. Do not summarize an unrun command as passed.

## Human/performance evidence

- DX participant count/profile/tasks/result and final `with_x` decision;
- stream comparison results and selected private strategy;
- clean/incremental/size/IDE/Rustdoc results.

## Deviations

Any deviation from this plan must state:

- why the prescribed path was impossible or inferior;
- which property was preserved;
- tests/evidence supporting the alternative;
- whether the gate remains closed.

## Final claim

Use exactly one truthful outcome:

```text
MERGED: Concord v2.1 is locally merged into main and all release-readiness gates passed.
```

or:

```text
NOT MERGED: the release branch is preserved; the following external or technical gate remains open: ...
```

Do not use the word “ready” when a mandatory gate remains provisional, waived, or unrun.
