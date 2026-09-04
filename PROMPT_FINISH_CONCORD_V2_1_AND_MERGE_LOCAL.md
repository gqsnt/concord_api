# Implementation prompt — finish Concord v2.1, close every release gate, and merge locally into `main`

Use this prompt from the root of the uploaded `concord_api2` workspace.

The repository and these source documents are available:

```text
CONCORD_V2_1_RELEASE_READINESS_REMEDIATION_PLAN.md
concord_v2_1_commit_merge_release_audit.md
concord_v2_1_workspace_static_evidence.md
Systems Thinking for Software Engineers, Third Edition
current docs/, dev_doc/, tests, ADRs, and release-gate ledger
```

---

# 0. Role and mandate

Act as the lead Rust library maintainer, DSL/API designer, runtime/security reviewer, release engineer, and evidence owner for Concord.

Your task is **not** to produce another design report. Your task is to modify the repository until Concord v2.1 is genuinely ready, commit the work in coherent local commits, and merge the completed branch into the local `main` branch.

You are explicitly authorized to:

- inspect and modify every file in the local repository;
- create local backup, feature, release, and evidence branches;
- create as many local commits as needed;
- install workspace-local development tools when justified;
- add tests, benchmarks, fixtures, scripts, docs, ADRs, and evidence artifacts;
- remove obsolete public syntax/API/code;
- change provisional v2.1 implementation details while preserving the selected semantic model;
- merge the finished release-readiness branch into local `main` with a merge commit.

You are **not** authorized to:

- push any branch or commit to a remote;
- publish crates;
- create a remote release;
- create a release tag;
- force-reset, force-push, rewrite, or discard existing history;
- hide a failing or unavailable gate behind a waiver you invented;
- claim human, performance, security, or mechanical evidence that was not actually obtained.

Interpret the user's phrase “until v1 ready” as **until v2.1 is ready**.

---

# 1. Source-of-truth precedence

When sources conflict, use this order:

1. `CONCORD_V2_1_RELEASE_READINESS_REMEDIATION_PLAN.md`.
2. `concord_v2_1_commit_merge_release_audit.md`.
3. `concord_v2_1_workspace_static_evidence.md`.
4. The selected v2.1 semantic model documented in the current repository, after contradictions are removed.
5. Systems Thinking for Software Engineers, Third Edition—especially claim states, evidence matching, negative-space testing, and stop/commit rules.
6. Current code/tests as observed implementation evidence, not as automatic authority to preserve an accidental behavior.
7. Older v2/v2.1 design studies as rationale/history only.

Do not silently reconcile contradictions by supporting both forms. Select the correct owner/meaning, delete or reject the conflicting form, and update every projection.

---

# 2. Success condition

The work is complete only when:

```text
all mandatory P0 corrections are implemented
+ Gates 1–10 are honestly closed
+ cognitive decisions have real unfamiliar-human evidence
+ stream/compile/docs costs have measured evidence
+ every normative example compiles or intentionally fails
+ a clean clone/worktree passes the complete suite
+ local packages work outside workspace path dependencies
+ stable version is 2.1.0
+ release branch is locally merged --no-ff into main
+ post-merge main passes the complete suite
```

A green `just verify` is necessary but not sufficient when the checker does not cover a known public/security claim.

Do not stop after fixing only the audit's P0 list. Continue through human-DX, stream-performance, large-contract/docs, packaging, and clean-clone gates.

The only permissible non-merged stopping outcome is a genuinely external blocker that cannot be fabricated—most notably unavailable human participants. In that case, finish and commit every other workstream, create the ready-to-run study package, leave the release branch clean and preserved, and report the single exact blocker. Do not merge stable `2.1.0` under that condition.

---

# 3. Preserve the existing prototype safely

The uploaded archive was observed with `main` at `f7464c5`, ahead of `origin/main`, and a very large uncommitted v2.1 tree. Re-check rather than assuming this is unchanged.

## 3.1 Baseline inspection

Run and record:

```bash
git status --short --branch
git branch -vv
git log --oneline --decorate -15
git diff --stat
git diff --check
cargo --version
rustc --version
rustup show
just --list
```

Run the available baseline maintained checks before semantic edits. If they are too expensive to run all before the checkpoint, run focused format/check/tests and record why, then run the full suite after the checkpoint.

Inspect untracked and modified files. Exclude build output, installed tools, logs, editor state, benchmark raw directories, and other generated artifacts that do not belong in source control.

## 3.2 Create an exact local checkpoint

Create a branch while retaining the current dirty tree:

```bash
git switch -c backup/v2.1-prototype-unreviewed
```

Commit the current intended prototype as:

```text
chore: checkpoint Concord v2.1 implementation prototype

Unreviewed release state. Known open areas include active language drift,
response-header observation, pagination coercion, executable docs,
representation-inheritance evidence, stream/compile/DX gates.
Do not tag or publish this commit.
```

Then create the working branch:

```bash
git switch -c release/v2.1-readiness
```

Do not delete the backup branch.

## 3.3 Use prerelease version while work remains

Change workspace/public crate version to `2.1.0-rc.1` while any gate is open. Update `Cargo.lock` and active docs. Use a later RC number if `rc.1` is already present in history.

Restore stable `2.1.0` only in the final release-readiness commit after every mandatory gate passes.

---

# 4. Preserve these selected v2.1 properties

Do not reopen a broad v2.2 design. Preserve:

- one client-contained domain tree;
- exact opaque Rust payload types;
- postfix `?` for Concord presence, independent of `Option<T>`;
- `#[default]` and `#[sensitive]` as independent axes;
- explicit `client.x`, named-scope, and `endpoint.x` ownership;
- `RequestAtom`, placement adapters, `OrderedValues`, and explicit `each`;
- word-visible query/header assignment, `when_present`, `append`, `remove`, `replace`, and `off`;
- exact caller body/output types;
- directional request/response representations with inheritance, local override, and explicit fences;
- endpoint-owned status and `safe`/`idempotent`/`non_idempotent` effect;
- separate `ByteStream`, `ItemStream<T>`, and request-stream lifecycle types;
- one executor and one absolute operation deadline;
- public-before-private auth placement;
- versioned credential slots and one bounded challenge recovery;
- separate application retry, auth recovery, and transport-private recovery;
- whole-plan rate admission and future-only response feedback;
- pagination as a safe complete-output aggregate under one deadline;
- direct/unbound equivalence;
- runtime endpoints using the same executor under stricter dynamic validation;
- plans/descriptions/diff/observation without native/private authority;
- extras as semantic implementations rather than transport plugins.

Do not regain convenience by reintroducing a native request builder, broad middleware, hidden type-name recognition, implicit cardinality, implicit pagination semantics, or arbitrary response-header observation.

---

# 5. Mandatory P0 corrections before an authoritative merge

Implement every item below as a complete vertical slice: parser/API/core/docs/tests/fixtures/semantic descriptions/migration must agree.

## 5.1 Delete active endpoint and runtime `accept` override

The selected v2.1 owner of `Accept` is the response representation.

Remove:

- DSL `accept ...` and `accept off` parsing;
- AST/semantic/effective-contract fields for endpoint accept override;
- generated shaping/description/assertion branches;
- public generated request controls;
- public runtime endpoint/request `.accept(...)` and suppression controls;
- active documentation and examples.

Retain:

- representation-owned `ResponseRepresentation<Output>::accept()` or its final equivalent;
- core logic that applies representation negotiation;
- old spelling only in migration and focused rejection tests.

Add tests proving:

- endpoint `accept` is rejected with a focused migration diagnostic;
- no public runtime/generated override exists;
- representation negotiation still sends the expected header;
- generic public headers cannot claim `Accept`;
- descriptions and semantic diff contain only representation-owned negotiation.

Do not accidentally remove representation media metadata through a broad textual deletion.

## 5.2 Purge active `clear_*` vocabulary

Active public API/docs use:

```text
set_*
with_* only while Gate 3 decides it
omit_*
reset_*
```

`clear_*` may appear only in migration/history/rejection contexts explicitly labelled retired.

Update at least crate-level façade docs, generated docs, examples, public tests, and source anchors. Add a retired-vocabulary check that fails on active `clear_*`, `accept`, numeric pagination examples, and other removed v1/v2 spellings while allowing migration tables and compile-fail fixtures.

Add a compile-fail test proving `clear_x()` is unavailable.

## 5.3 Require explicit pagination completeness

Remove `impl From<u32> for PageLimit` and every `Into<PageLimit>` terminal shape.

`.paginate()` must accept exactly:

```rust
PageLimit::Prefix(n)
PageLimit::CompleteWithin(non_zero_n)
```

Update every successful test/example. Keep `Prefix(0)` no-work semantics through the named variant.

Add a compile-fail test on a valid paginated endpoint for `.paginate(10_u32)`.

## 5.4 Close the response-header information-flow defect

Do not rely on a sensitive-header-name denylist to decide which arbitrary remote values are safe.

### General operation observation/debug

Remove arbitrary response-header values from `OperationObserver` events and built-in debug output. Expose only typed, purpose-specific facts:

- status;
- safely parsed bounded content length;
- representation/media category where already validated;
- server delay/not-before;
- rate feedback outcome category;
- operation/page/attempt IDs, timing, dispatch knowledge, and final category.

Unknown response headers are absent by default. If names are shown structurally, values are always redacted and counts/bytes are bounded.

`DebugLevel::Headers` must not reveal every non-denylisted response value. Rename/narrow it as needed.

### Explicit caller response

Keep `ApiResponse<T>::headers()` only as deliberate caller authority. Its `Debug`/`Display` remains sanitized and must not print arbitrary values.

### Response representation

A response representation may receive the accepted head required for its contract; document it as trusted conversion authority, not general observation.

### Pagination

Continue using a bounded static response-header interest list. Reject forbidden/private interests and expose only requested bounded values through a narrow view, never a `HeaderMap`.

### Rate feedback

Remove generic response-header maps from rate-limiter observation.

Preferred minimum:

- core parses `Retry-After` once;
- `RateLimiter::observe` receives typed status/server-delay/plan/attempt feedback;
- remove the separate broad `RateLimitObserver` path when it only exists to inspect headers;
- make the local limiter consume typed server delay directly;
- if a real provider-specific rate format needs extra fields, introduce a finite static interest declaration and filtered `RateFeedbackHeaders` view, not arbitrary iteration or a `HeaderMap`.

Do not “fix” this by expanding the name denylist alone.

### Security tests

Use unknown-name sentinels and prove they never appear in:

- general observer events;
- built-in debug/tracing;
- errors/source formatting;
- plans/descriptions/diff;
- rate keys/feedback;
- pagination identity errors.

Also prove:

- `.response().headers()` can explicitly retrieve the actual sentinel;
- its Debug cannot reveal it;
- pagination sees only declared interests;
- rate feedback sees only typed/declared data;
- current results cannot be replaced by observation.

Gate 9 remains open until these tests pass.

## 5.5 Exercise representation inheritance in real active contracts

Migrate a material multi-scope contract—prefer Riot—to use real client/scope request and response defaults instead of spelling `via Json` on nearly every leaf.

The active compiled corpus must include:

- client-level response default;
- request default where body endpoints make it honest;
- nested response override;
- request override independent of response;
- a real or realistic `request explicit`/`response explicit` fence;
- endpoints inheriting, overriding, and failing after a fence;
- provenance in generated Rustdoc and `contract()`;
- semantic-diff tests for a parent representation change;
- capability/media/acquisition changes reported for affected leaves.

Add a focused fixture as well as the large real contract.

Gate 2 is mechanically closed only after this. Human discoverability remains Gate 3.

## 5.6 Compile every normative public example

Every code block must be classified:

- compiled `rust`/`concord` source anchor;
- intentional compile-fail/UI fixture;
- explicitly conceptual `text`/`concord-pseudocode`;
- generated snapshot.

Do not relabel normative usage as pseudocode to avoid compilation.

At minimum compile source-anchored examples for:

- README and quick start;
- canonical spec contract;
- client construction and optional/default controls;
- body/result/representation inheritance;
- auth;
- retry/rate;
- pagination;
- streams/accepted bytes;
- runtime endpoints;
- migration;
- Riot walkthrough.

Extend `scripts/check_docs_examples.ps1` or add `xtask docs-check` so future signature/vocabulary drift fails `just docs` under maintained feature/MSRV matrices.

## 5.7 Correct observer/deadline truth

A synchronous callback is not preemptible by an async deadline.

Document and implement exactly:

- core bounds event data/count and checks time before/after callbacks;
- intermediate observer time consumes the remaining operation budget after return;
- if deadline expired during an intermediate callback, no further remote work starts;
- final completion observation occurs after semantic outcome fixation;
- a slow final callback may delay delivery beyond the nominal deadline but cannot change the fixed result;
- panic containment behavior is explicit and tested;
- observers must be prompt, finite, nonblocking, and non-panicking;
- async export is application-owned bounded handoff, not a core detached task.

Use deterministic injected time for a deliberately slow callback test. Do not claim preemption.

## 5.8 Reclassify gates and rate semantics honestly

Update the gate status and claim ledger as work is completed. Gate 7 must document whole-plan admission and multi-policy debit semantics. The first-party local limiter must have deterministic tests for conjunctive admission and no accidental partial debit, or the conservative behavior must be explicitly documented and evidenced.

Do not mark any P0-dependent gate passed before its negative-space tests exist.

---

# 6. Close Gate 3 — real DSL/generated-API comprehension evidence

Create `dev_doc/dx_study/` with instructions, variants, tasks, rubric, anonymized results, and decision record.

Use at least three Rust developers unfamiliar with Concord; five is preferred. The primary implementer cannot be the only participant. Independent AI cold reads may supplement but not replace humans.

Tasks must cover:

1. `x?: Option<T>` and defaulted optional generated types/methods;
2. `set_x(None)` versus `omit_x()`;
3. whether `with_x(T)` materially helps or causes noise/ambiguity;
4. scalar versus `each` repeated collection placement;
5. assignment versus `when_present` versus append/remove;
6. locating inherited request/response representation and provenance;
7. adding an endpoint under a nested override/fence;
8. reviewing a parent representation semantic diff;
9. selecting `Prefix` versus `CompleteWithin`;
10. diagnosing a generated compiler error;
11. explaining `ByteStream` versus `ItemStream<T>` lifecycle;
12. stating observer information/authority boundaries.

Measure correctness, completion time, files/docs opened, corrections, misconceptions, and confidence calibration. Do not use preference alone.

### Freeze `with_x`

- Keep it only if evidence shows clear task benefit without confusion over outer omission/exact optional assignment.
- If evidence is mixed or unavailable, remove it and keep the smaller exact surface: `set_x(Option<T>)`, `omit_x()`, `reset_x()`.
- Apply the decision everywhere and record an ADR.

If no human participants can actually be reached, finish every other task and produce the ready study package, but do not claim Gate 3 passed and do not merge stable-ready v2.1 into `main`.

---

# 7. Close Gate 8 — comparative stream evidence

Keep public `ByteStream` and `ItemStream<T>` unchanged while comparing private implementations:

- current boxed erasure;
- monomorphized/static;
- private enum erasure where viable.

Measure:

- construction/first item latency;
- allocations at construction and per item/chunk;
- byte stream throughput at several chunk sizes;
- NDJSON/CSV item throughput;
- backpressure and early drop;
- terminal error/cancellation evidence;
- clean/incremental compile time;
- binary/code size.

Precommit these default budgets before final measurements:

- no per-item/per-chunk allocation solely due to erasure;
- at most one unavoidable boxed allocation at construction;
- byte throughput within 10% of fastest practical strategy;
- item throughput within 15%;
- no >15% binary-size regression versus the smallest practical strategy without a documented compensating gain;
- identical lifecycle, bounds, errors, and completion semantics.

If boxed erasure misses the budget, change only private drivers. Do not expose decoder types publicly.

Commit `dev_doc/evidence/stream_driver_comparison.md` with environment, raw-result location, summary, decision, and limitations.

---

# 8. Close Gate 10 — scale, compile, docs, and human review

Use clean worktrees for the v2.0 baseline and final candidate where possible.

Measure and record:

- clean check/build/test wall time and peak memory;
- no-change incremental build;
- one leaf edit;
- one parent representation edit affecting many leaves;
- default/all-features/MSRV;
- binary and static-description size;
- 1/10/100/500/1,000 endpoint fixtures;
- rust-analyzer initial analysis, completion/hover, representative diagnostics, and memory where available;
- method count and expansion size without treating line count as the only cost.

At least one relevant reviewer other than the primary implementer must inspect generated Rustdoc and a parent semantic diff. Record problems found and fixes made.

All normative documentation examples must compile in the final tree, and rustdoc must pass with warnings denied across the maintained matrix.

Commit a concise evidence report; do not commit huge raw build/benchmark directories.

---

# 9. Clean-clone, packaging, and downstream verification

After all fixes and decisions are committed on `release/v2.1-readiness`:

1. Create a clean local clone/worktree at the exact candidate commit.
2. Run the complete maintained suite including `just verify`.
3. Run critical Cargo commands with `--locked`.
4. Run default/no-default/all-feature/MSRV matrices.
5. Build rustdoc with `-D warnings`.
6. Run UI/compile-fail, fixtures, analyzer, deny, workflow, docs, benchmarks, migration audit, retired-vocabulary, placeholder, unsafe, and `git diff --check` checks.
7. Locally package publishable crates in dependency order.
8. Test downstream fixtures against packaged artifacts rather than workspace paths alone.
9. Run `cargo publish --dry-run` only when guaranteed not to publish and useful for package validation.
10. Record toolchain versions, commands, and exit codes in `dev_doc/release_verification_2_1.md`.

No result from the dirty development worktree substitutes for this clean-clone evidence.

---

# 10. Finalize version and release status

Only after Gates 1–10 all have genuine evidence:

- set workspace/public versions to stable `2.1.0`;
- update lockfiles;
- update README, migration, stability/SemVer policy, claim ledger, ADR status, and release gates;
- remove active “prototype”, “provisional”, “blocked”, “candidate”, and RC wording where evidence actually resolved the claim;
- leave historical records intact where they explain the design process;
- ensure no public method/type remains intentionally provisional;
- commit:

```text
chore: finalize Concord 2.1.0 release readiness
```

Do not tag or publish.

---

# 11. Required focused and aggregate checks

Run focused checks after each slice. Before final merge, run at least the maintained equivalents of:

```text
just fmt
just check
just lint
just test
just ui
just msrv
just fixtures
just docs
just deny
just workflow
just analyzer
just bench
just verify
cargo test --workspace --all-features --locked
cargo rustdoc -p concord --all-features -- -D warnings
cargo rustdoc -p concord-extras --all-features -- -D warnings
cargo run -p concord-xtask -- migration-audit
cargo run -p concord-perf --release --bin expansion_scale
cargo bench -p concord-perf --bench concord_overhead
git diff --check
```

Adapt command spelling to the repository, but do not silently omit a maintained gate. Record unavailable commands and fix the environment when reasonably possible.

---

# 12. Commit and local merge

Preferred semantic commits:

```text
fix!: align v2.1 public language and pagination semantics
fix: narrow response observation and rate feedback authority
test: exercise representation inheritance in real contracts
docs: make normative v2.1 examples executable
fix: make observer deadline claims and evidence truthful
perf: close stream and large-contract evidence gates
test: close v2.1 DX and documentation review gates
chore: finalize Concord 2.1.0 release readiness
```

Before merging, ensure:

```bash
git status --short --branch
```

is clean on the release branch and the exact candidate commit passed clean-clone verification.

Then:

```bash
git switch main
git status --short
git merge --no-ff release/v2.1-readiness \
  -m "merge: finalize Concord v2.1"
```

Run `just verify` again on merged `main`. If it fails, repair on the release branch, commit, merge the new repair commit, and re-run.

Keep both local branches:

```text
backup/v2.1-prototype-unreviewed
release/v2.1-readiness
```

Do not push, tag, or publish.

---

# 13. Definition of done

Do not merge until every statement is true:

## Language and API

- one active v2.1 language;
- no endpoint/runtime accept override;
- no active clear vocabulary;
- no numeric PageLimit coercion;
- `with_x` is evidence-resolved and frozen;
- parser, IR, generated API, descriptions, docs, migration, and tests agree;
- retired syntax is rejected with focused diagnostics.

## Security and observation

- automatic observers/debug/errors cannot reveal arbitrary response-header values;
- explicit response header access is deliberate and Debug-safe;
- rate/pagination/representation receive narrow purpose-specific views;
- unknown-header sentinel matrix passes;
- no observer can alter a result;
- Gate 9 is supported, not merely asserted.

## Representations and docs

- Riot/real contract exercises inherited defaults, overrides, and fences;
- provenance appears in docs/description/diff;
- every normative example compiles or intentionally fails;
- Rustdoc/effective-contract review has non-author human evidence.

## Runtime and performance

- observer timing language matches enforceable behavior;
- one executor/deadline/retry/auth/rate/pagination model remains intact;
- explicit pagination completeness is enforced;
- stream strategy meets selected budget;
- clean/incremental/size/IDE measurements are acceptable and recorded.

## Release mechanics

- all Gates 1–10 say `Passed` with exact evidence;
- clean-clone `just verify` passes;
- packages and downstream package fixtures pass;
- stable version is `2.1.0` only after gates close;
- release branch is locally merged with `--no-ff`;
- post-merge `main` is clean and passes `just verify`;
- no remote action occurred.

---

# 14. Final response format

Report:

## Outcome

Use exactly one:

```text
MERGED: Concord v2.1 is locally merged into main and all release-readiness gates passed.
```

or, only for a genuine unresolvable external gate:

```text
NOT MERGED: release/v2.1-readiness is clean and preserved; the following mandatory gate remains externally blocked: ...
```

## Git

- starting branch/commit/status;
- backup branch and checkpoint commit;
- release branch commits;
- final candidate commit;
- merge commit;
- final `main` status;
- confirmation of no push/tag/publish.

## Changes

Summarize by:

- DSL/generated usage;
- core execution/information flow;
- auth/rate/pagination/streams;
- extras;
- docs/tooling/evidence.

## Gate table

For Gates 1–10 include status, claim, exact evidence, and residual limitation.

## Verification

List every command actually run with exact result and toolchain version. Do not state an unrun command passed.

## Human and performance evidence

- participants/tasks/results and `with_x` decision;
- stream strategy measurements/decision;
- compile/binary/IDE/Rustdoc measurements/review.

## Deviations

For every deviation from this prompt state why, which property remains, and what evidence supports the alternative.

Do not end with another redesign proposal. The finished implementation, evidence, commits, and local merge are the deliverable.
