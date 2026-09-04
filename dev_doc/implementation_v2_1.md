# Concord v2.1 implementation roadmap

Implement v2.1 directly from the supplied v1 repository in coherent semantic slices. Do not land the full target docs ahead of behavior or retain a dual public language.

## Phase 0 — baseline and evidence controls

Before semantic edits:

- run and record every available baseline check;
- archive current public API, generated fixtures, contract descriptions, expansion size, and representative runtime traces;
- record existing failures separately;
- create 1/10/100/500/1,000 endpoint expansion fixtures;
- define compile/token/runtime/stream benchmark budgets;
- establish the v2.1 claim/evidence ledger and ADR statuses;
- bump no versions yet.

Exit: current behavior and verification limits are documented.

## Phase 1 — canonical semantic graph

Refactor macro internals while preserving current behavior temporarily:

- stable owner/value/endpoint/policy IDs;
- separate presence, initializer, sensitivity, and payload type;
- declared/local/effective attachments with provenance;
- one surface/capability model;
- one description projection;
- direct/unbound generation from one surface;
- semantic snapshots/golden tests.

Exit: emitters no longer reconstruct type shape/inheritance/surface independently.

## Phase 2 — prototype gate: representations and streams

Implement private experimental trait variants from Gate 1 without migrating the whole DSL:

- generic request/response representation pairs;
- sealed mode/driver alternatives;
- JSON/Text/Binary/Empty/Discard;
- NDJSON/CSV collected and lazy output;
- `ByteStream`, `ItemStream<T>`, `RequestStream<T>`;
- optional body;
- custom representation and endpoint-backed material;
- focused diagnostic assertions.

Measure coherence, MSRV, diagnostics, compile/code size, and stream runtime cost. Select private driver strategy and record ADR update.

Exit: Gate 1 and stream mechanical baseline pass. If they fail, revise the private trait shape while preserving semantic ownership.

## Phase 3 — v2.1 value, owner, and atom language

- parse literal client-contained tree;
- parse postfix `?`, `#[default]`, `#[sensitive]`;
- remove lexical `Option`/`Vec` semantics;
- implement exact `set`, `with`, `omit`, `reset` controls;
- resolve `client`, named scope, and `endpoint` references;
- implement `RequestAtom`, adapters, `concat`, `OrderedValues`, `each`;
- implement subdomain/path structural atoms;
- validate generated-name collisions;
- update runtime endpoint value/sensitivity parity.

Exit: value matrix, alias transparency, atom properties, and control-surface tests pass.

## Phase 4 — query/header patch vocabulary

- implement assignment, `when_present`, `append`, `remove`, `replace`, `off`;
- remove old symbolic/operator aliases;
- centralize static keys/header identity/reserved ownership;
- derive real and sanitized atoms from one emission;
- add reference-model property tests;
- update runtime endpoint patch parity.

Exit: Gate 4 passes and docs/examples use one spelling.

## Phase 5 — representation ownership and endpoint truth

- add client/scope `request` and `response` defaults;
- add scope-local `explicit` fences;
- add inline body/result `via` exceptions;
- make body input and result exact caller types;
- add endpoint status and effect class;
- remove response-spec status ownership;
- generate effective representation/provenance docs and semantic diff;
- migrate common examples to inherited JSON and local exceptions;
- remove wrapper-bearing ordinary signatures.

Exit: Gates 1–3 representation/inheritance mechanics pass; no old wrapper surface remains in active examples.

## Phase 6 — normative executor and errors

- add/normalize operation/page/attempt/physical identities;
- add monotonic dispatch knowledge;
- enforce one absolute deadline;
- implement selected body prepare/attempt/rate/private ordering;
- centralize server-delay fact;
- add effect-aware retry gates and shared budget timing;
- preserve transport-private recovery separation;
- make accepted decode terminal by default;
- implement stable error categories and bounded structured diagnostics.

Exit: Gate 5 deterministic traces pass.

## Phase 7 — authentication

- retain/version current slot state machine and systematic tests;
- migrate secret syntax and optional initial secret behavior;
- group auth blocks and enforce one challenge attribution;
- keep private target ownership;
- update source-kind recovery and one replay;
- endpoint-backed store returns unit and uses exact output/material obligation;
- provider isolation/time ownership;
- remove any parallel secret placement path.

Exit: auth state/ordering and information-flow tests pass.

## Phase 8 — rate limits

- migrate policy syntax/IDs/dimensions/cost/windows;
- implement canonical `RateKeyPart` and whole-plan admission;
- require explicit limiter selection;
- remove automatic feature-driven local limiter;
- change feedback to non-result-determining future admission state;
- centralize header interests and server delay;
- update local limiter clock/bounds/fairness docs/tests.

Exit: Gate 7 passes.

## Phase 9 — pagination

- require safe effect and complete page output;
- implement controller-owned items + continuation;
- add bounded header interests/current logical URI;
- reject cross-origin continuation;
- restrict live destinations and rate-key interactions;
- prepare body once per page only;
- implement aggregate deadline/send/page/item/byte/cycle/progress limits;
- implement Prefix/CompleteWithin and Prefix(0) no-work law;
- one aggregate final result/observer event.

Exit: Gate 6 passes.

## Phase 10 — observer, plans, reflection, runtime endpoints

- remove broad veto hooks;
- add minimal read-only operation/stream event vocabulary;
- ensure no current-result observer failures;
- implement `RequestPlan`, sensitivity propagation, effective descriptions/diff;
- align generated/unbound/runtime endpoint planning;
- complete runtime endpoint patch/representation/status/effect/sensitivity/binding parity;
- no second executor/native escape.

Exit: Gate 9 passes and the event vocabulary answers documented operator tasks.

## Phase 11 — extras, façade, docs, migration

- migrate representations, records, OAuth2, local limiter, tracing, test support;
- curate facade re-exports and features;
- replace active repository docs with this overlay, adjusted to actual names;
- migrate Riot and all examples;
- add migration tooling/report for ambiguous v1 Option/Vec/body/result cases;
- remove v1 parser/model/emitter/runtime/public aliases;
- bump workspace crates to 2.1.0;
- update Cargo lock/policy/CI if required.

Exit: one canonical language and no stale public vocabulary outside migration history.

## Phase 12 — release evidence

- run unfamiliar-user protocol;
- decide `with_x` and any remaining spelling gate;
- run large-contract and stream benchmarks;
- run full sentinel security matrix;
- satisfy all ten release gates;
- run `just verify`;
- update ADRs from Proposed to Accepted/Rejected/Superseded;
- write final implementation report.

Exit: v2.1 release candidate is mechanically, behaviorally, cognitively, and operationally supported to the stated level.

## Change discipline

Each phase lands as coherent semantic slices. Parser, generated API, bridge, runtime, docs, and tests for one changed meaning should not describe different versions.

Do not open a new architecture branch unless:

- a selected trait/syntax is mechanically impossible under MSRV/coherence;
- a release gate produces contrary evidence;
- a real repository invariant contradicts the source analysis;
- the project owner changes the product objective.

Record the counterexample and update the relevant ADR rather than silently redesigning.

