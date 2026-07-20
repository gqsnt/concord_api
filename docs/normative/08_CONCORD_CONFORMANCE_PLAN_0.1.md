# CONCORD_CONFORMANCE_PLAN_0.1

**Version:** 0.1  
**Status:** Normative Implementation Candidate
**Authority:** Required evidence for Concord 0.1 conformance  
**Requirement prefix:** `CONF`  
**Depends on:** Every preceding normative Concord 0.1 specification  
**Supersedes:** None

## 1. Purpose

This document defines how an implementation proves that it satisfies the Concord 0.1 normative specifications.

It does not introduce product behavior.

An implementation-candidate package is complete when every normative requirement has a planned evidence strategy. A release is conforming only when:

- every normative requirement has actual passing or approved waived evidence;
- every required automated test passes;
- every required compile configuration succeeds;
- no prohibited implementation-coupled test is used as the sole evidence for behavior;
- public documentation and examples match the normative specifications.

**CONF-CORE-001 — Complete evidence.** Every normative `PROD`, `DSL`, `GEN`, `API`, `EXEC`, `SEC`, `BOUND`, `FEAT`, and `COMPAT` requirement MUST have at least one actual passing or approved waived evidence entry before release-final status.

**CONF-CORE-002 — Behavioral evidence.** Observable requirements MUST be proved through public compile tests or runtime behavior rather than private source inspection alone.

## 2. Coverage planning and actual evidence

The archive contains two separate catalogs.

### 2.1 Requirement coverage plan

```text
conformance/REQUIREMENT_COVERAGE_PLAN_0.1.csv
```

Required columns:

```text
requirement_id
document
summary
planned_evidence_type
planned_evidence_id
planned_path
implementation_milestone
status
notes
```

Allowed coverage-plan status values are:

```text
planned
implemented
superseded
```

One coverage-plan row per active normative requirement proves that verification work has been designed. It does not prove conformance. Superseded historical identifiers remain in the requirement change record and may remain on byte-preserved historical evidence rows, but they are absent from current active coverage.

A `superseded` coverage row is lineage metadata rather than current planned coverage; its notes name
the active replacement and requirement-change record. Current active coverage is the exact set of
`planned` and `implemented` rows.

### 2.2 Actual conformance evidence

```text
conformance/CONFORMANCE_EVIDENCE_0.1.csv
```

Required columns:

```text
requirement_id
evidence_type
evidence_id
exact_path
exact_test
configuration
result
verified_commit
verified_at
waiver
```

Allowed actual results are:

```text
passing
waived
```

One requirement may have several actual evidence rows. An evidence row identifies an exact test, command, or repeatable manual review rather than only a directory.

### 2.3 Evidence-type vocabulary

Both catalog columns use a closed vocabulary. The following 16 labels are the canonical primitive
proof types:

| Label | Exact meaning |
|---|---|
| `parser-unit` | Isolated acceptance or rejection of source syntax without relying on runtime behavior. |
| `semantic-unit` | Isolated resolution, normalization, ownership, state-model, configuration, or semantic-diagnostic behavior without requiring an end-to-end runtime. |
| `compile-pass` | Successful Rust compilation of the exact supported use and configuration named by the row. |
| `compile-fail` | Expected Rust compilation failure with the relevant diagnostic and boundary reviewed by the cited fixture. |
| `downstream-generated` | Application-shaped compilation using a generated Concord API through documented public namespaces and no private imports. |
| `downstream-extension` | Downstream implementation or use of a documented stable extension surface through public namespaces. |
| `runtime` | Observable execution behavior under the deterministic or test-owned runtime conditions named by the row. |
| `concurrency` | Deterministically synchronized concurrent or cancellation behavior; timing-only sleeps are not sufficient. |
| `model-check` | Systematic exploration of a reduced state-transition model and its stated invariants. |
| `property` | An invariant checked across a named deterministic matrix or generated input space; generated failures retain a reproducible seed or minimized case. |
| `fuzz` | Supplemental execution of a named fuzz target; it does not replace deterministic evidence. |
| `feature-boundary` | Presence, absence, or isolation of behavior under the exact Cargo feature graph named by the row. |
| `msrv` | Compilation or public type checking under the declared minimum supported Rust version. |
| `doc-test` | Compilation and, when applicable, execution of Rust documentation examples through the maintained doctest path. |
| `manual-review` | A repeatable reviewer audit of exact named artifacts or command output against explicit recorded criteria. |
| `release-command` | Execution of the exact maintained release-gate command or aggregate release recipe named by the row at the recorded commit and configuration. |

The following 19 non-primitive labels are additionally permitted in both catalogs only with these
bounded meanings:

| Label | Kind | Exact definition or primitive mapping |
|---|---|---|
| `artifact-audit` | Specialized procedural type | A repeatable commit-scoped audit of the exact artifact paths, row ownership, recorded results, and any historical-preservation claim named by the row. It proves catalog artifact integrity, not product behavior absent another exact proof. |
| `boundary-test` | Specialized procedural type | An exact `BOUND-*` accepted/rejected compile boundary or below/equal/first-excess/checked-arithmetic runtime boundary. The row identifies which boundary and test procedure apply. |
| `catalog-audit` | Specialized procedural type | A repeatable audit of catalog headers or schema, row and identifier counts, uniqueness, normative-ID membership, planned/actual separation, or historical preservation, limited to the checks named by the row. |
| `compatibility-matrix` | Specialized procedural type | For `COMPAT-*` rows, the exact compiler, MSRV, package topology, dependency, or public-surface configurations named by the row. It does not imply an unrecorded platform or configuration. |
| `compile-fail-and-rustdoc` | Compound type | `compile-fail` + `compile-pass`; the `compile-pass` component is warning-denied Rustdoc generation, with any rendered-surface inspection named explicitly by the row. |
| `compile-fail-runtime-and-rustdoc` | Compound type | `compile-fail` + `runtime` + `compile-pass`; the `compile-pass` component is warning-denied Rustdoc generation, with any rendered-surface inspection named explicitly by the row. |
| `configuration` | Specialized procedural type | Exact public configuration signatures, default values, finite enable/disable rules, or runtime enforcement for the `API-*` or `BOUND-*` contract named by the row. |
| `conformance` | Specialized procedural type | Only for `CONF-*` rows: the exact conformance-process property, fixture architecture, catalog rule, documentation gate, runner rule, or maintained command named by the row. It is not a general assertion that the product or release conforms. |
| `doctest-and-rustdoc` | Compound type | `doc-test` + `compile-pass`; the `compile-pass` component is warning-denied Rustdoc generation, with any rendered-surface inspection named explicitly by the row. |
| `downstream-generated-runtime` | Compound type | `downstream-generated` + `runtime`. |
| `feature-matrix` | Specialized procedural type | For `FEAT-*` rows, the exact set of Cargo feature graphs and isolation checks named by the row. It does not imply an unrecorded feature combination. |
| `lifecycle-runtime` | Permitted alias | `runtime`, narrowed to the `EXEC-*` lifecycle ordering, side effects, attempt counts, deadlines, cancellation, or completion behavior named by the row. |
| `manifest` | Specialized procedural type | Review of exact manifest or lockfile metadata, dependency versions and sources, or package topology through the artifact and metadata/policy command named by the row. |
| `parser-semantic` | Compound type | `parser-unit` + `semantic-unit`. |
| `public-api` | Specialized procedural type | Exact `API-*` signature, namespace, trait, ownership, or accessor behavior proved by the downstream compilation or observable runtime artifacts named by the row. It is not a general public-API approval. |
| `runtime-and-compile` | Compound type | `runtime` + `compile-pass`. |
| `rustdoc` | Specialized procedural type | Warning-denied Rustdoc generation plus only the rendered public-surface semantics or private-name exclusions explicitly inspected by the row. It does not imply a doctest. |
| `security-runtime` | Permitted alias | `runtime`, narrowed to the `SEC-*` secrecy, redaction, safe-surface, or fail-closed behavior named by the row. |
| `source-runtime` | Permitted alias | `runtime`, narrowed to `PROD-*` behavior. Exact source paths identify production ownership but do not add source-inspection proof or strengthen the runtime result. |

A compound label requires evidence for every mapped primitive in the exact row. A specialized label
provides only its bounded definition and does not stand in for an unnamed primitive proof. The
canonical `fuzz` type is intentionally retained for the supplemental targets in section 20 even
though neither current catalog uses it.

In the coverage plan, a type label describes only the intended verification procedure. In the
actual-evidence catalog, it classifies only the exact artifact, command or test, configuration,
result, and verified commit recorded by that row. No label turns planned coverage into actual
evidence or strengthens an actual row beyond those fields.

This vocabulary is closed. A label not enumerated in the two tables above is not permitted in
either catalog until an approved normative amendment to this conformance plan explicitly names and
defines it.

A waived requirement records an approved decision and a repeatable manual review procedure.

**CONF-TRACE-001 — Exact coverage planning.** The coverage-plan catalog MUST contain exactly one row for every normative requirement ID.

**CONF-TRACE-002 — No undocumented waiver.** A waived actual-evidence row MUST identify an approved decision and manual evidence procedure.

**CONF-TRACE-003 — No untracked normative prose.** Recorded normative-document review MUST reject substantive normative `MUST`, `MUST NOT`, `SHOULD`, `SHOULD NOT`, or `MAY` statements outside stable requirement definitions, except an explicit allowlist for normative-language definitions.

**CONF-TRACE-004 — Acyclic normative dependencies.** Recorded normative-document review MUST reject a cycle in normative document dependencies.

**CONF-TRACE-005 — Planned coverage is not evidence.** Tooling and documentation MUST distinguish a planned or implemented coverage row from actual passing or waived conformance evidence.

**CONF-TRACE-006 — Exact actual evidence.** Every actual-evidence row MUST identify an exact artifact and result, and release-final validation MUST reject missing actual evidence for any normative requirement.

## 3. Test organization

Recommended repository layout:

```text
concord_core/
├── src/
├── tests/
│   ├── public_api/
│   ├── runtime/
│   ├── concurrency/
│   ├── security/
│   ├── feature_boundary/
│   └── downstream/
concord_macros/
├── src/
├── tests/
│   ├── generated_surface.rs
│   ├── ui.rs
│   ├── ui/fail/
│   └── fixtures/
concord_examples/
concord_test_support/
```

The exact paths may differ, but test ownership and separation must remain clear.

Nextest is the canonical runner for Rust unit and integration tests. `cargo test` is retained only
for Rust doctests because Nextest does not execute them. A Rust test must not spawn `cargo test` to
exercise another Rust test. Nested Cargo check/build/tree/metadata processes are permitted only
when a distinct manifest, dependency topology, feature graph, or compile-fail boundary is itself
under test.

Nextest configuration requires version 0.9.115, the first repository-compatible release needed for
the inherited CI profile used here. It declares zero retries, finite stuck-test and CI run timeouts,
and a bounded test group for Cargo-spawning tests. It has no default filter that omits tests.
Independent loopback tests remain parallel unless a concrete shared resource requires a group.

## 4. Parser unit tests

Parser tests operate on syntax without runtime dependencies.

They cover:

- lexical forms;
- required semicolons;
- client/scope/endpoint ordering;
- endpoint parentheses;
- endpoint clause order;
- duration units;
- restricted public expressions;
- `fmt[...]` comma rules;
- credential source syntax;
- rate-limit syntax;
- pagination assignment punctuation;
- compile-time nesting limits.

Example evidence identifiers:

```text
PARSER-CLIENT-ORDER-001
PARSER-FMT-COMMAS-001
PARSER-PAGE-ASSIGN-001
PARSER-DEPTH-001
```

**CONF-PARSER-001 — Grammar coverage.** Every accepted grammar production and every explicit grammar prohibition MUST have parser-level coverage where it can be isolated from semantics.

## 5. Semantic unit tests

Semantic tests operate on parsed AST and normalized IR.

They cover:

- client, scope, endpoint, credential, and template symbol tables;
- ambiguous value references;
- name uniqueness;
- generated-name collisions;
- host/path/query/header composition;
- auth append/replace/off;
- rate-limit append/replace/off;
- timeout inheritance/disablement;
- secret confinement;
- credential/placement compatibility;
- endpoint-backed cycles;
- rate-limit dimension visibility and optionality;
- endpoint body/response compatibility;
- pagination bodyless requirement;
- normalized IR containing no unresolved inheritance operations.

Example evidence identifiers:

```text
SEMA-AUTH-COLLISION-001
SEMA-ENDPOINT-CYCLE-001
SEMA-RATE-DIMENSION-001
SEMA-NORMALIZE-001
```

**CONF-SEMA-001 — Normalization proof.** Semantic tests MUST prove that code generation receives fully resolved configuration rather than reconstructing inheritance.

## 6. Compile-pass tests

Ordinary integration tests compile canonical public usage that needs no distinct dependency graph.
Trybuild or an equivalent focused compiler harness owns source-only diagnostic cases. A standalone
Cargo package is reserved for behavior that depends on package metadata or dependency topology,
such as a renamed dependency, a missing runtime package, an incompatible macro/core package, or a
feature/visibility graph.

Pass fixtures include:

- minimal client;
- multiple clients in one macro invocation;
- required/optional/default client values;
- client builder, immediately evaluated `.http`/`.runtime` closures, duplicate-call suppression, same-snapshot cloning, and independent client construction;
- nested facades;
- endpoint required/optional/default arguments;
- endpoint request setters after one generated support-prelude import;
- simultaneous generated preludes for multiple clients and repeated setter names on distinct typed requests;
- JSON under feature;
- text;
- custom request/response codec;
- bytes;
- no-content;
- request stream;
- response stream;
- multipart under feature;
- API key, bearer, Basic, OAuth2, endpoint-backed, custom provider;
- rate-limit templates and local rules;
- custom limiter and observer;
- offset/page/cursor/custom pagination;
- explicit generated endpoints, including nested-scope constructor parity;
- renamed `concord_core` dependency and nested-module macro invocation;
- prepared endpoints;
- runtime hooks;
- dangerous raw and custom retry under features.

Each pass fixture must import only documented public namespaces.

**CONF-COMPILE-001 — Canonical generated use.** Every generated API category defined by `GEN-*` MUST have at least one downstream compile-pass fixture.

**CONF-COMPILE-002 — Advanced extension use.** Every stable advanced extension trait MUST have a downstream implementation fixture.

## 7. Compile-fail tests

Compile-fail fixtures cover:

- missing base;
- duplicate base;
- configuration after children;
- DSL optional `?` syntax;
- invalid `fmt` punctuation;
- arbitrary public Rust expressions;
- secret use in public shaping;
- duplicate names;
- generated name collisions;
- invalid host/path literals;
- duplicate query/header entries;
- reserved headers;
- incompatible credential placement;
- auth collisions;
- endpoint-backed cycles;
- invalid rate-limit cost/window/dimensions;
- pagination on body/bytes/no-content/stream endpoints;
- old built-in assignment or custom-factory pagination syntax;
- invalid pagination struct fields, bindings, initializers, controller bounds, or visibility;
- missing features;
- unavailable generated methods;
- endpoint request setter use without its generated prelude import;
- downstream implementation of a sealed endpoint request extension trait;
- an endpoint request extension trait used with the wrong endpoint request type;
- generated-prelude trait alias collisions;
- invalid custom trait implementations;
- private-module imports in supported fixtures.

Checked-in expected diagnostics must be reviewed for:

- primary wording;
- primary span;
- useful notes;
- feature guidance;
- absence of sensitive values;
- absence of private generated internals.

**CONF-COMPILE-003 — Diagnostic fixtures.** Every explicit DSL diagnostic rule MUST have a compile-fail fixture with reviewed expected output.

## 8. Downstream generated API fixture

A standalone test crate must depend on published-path `concord_core` and `concord_macros` as an application would.

It exercises:

- client construction;
- builder methods;
- same-snapshot client clones and independent clients with different client values;
- facades;
- request setters;
- endpoint-specific request setters through only `use example_api::prelude::*`;
- direct await and supported metadata terminals after endpoint-specific setters;
- simultaneous imports of collision-free preludes from multiple generated clients;
- direct await type checking;
- metadata terminal;
- stream terminal;
- pagination;
- credential handles;
- endpoint-backed acquisition;
- explicit generated endpoints with required nested scope values;
- renamed dependency and nested macro expansion.

It must not import:

- `__private`;
- `__development` except in a dedicated feature-boundary fixture;
- internal architecture modules.

**CONF-DOWNSTREAM-001 — Generated public sufficiency.** Normal generated workflows MUST compile downstream without application awareness of generated-private integration types.

The generated-request proof additionally checks that generic timeout/debug methods remain inherent,
stream requests retain direct await without the metadata terminal, renamed runtime dependencies use
the same public prelude contract, and no application import from `concord_core::__private` appears.

## 9. Downstream public extension fixture

A standalone crate implements every intentional advanced extension:

- `MediaType`;
- `EncodeBody`;
- `DecodeBody`;
- `BodyFactory` and `MultipartFactory`;
- custom credential material;
- custom credential provider;
- provider HTTP use;
- custom pagination controller;
- `HasNextCursor`;
- custom rate limiter;
- custom rate-limit observer;
- runtime hooks;
- prepared buffered endpoint;
- prepared streaming endpoint;
- safe proxy and HTTP configuration.

The fixture imports only `prelude`, `advanced`, and an explicitly tested dangerous namespace.

**CONF-DOWNSTREAM-002 — Extension surface sufficiency.** Every documented stable extension workflow MUST compile without private imports.

## 10. Deterministic native executor

Runtime tests use a private or `dangerous-dev-tools` deterministic seam at the final managed Reqwest execution boundary.

It provides independent channels:

```text
Application
Provider
```

The harness supports:

- scripted response heads and bodies;
- scripted native errors;
- body polling control;
- response polling control;
- synchronization gates;
- cancellation points;
- request count;
- safe logical request capture;
- explicit fake-value authentication/body expectations.

Ordinary captures expose:

- endpoint;
- method;
- logical URL;
- public safe headers;
- protected header names without values;
- body category;
- known body length;
- page and recovery indexes.

Ordinary captures do not expose:

- credential values;
- request body bytes;
- native authenticated URL.

Explicit fake expectations compare inside the executor and return redacted mismatches.

**CONF-DET-001 — No production abstraction.** The deterministic seam MUST NOT alter generated client type signatures or create a public production transport contract.

**CONF-DET-002 — Separate channels.** Tests MUST be able to distinguish and independently script application and provider HTTP.

### 10.1 Reqwest baseline contract probes

Executable local probes cover the selected Reqwest baseline assumptions on which Concord relies:

- redirects are disabled on application and provider clients;
- system proxy discovery is disabled and test environments cannot silently route through proxy variables;
- default and disabled native retry selections behave as assumed;
- native request timeout behavior remains distinguishable from Concord operation deadlines;
- hidden native retry does not re-enter Concord body factories, hooks, credentials, limiter, or page planning;
- managed request-body failure remains reducible to the Concord request-body category.
- application retry is disabled by default and explicit Reqwest-default opt-in may create hidden physical attempts;
- dropping an unread terminal response, capturing a small complete terminal body, and abandoning an oversized terminal body are probed for connection-pool behavior without making reuse a product guarantee.

**CONF-REQWEST-001 — Executable dependency assumptions.** Every Reqwest behavior that affects Concord request counts, security defaults, timeout semantics, or body replay MUST have an executable deterministic contract probe for the selected Reqwest baseline.

## 11. Runtime ordering tests

Runtime tests record deterministic phase events and verify the Execution Specification.

Required ordering cases:

```text
public head + body metadata
< before-send hook
< body materialization
< credential preflight
< limiter acquisition
< credential finalization
< request debug
< auth materialization
< native execution
< response-head debug
< response-head hook
< limiter observation
< auth challenge/status/body/decode
< completion
```

Failure tests verify absence of every later phase.

Examples:

- invalid host calls nothing protected;
- hook veto calls no codec/body factory/provider/limiter/send;
- codec or body-factory failure occurs after hook approval and calls no provider/limiter/send;
- credential preflight failure calls no limiter/send;
- limiter failure calls no finalization/send;
- credential clear, replacement, rejection, or expiry during limiter wait is observed at finalization and prevents stale send;
- request debug lacks auth while native capture proves auth insertion;
- challenge response release precedes refresh;
- decode receives only bounded bytes.

**CONF-ORDER-001 — Side-effect matrix.** Every row in the Execution Specification side-effect matrix MUST have deterministic runtime evidence.

## 12. Request-body tests

Required cases:

- empty body under the minimum positive configured limit;
- nonempty body above a deliberately small positive limit;
- exact boundary succeeds;
- one-byte excess fails;
- excess frame not delivered;
- exact-length underflow;
- exact-length overflow;
- stream not polled before send;
- stream cancellation;
- buffered codec invoked once;
- auth recovery reuses buffered bytes;
- body factory invoked per Concord attempt;
- hidden retry does not invoke factory;
- multipart framing counts;
- one-shot multipart cannot recover;
- multipart factory can recover.

## 13. Response tests

Required cases:

- known excessive `Content-Length` without body polling;
- unknown-length first excess frame withheld;
- decompressed bytes counted;
- terminal status remains primary while a small complete body may be captured;
- terminal `Display`, `Debug`, source chains, hooks, limiter contexts, and completion events contain no body sentinel;
- known oversized terminal status body is not polled;
- unknown oversized terminal status body exposes no partial prefix;
- terminal body read failure preserves status and records `UnavailableReadFailure`;
- terminal body deadline preserves status and records `UnavailableDeadline`;
- disabled terminal capture records `Disabled`;
- post-decompression terminal bytes count against the status-body limit;
- explicit `decode_body::<CustomCodec>()` succeeds only for a complete body;
- decoder not called on successful-response body error;
- decoder receives complete bounded bytes;
- JSON media matching including `+json` and parameters;
- custom media validation;
- no-content accepts every `2xx` and does not poll body;
- stream unknown body remains lazy;
- stream known-length excess before return;
- stream first excess chunk withheld;
- raw response bounded and success-only;
- wrapper `Debug` contains no body/value.

### 13.1 Operation-deadline tests

Required cases:

- deadline expires while `before_send` is pending;
- deadline expires during body materialization;
- provider HTTP uses the earlier of its own timeout and remaining parent budget;
- limiter wait observes the remaining budget;
- authentication recovery does not reset the deadline;
- buffered response collection remains under the original budget;
- each pagination page receives an independent deadline;
- returned stream polling is outside the endpoint-operation deadline after `StreamOpened`.

## 14. Authentication tests

### 14.1 Source categories

- static API key;
- static bearer;
- Basic;
- OAuth2 client credentials;
- endpoint-backed;
- custom provider.

Built-in OAuth2 coverage additionally verifies:

- POST token request;
- OAuth-compatible Basic client encoding;
- form-encoded `grant_type=client_credentials`;
- optional form-encoded scope;
- non-empty `access_token`;
- optional case-insensitive bearer `token_type`;
- checked non-negative integer `expires_in`;
- reacquisition on refresh rather than refresh-token grant;
- non-success and malformed token responses remain body-safe.

### 14.2 Coordination

- one acquisition for concurrent missing requests;
- one refresh for concurrent expiring requests;
- all current waiters receive same failure;
- later independent call retries no-cooldown failure;
- provider lock freedom;
- leader cancellation;
- waiter cancellation;
- stale acquisition after manual replace;
- stale refresh after replace;
- stale exact-version rejection;
- atomic Basic replacement;
- atomic OAuth input replacement.
- clear during limiter wait prevents stale credential send when clear completes before finalization;
- complete source replacement during limiter wait prevents old-slot send when replacement completes before finalization;
- expiry during limiter wait triggers current refresh/acquisition under the remaining deadline;
- clear or replacement racing after finalization follows the documented linearization boundary.

### 14.3 Challenge behavior

- static API key, static bearer, Basic, and endpoint-backed placements default to `never`;
- built-in OAuth2 bearer defaults to `unauthorized`;
- custom provider placement requires explicit challenge policy;
- semantic rejection of more than one challenge-enabled placement;
- an ambiguous `401` on a multi-credential endpoint mutates only the one challenge-enabled credential;
- default terminal `403` and explicit `403` eligibility;
- explicitly challenge-enabled static no immediate resend;
- persistent static rejection prevents later provider invocation/send until source replacement;
- explicitly challenge-enabled endpoint-backed invalidate-only;
- refreshable one recovery;
- refreshable recovery passes the challenged current material to `refresh`;
- a newer ready material is reused without stale refresh overwrite;
- one-shot body no recovery;
- second challenge terminal;
- recoverable challenge body remains unread;
- terminal challenge body follows bounded status-body capture rules;
- query auth absent from logical URL.

### 14.4 Provider HTTP

- isolated application hooks/limiter/debug;
- request limit;
- response limit;
- HTTPS-only provider URL rejection before body-factory invocation;
- TLS preflight;
- status returned to provider;
- provider retry independence.

**CONF-AUTH-001 — Race determinism.** Credential concurrency tests MUST use deterministic barriers/channels rather than timing-only sleeps.

### 14.5 Systematic interleaving model

A reduced credential transition core is exercised with Loom, Shuttle, or an equivalent systematic concurrency model checker.

The model covers:

- two missing prepares;
- leader and waiter cancellation;
- manual set/clear during acquisition and refresh;
- slot replacement during an old operation;
- exact-version rejection during refresh;
- persistent static rejected state;
- stale success and stale failure publication;
- cohort wakeup and progress.

Properties include single-flight, no lost wakeup, no stale overwrite, no rejected-value restoration, monotonic private versions, and eventual progress after cancellation.

**CONF-AUTH-002 — Model-checked state transitions.** Authentication release evidence with evidence type `model-check` MUST include systematic interleaving/model checking of the reduced credential state machine in addition to deterministic integration tests.

## 15. Rate-limit tests

Required cases:

- one acquire per Concord attempt;
- no acquire for hidden Reqwest retry;
- second acquire for auth recovery;
- one acquire per page;
- multiple windows;
- multiple rules;
- atomic multi-rule reservation;
- cancellation while waiting consumes no quota;
- cooldown wait and reevaluation;
- `Retry-After` delta and HTTP date;
- cooldown clamping;
- future-call-only behavior;
- response observation cannot replace current result;
- key-size rejection without key text;
- state entry bounds;
- idle pruning;
- eviction behavior, including a fresh local burst after an evicted key is observed again;
- no-governor construction failure;
- explicit no-op limiter;
- custom limiter contexts exclude body/auth.

## 16. Pagination tests

Required cases:

- `Take(0)` performs no controller construction or planning;
- requested nonzero page limit above configured ceiling fails before controller construction;
- soft page count sends at most count;
- final soft page does not calculate next controller;
- hard cap succeeds on natural stop;
- hard cap errors when next exists;
- no page beyond cap;
- `Vec<T>` works directly;
- custom `IntoIterator` page works;
- one built-in and one application controller share the parser, normalized IR, generated binding, public trait, and core loop;
- an application controller is added without changing macro source;
- qualified, imported, generic, nested, repeated-expansion, renamed-core, bindings-only, stable-unbound, mixed, and trailing-comma declarations compile;
- old built-in assignment and custom-factory syntax fail with focused struct-and-`bind` migration diagnostics;
- duplicate fields/destinations, invalid bind ownership, indirect binds, shorthand, rest syntax, calls, methods, closures, async/blocks, and protected initializers fail structurally;
- Rust reports inaccessible, missing, unknown, wrong-type, non-controller, and non-`Send` controller failures;
- a `Send` controller that is neither `Clone` nor `Sync` executes and moves state between pages;
- controller construction occurs once after nonzero ceiling preflight, while `Take(0)` and failed preflight construct nothing;
- validation occurs before every attempted page;
- bound endpoint/scope values transfer into the controller once and current values bind into every fresh endpoint clone;
- stable unbound fields initialize once and never rebind;
- original endpoint, facade, scope, client, and stable snapshots remain unchanged;
- bound dynamic host, path, ordered query, public header, current rate key, and canonical identity change together across pages;
- an application controller may select the same exact count termination as offset and page-number controllers;
- `Some(1)` selects empty-page termination;
- a custom controller with no count policy continues after an empty page when `next` returns `Some`;
- cursor pagination continues after an empty item page when metadata supplies a continuation;
- item order across pages;
- offset short-page stop;
- page-number short-page stop;
- `next -> None` stops;
- count termination precedes a retained next error, while a non-count-terminated page propagates it;
- repeated identical request blocked;
- two-state and longer cycles blocked;
- exact canonical identity equality for URL default ports, query order, lowercase header names, repeated header values, and full equality after hash collision;
- loop error redaction;
- one active page;
- page auth recovery retains page index;
- later page ordinary error retains category;
- collected-item ceiling fails before excess append and returns no partial result;
- identity-storage ceiling fails before excess retention and returns no partial result;
- aggregate pagination completion reports final success/failure and safe counts;
- no partial result;
- cancellation prevents later pages.

**CONF-PAGE-001 — No item-trait dependency.** A downstream compile test MUST prove that `Vec<T>` and a custom `IntoIterator` page require no Concord-specific item extraction implementation.

**CONF-PAGE-002 — Downstream controller without macro modification.** A downstream compile-and-runtime fixture using only stable public namespaces MUST prove that an application pagination controller uses generated struct construction and binding without a controller-specific `concord_macros` implementation branch or private import.

## 17. Hook and debug tests

Required cases:

- hook veto before provider and limiter;
- hook input excludes body/auth/native URL;
- response-head hook for success and every final status;
- response-head hook before body polling;
- one completion per direct/page operation;
- stream creation reports `StreamOpened`, not full `Success`;
- one aggregate completion per pagination collection, including `Take(0)`;
- auth recovery produces two before-send and response-head events with one operation ID and one completion;
- concurrent identical endpoint calls have distinct operation IDs;
- page events carry the aggregate pagination parent operation ID;
- completion exposes monotonic elapsed duration and final status where available;
- hidden retries produce no extra events;
- cancellation does not guarantee completion;
- debug `None`, `Basic`, and `Headers` volume;
- request debug before auth insertion;
- response debug for every final response;
- wrapper debug structural and redacted.

## 18. Security sentinel matrix

For each sentinel class from `SEC-TEST-001`, tests inspect applicable:

- `Display`;
- `Debug`;
- `std::error::Error::source` chain;
- before-send hook;
- response-head hook;
- completion hook;
- built-in debug output;
- rate-limit context;
- built-in rate-limit errors;
- pagination errors;
- generated Rustdoc fixture output;
- successful wrapper `Debug`;
- terminal status-response structural `Debug`;
- explicit terminal `StatusBody::bytes()` positive-access fixture.

A test fails if a sentinel appears on a prohibited automatic surface. The positive-access fixture proves the same sentinel is available only through the explicit complete status-body accessor.

**CONF-SEC-001 — Source-chain scan.** Security tests MUST recursively inspect every retained source in a built-in error chain.

## 19. Property tests

Required property-test targets:

### 19.1 Path segmentation

For arbitrary strings, dynamic path insertion must not change the declared segment count.

### 19.2 Logical URL auth exclusion

For arbitrary query keys/values and auth values, logical/safe URLs never contain the auth value.

### 19.3 Byte-limit wrapper

For arbitrary frame sequences and limits, delivered bytes never exceed the limit and the first excess frame is not delivered.

### 19.4 Credential version monotonicity

For generated operation sequences of prepare, reject, recover, set, clear, complete, and cancel, stale operations never replace newer state.

### 19.5 Pagination identity

For generated controller cycles, no previously seen logical request identity is sent twice.

**CONF-PROP-001 — Deterministic seeds.** Property-test failures MUST print reproducible seeds or minimized cases.

## 20. Fuzz tests

Recommended fuzz targets:

- DSL parser token streams;
- nested `fmt` parsing;
- host-label validator;
- path-segment validator;
- sensitive-name classifier;
- safe URL renderer;
- duration parser;
- `Retry-After` parser.

Release CI may run bounded fuzz smoke durations; longer fuzzing should run on scheduled CI.

Fuzzing is supplemental and does not replace deterministic conformance tests.

## 21. Feature-boundary tests

Each feature is tested independently.

Required fixtures:

- default core feature list;
- no-default HTTP;
- no-default fixed and dynamic HTTPS failure;
- no-default custom limiter;
- no-default + JSON independence;
- JSON absence diagnostics;
- multipart absence diagnostics;
- each compression feature independently;
- HTTP/2 API gate;
- governor absence fail closed;
- raw method absence;
- custom retry absence;
- development module absence;
- dangerous feature independence;
- application retry disabled when no DSL or builder opt-in exists;
- explicit `retry reqwest_default` and `.retry_reqwest_default()` behavior;
- Cargo feature unification documented fixture where practical.

## 22. MSRV tests

On Rust 1.85, CI must compile:

- `concord_core --no-default-features`;
- `concord_core --all-features`;
- `concord_macros`;
- downstream generated API fixture;
- downstream public extension fixture.

Tests that require a newer auxiliary tool may be skipped in the MSRV job, but library compilation and public type checking may not.

**CONF-MSRV-001 — Public MSRV.** Every stable public signature and generated workflow MUST compile on Rust 1.85.

## 23. Documentation tests

Every public Rust example should be one of:

- a passing doctest;
- a compile-checked example crate/module;
- a deliberately ignored non-compiling conceptual fragment clearly labeled as such.

The complete canonical DSL example must compile in a dedicated fixture.

Generated Rustdoc must build with warnings denied.

Rendered generated Rustdoc must document the support-module prelude import and each public endpoint
request extension trait/alias while omitting the private seal, core endpoint mapping bridge,
generated-private endpoint bounds, request fields, and `concord_core::__private`. The rendered
fixture includes multiple clients, nested scopes, repeated endpoint names, and a renamed core
dependency.

**CONF-DOC-001 — No uncompiled canonical example.** A canonical example presented as valid MUST have compile evidence.

## 24. Examples

Required compile-checked examples:

- minimal client;
- full DSL reference;
- JSON/text/custom codec;
- request stream and response stream;
- multipart upload;
- static and OAuth auth;
- endpoint-backed auth;
- custom provider;
- rate limiting and `Retry-After`;
- offset/page/cursor/custom pagination;
- explicit generated endpoint;
- prepared endpoint;
- dangerous raw response under feature;
- dangerous custom retry under feature;
- large real-world fixture such as Riot-style regional API.

Normal automated example execution must require no API key and perform no live network call.

## 25. Performance checks

The performance package measures local overhead for:

- core client construction;
- generated client/facade/request creation;
- public head preparation;
- codec encoding/decoding;
- body-limit polling;
- credential ready-path preparation;
- limiter acquisition ready path;
- pagination loop overhead;
- stream wrapping.

Benchmarks do not measure internet latency and do not define hard product thresholds in 0.1.

Benchmark compilation is release-required.

## 26. Prohibited implementation-coupled tests

The following must not be the sole evidence for a normative requirement:

- exact private type names;
- exact private field counts;
- mutex or channel implementation choice;
- module/file path;
- source-text keyword searches;
- exact generated token-stream snapshots containing private adapters;
- enum memory size;
- internal helper function order;
- private compatibility-marker name;
- current lock-poisoning strategy;
- runtime source substring assertions.

Focused codegen assertions are allowed for public names or the presence of the generated contract marker when compile tests cannot isolate the behavior.

**CONF-TEST-001 — Public-behavior priority.** Tests MUST prefer public compilation and observable runtime behavior over private source shape.

## 27. CI and release commands

The repository provides one canonical release command:

```text
just release
```

The pinned `rust-toolchain.toml` makes ordinary `cargo` recipes MSRV runs. The release recipe
composes maintained default, all-feature, and no-default recipes and executes jobs equivalent to:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo check --workspace --all-targets --all-features
cargo check -p concord_core --all-targets --no-default-features
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo nextest run --profile ci --workspace
cargo nextest run --profile ci --workspace --all-features
cargo nextest run -p concord_core --no-default-features --lib -E 'test(=http::build::tests::no_default_http_works_and_https_preflight_is_typed)'
cargo test --workspace --doc
cargo test --workspace --doc --all-features
cargo rustdoc -p concord_core -- -D warnings
cargo rustdoc -p concord_macros -- -D warnings
cargo rustdoc -p concord_test_support -- -D warnings
cargo rustdoc -p concord_core --all-features -- -D warnings
cargo rustdoc -p concord_macros --all-features -- -D warnings
cargo rustdoc -p concord_test_support --all-features -- -D warnings
cargo deny check
git diff --check
```

`cargo test` appears only in the two doctest commands. Default/all/no-default repetition is
intentional because each command compiles a different feature surface. The release command invokes
the maintained recipes rather than copying their command bodies. Rust 1.85 `cargo rustdoc` has no
workspace selector, so the documentation recipes invoke it once for each workspace library and pass
the warning policy directly to rustdoc.

The release owner additionally records a manual normative-document review, coverage-plan review, conformance-evidence review, and release-command audit. These reviews are governance evidence; they are not replaced by a repository specification checker.

It also compiles:

- trybuild fixtures;
- downstream generated fixture;
- downstream extension fixture;
- examples;
- benchmark targets.

Nextest automatic test retries must be zero. Cargo-process tests use a bounded Nextest test group
and a longer finite slow-test timeout; the CI profile has fail-fast disabled and a finite global
timeout.

**CONF-CI-001 — Warnings denied.** Clippy and Rustdoc warnings MUST fail release validation.

**CONF-CI-002 — No automatic retries.** Maintained release test execution MUST use zero automatic retries.

**CONF-CI-003 — No external live services.** Release tests MUST NOT require live external APIs, real credentials, pre-existing proxy configuration, externally managed network listeners, or Internet access. Tests MAY create and own ephemeral loopback HTTP, HTTPS, mTLS, and proxy listeners and MAY use process-scoped environment configuration when the behavior is deterministic, isolated, and restored or confined to a child process.

**CONF-CI-004 — Canonical Rust test runner.** Maintained unit and integration test execution MUST use Nextest with no omitted default test set, and Cargo-spawning tests MUST use bounded concurrency; `cargo test` is reserved for doctests.

## 28. Milestone exit criteria

### 28.1 Foundation milestone

Requires passing:

- errors/redaction tests;
- safe HTTP config tests;
- deterministic executor boundary tests;
- default/no-default construction tests.

### 28.2 Typed request milestone

Requires:

- direct bytes/no-content hand-authored endpoints;
- one minimal generated endpoint through the same real managed Reqwest executor;
- renamed-dependency and nested-module expansion evidence;
- status behavior;
- hooks;
- typed response path without runtime downcasting or a temporary generated executor.

**CONF-VERTICAL-001 — Early generated vertical slice.** Before broad body, authentication, rate-limit, or pagination surfaces are stabilized, one generated endpoint MUST execute end to end through the real managed Reqwest executor under deterministic test control.

### 28.3 Body/response milestone

Requires complete body and response-limit matrices, codecs, streams, and multipart tests.

### 28.4 Macro milestone

Requires parser, semantic, trybuild, generated API, and canonical example coverage for implemented DSL features.

### 28.5 Authentication milestone

Requires all authentication source, concurrency, cancellation, recovery, and provider HTTP tests.

### 28.6 Rate-limit milestone

Requires limiter behavior, state bounds, feature absence, and custom extension tests.

### 28.7 Pagination milestone

Requires page-limit, controller, loop, error, and no-partial-result coverage.

### 28.8 Release milestone

Requires exact coverage planning plus actual conformance evidence for every normative requirement. Every actual result is `passing` or explicitly approved `waived`; coverage-plan status alone is insufficient.

## 29. Definition of done

A Concord 0.1 release is complete only when:

- all normative documents are `Normative Release Final`;
- coverage planning is exact and actual conformance evidence is complete;
- every required test passes;
- all public examples compile;
- default, all-feature, no-default, dangerous-feature, and MSRV matrices pass;
- public extension fixtures use no private imports;
- security sentinel matrices pass;
- release documentation contains no behavior absent from the specifications;
- dependency and license policy passes;
- benchmark targets compile.

**CONF-RELEASE-001 — Evidence-complete release.** A release MUST NOT be declared conforming while any normative requirement lacks passing or approved waived evidence.
