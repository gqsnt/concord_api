# Execution Prompt — Concord Refactor Closure

You are the implementation agent responsible for closing the remaining Concord v2 refactor gaps. Act as a senior Rust library architect, runtime boundedness/security reviewer, proc-macro compiler engineer, public API designer, test architect, release engineer, documentation owner, and developer-experience reviewer.

You are working directly in the Git repository. This is an implementation assignment, not another top-down redesign.

## Authoritative inputs

At the repository root, read these files in full before editing:

1. `CONCORD_REFACTOR_CLOSURE_PLAN.md` — authoritative remaining-work plan;
2. `CONCORD_REFACTOR_AUDIT.md` — source findings and severity;
3. `CONCORD_REFACTOR_REFERENCE.md` — original product thesis, architecture, invariants, and completed design;
4. current manifests, source, tests, workflow files, evidence, and Git history.

`CONCORD_REFACTOR_CLOSURE_PLAN.md` overrides the original Phase 0–14 sequence for work that remains. The original reference remains authoritative for the accepted product architecture and lifecycle invariants.

After every context compaction, interruption, or handoff, re-read:

- Closure Plan Sections 1–5;
- the complete active closure phase;
- the closure progress ledger;
- all prior closure evidence and deviations.

Do not rely on remembered conversation context or the previous completion summary.

---

# Mission

Close only the remaining gaps:

1. incrementally bound runtime endpoint construction;
2. hard-bound rate-limit semantic structures and governor preparation;
3. bound OAuth2 configuration/encoding and Concord-owned formatted capture;
4. remove panic-prone endpoint ID conversion;
5. rename runtime-only timeout configuration truthfully;
6. complete macro semantic normalization;
7. remove public `dangerous-dev-tools` and the normal test-support dependency;
8. make product packages technically package-verifiable;
9. add executable CI;
10. add a passing MSRV semantic diagnostic owner;
11. execute or honestly retain rust-analyzer/platform external blockers;
12. correct validation, README, links, evidence, and completion claims.

Do not redo completed crate, DSL, generated API, example, fixture, or documentation architecture.

---

# Frozen architecture

Preserve:

```text
concord ───────────► concord-core
   ├──────────────► concord-macros
   └──────────────► concord-extras ───► concord-core

concord-examples ─► concord
```

Preserve:

- façade-only ordinary user imports;
- extras-agnostic macros;
- JSON/CSV/NDJSON/pagination/governor/OAuth2 in extras;
- named request wrappers with inherent methods;
- named client builders;
- DSL v2;
- scope-oriented generated modules;
- endpoint descriptions and safe planning;
- `RuntimeEndpoint`;
- one managed executor;
- importable examples;
- consolidated fixtures;
- current documentation learning journey.

Do not reintroduce:

- two-crate application setup;
- direct generated core paths;
- generated request extension traits or preludes;
- PascalCase source identifiers;
- fixed clause ordering;
- body-name inference;
- macro-special extras types;
- positional secrets/providers;
- alternate transport/executor paths;
- runtime pagination.

---

# Non-negotiable invariants

Never weaken:

1. inert endpoint/request construction;
2. immutable client snapshots;
3. one executor for all endpoint forms;
4. private native transport and authentication material;
5. seeded redaction across plans/descriptions/errors/hooks/completion/Debug/Display;
6. finite bodies, responses, streams, status capture, records, identities, pagination, limiter state, cooldowns, provider HTTP, and runtime construction;
7. first-excess withholding;
8. deterministic lifecycle ordering;
9. one additional authentication recovery attempt;
10. sequential cycle-detecting all-or-error pagination;
11. fixed-result exactly-once completion;
12. cancellation without detached correctness work;
13. sealed response acquisition;
14. dangerous capabilities remain explicit and bounded.

If an implementation convenience conflicts with an invariant, preserve the invariant and record a deviation from the convenience.

---

# Working rules

1. Begin with Closure Phase C0.
2. Execute C0 through C8 in order.
3. Do not start a later phase before the current phase’s code, tests, evidence, and exit criteria are complete.
4. Make one intentional commit per phase. A phase may use two commits only when the closure plan explicitly permits it or a deviation explains why.
5. Do not push.
6. Update the closure ledger after every phase.
7. Add evidence under `docs/contributor/evidence/`.
8. Report only commands actually run.
9. Record unavailable tools/platforms/editor sessions as unexecuted.
10. Do not claim a clean worktree without `git status --short` and `git diff --exit-code`.
11. Use `--locked` for correctness validation.
12. Do not run fixture/public-compile processes concurrently on Windows when they can target the same executable.
13. Never blanket-accept Trybuild output. Inspect every changed snapshot.
14. Stable owns exact Trybuild snapshots; MSRV owns semantic JSON-diagnostic checks.
15. Do not invent license, repository URL, registry ownership, or package name.
16. Do not use external network calls in deterministic tests.
17. Keep errors and evidence free of secrets, rate keys, auth values, and protected URLs.
18. Search for all call sites before changing a public constructor or feature.
19. Update migration/changelog for every intentional breaking closure change.
20. If the current source differs from the audit, verify whether the issue is already fixed and record evidence rather than reimplementing it.

---

# Binding implementation choices

Follow the closure plan’s selected decisions:

- keep fluent `RuntimeEndpointBuilder` methods and use an error-latching state;
- stop iterator consumption at the first excess;
- add core-owned canonical structural limits;
- implement one capped `fmt::Write` capture primitive;
- make public rate-limit structures fallible and incremental;
- validate limiter preparation limits against a hard cap;
- expose provider request-body limit semantically;
- validate/encode OAuth2 with checked lengths and no large duplicate intermediates;
- remove `From<&'static str> for EndpointId`;
- rename `operation_timeout` to `runtime_endpoint_timeout`;
- remove production `emit → syntax` dependencies;
- remove `dangerous-dev-tools` and the normal product dependency on test support;
- preserve benchmark coverage through a hidden support bridge with no test-support product edge;
- add stable exact UI plus MSRV semantic UI;
- add and lint `.github/workflows/ci.yml`;
- separate local, MSRV, package, and release validation scopes;
- keep unresolved external release/editor decisions explicit.

Do not replace these choices with easier but weaker behavior unless a deviation document proves the constraint.

---

# Phase execution

## C0 — Truth reset

- inspect Git, tools, workflow, package, MSRV, and editor state;
- correct overstated ledger statuses;
- create baseline evidence;
- make no product-code changes.

## C1 — Bounded foundation

- add checked budgets;
- add capped formatting;
- establish canonical public limits;
- add first-excess/no-overconsumption primitives and tests.

## C2 — Runtime endpoints

- error-latching builder;
- incremental path/query/header/auth/rate accounting;
- no infinite iterator consumption;
- preserve inert build, planning, and shared execution.

## C3 — Rate limits

- hard structural caps;
- fallible incremental key/rule/plan constructors;
- generated/runtime same validation;
- bounded governor preparation;
- redacted first-excess tests.

## C4 — OAuth2 and formatting

- provider request-limit accessor;
- bounded OAuth2 builder inputs;
- checked percent/Base64 output lengths;
- single final allocations;
- capped generated/public-head formatted capture.

## C5 — API/macro cleanup

- remove panic-prone endpoint ID conversion;
- rename runtime timeout setting;
- normalize syntax into semantic enums before emit;
- remove obsolete bridge helpers;
- mechanically own exact limits in docs.

## C6 — Package graph

- remove public dev-tools feature;
- remove normal test-support dependency;
- preserve performance probes internally;
- add local package verification;
- retain external legal/name blockers.

## C7 — Validation/docs

- add GitHub Actions workflow;
- add actionlint/policy checks;
- add locked Just/xtask scopes;
- implement passing MSRV semantic UI;
- execute automated rust-analyzer checks;
- record manual editor status honestly;
- fix README, links, evidence, and validation claims.

## C8 — Final certification

- run complete local proof matrix;
- compare performance;
- verify package graph;
- verify clean Git state;
- record exact commit and external blockers;
- do not push.

The detailed tasks, commands, exit criteria, and risks are in the closure plan. Use them directly.

---

# Required evidence discipline

For every phase create the named evidence file from the closure plan and include:

```text
Phase:
Started from commit:
Finished at commit:
Files changed:
Design decisions:
Audit findings closed:
Tests added/changed:
Commands executed:
Exact results:
Unexecuted checks:
Security/redaction review:
Performance impact:
Deviations:
Remaining blockers:
```

Never write “PASS” for an unexecuted command.

When a command fails:

- preserve the relevant output or concise excerpt;
- identify whether it is an expected baseline failure, code defect, environment limitation, or external blocker;
- fix code defects before closing the phase;
- do not recategorize a defect as a deferral merely to finish.

---

# Completion standard

You may report **“Refactor closure complete”** only if:

- C1–C7 are complete;
- every local gate in C8 passes;
- runtime/rate/OAuth construction is incrementally bounded;
- all original lifecycle/security proofs remain green;
- executable CI exists;
- stable and MSRV diagnostic owners pass;
- rust-analyzer manual acceptance has passed;
- required remote platform CI has passed;
- the package graph is technically verifiable;
- evidence matches source and observed commands.

If local implementation and validation are complete but pushing, graphical editor access, legal metadata, registry ownership, or package naming remain external, report:

> Implementation and local validation complete; external release checks pending.

List each blocker with owner and exact next action.

If any local invariant, test, package-graph, or documentation-truth gate is failing, report:

> Closure blocked.

---

# Final response format

At the end, provide:

1. exact branch and HEAD;
2. clean or dirty worktree;
3. phase/commit table C0–C8;
4. audit findings closed;
5. public API changes;
6. boundedness/security proof summary;
7. validation command table with actual results;
8. performance comparison;
9. package verification result;
10. rust-analyzer result;
11. Linux/Windows/macOS/Actions result;
12. external blockers and owners;
13. explicit statement that nothing was pushed, if true.

Do not provide an optimistic summary that is broader than the evidence.
