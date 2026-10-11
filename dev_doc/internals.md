# Concord v3 internals

**Status:** Active maintainer map for `3.0.0-beta.1`. It assigns implementation
responsibility; public semantics are owned by the v3 implementation
specification and the in-repository normative documents
([`docs/design_invariants.md`](../docs/design_invariants.md),
[`docs/spec.md`](../docs/spec.md)). The per-path source map is
[`repository_change_map.md`](repository_change_map.md).

## Workspace roles

| Crate / path | Responsibility |
|---|---|
| `crates/concord-macros` | `api!` compiler: parse (`syntax/`), resolve into one semantic model (`semantic/`), emit the generated client against the versioned bridge (`emit/`) |
| `crates/concord-core` | Runtime contracts and the one managed executor: symbolic head programs, owner projections, controls, bodies, responses, auth slots, rate limiting, retry, pagination, streams, journal/errors, inspection model |
| `crates/concord-extras` | First-party semantic implementations: Json, `NotFound<R>`, CSV/NDJSON records, built-in pagination controllers, OAuth2 client credentials, `LocalRateLimiter`, shared-adapter conformance fixture, `DisplayAtom` |
| `crates/concord` | Façade: `api!`, the §7.2 root re-exports and the role modules (`auth`, `body`, `codec`, `endpoint`, `error`, `http`, `inspection`, `observation`, `pagination`, `rate_limit`, `records`, `response`, `retry`, `runtime`) |
| `crates/concord-inspect` | `concord.contract/3` export, canonical JSON, snapshot check, axis-classified semantic diff and explanations; keeps JSON out of core |
| `crates/concord-test-support` | Deterministic loopback servers, TLS server, CONNECT proxy and scripted helpers that use production semantics |
| `crates/concord-examples` | Compile-checked examples: canonical §22 Catalog, §23 application examples, §17.2 envelope controller, Riot conversion, docs anchors and `docs_spec.rs` |
| `tests/downstream/*` | Two real crates: an API crate that expands `api!` and an application crate that consumes the generated types (CALL-02) |
| `tools/xtask` | `contract export/check/diff/list`, `migrate-v3`, `docs-check`, `release-audit`, `policy`, `fixtures`, `msrv-ui`, `source-export` |
| `tools/perf` | Expansion-scale, overhead and stream-driver measurements |

## Compiler pipeline

```text
TokenStream
-> syntax AST with retained spans and docs          (syntax/*)
-> owner/endpoint tree, values registered            (semantic/tree.rs)
-> credential sources, secrets, rate registry         (semantic/auth.rs, rates.rs)
-> per owner then endpoint: LANG-03 references,       (semantic/head.rs, auth.rs, rates.rs)
   normalized query/header programs, PATCH-02
   preconditions, header ownership, auth, rates
-> representations, status/effect/absence,            (semantic/endpoint.rs)
   pagination, operational identities
-> liveness, value controls, generated names,         (semantic/names.rs, surface.rs, limits.rs)
   collisions, static head bounds, limiter need
-> one resolved Model                                 (semantic/model.rs)
-> generated client, owners, endpoints, head tables,  (emit/*)
   contract-builder input and rustdoc
```

The resolved `Model` is the macro's single source of meaning: every generated
accessor, patch table, route, auth/rate declaration, documentation card and
contract-description input is emitted from it. Emitters never infer semantics
from type tokens (no `Option`/`Vec` recognition) or from generated strings.
Diagnostics carry a stable `[C3-…]` code (`syntax/diagnostics.rs`).

## Generated surface and the bridge

- Generated code targets the façade's versioned `__private` bridge; the bridge
  checks `GENERATED_CONTRACT_VERSION` (currently 34) during const evaluation, so
  a macro/core mismatch is a deterministic compile error (BRIDGE-01).
- Every client and scope handle implements core's `OwnerProjection`, the single
  source of what an endpoint inherits: host labels, path segments, the owner's
  query/header program segments, effective auth placements, rate declarations
  and the effective timeout. Generated endpoints and runtime endpoints bound at
  the same owner use the same projection. A projection is not an authenticity
  token.
- Named requests are nominal types emitted in the consuming crate; they own a
  core `Request<E>` through private fields and implement `RequestControls`,
  `IntoFuture` and their own value controls (ADR 0026).
- Contract descriptions: the macro passes only local, as-written facts to
  core's `ContractBuilder` (built once in a `LazyLock`); core's inspection
  resolver derives every effective view with the same composition laws and
  validates it. The macro semantic model and the core resolver therefore both
  implement the composition laws; their agreement is covered by the inspection
  tests and the generated-contract tests in `concord-inspect`.

## Core request model

- `request/program.rs` — symbolic query/header patch programs. Outer presence
  and every structural operation are resolved on a symbolic occurrence list
  before any encoder runs; only surviving emissions are encoded, in final
  order; sensitivity is sticky per target key (PATCH-01…03, PRIVACY-01).
- `request/head.rs`, `request/prepare.rs` — route/DNS composition, bounded
  encoding, real and redacted head projections, plan construction.
- `request/owner.rs` — `OwnerProjection`.
- `request/runtime_endpoint/` — runtime endpoints: construction records a
  bounded symbolic program; `bind` validates owner-level structure, auth/status
  relationships, generated-ID collisions and limiter presence; the runtime
  program is one more program segment after the owner chain (ADR 0024).
- `request/pending.rs` — `Request<E>` and the erased terminals
  `ResponseRequest<O>` and `AcceptedBytesRequest`.
- `controls.rs` — `RequestControls`, `CallOptions`, `CancellationToken`,
  `ControlSummary`; timeout selection versus caps.
- `value.rs` — closed `IntoValue<T>` conversions.

## Execution

- `execute/mod.rs` — one statically typed engine. Every direct request,
  traversal page and credential-storage execution runs through
  `execute_finalized_operation`; direct requests and pages use its identity
  form `execute_prepared_operation`. There is no second executor. The identity
  form and the direct-request terminals return the inner future from plain
  functions (no forwarding `async fn` or `async` block), so a request or a
  prepared operation is not stored twice in the boxed state machine (R3). The optional
  synchronous finalization step (`Finalization`) runs inside the operation's
  journal and cancellation scope after the exchange produced its accepted
  value and before the journal completes; `Finalization::decide_caps(stage)`
  enters a failure stage and decides cancellation, then the deadline, before a
  local effect. Its value or error is the operation's result (`Executed<R>`;
  `ExecutedPage<T>` is its alias and carries no journal snapshot).
- `execute/journal.rs` — the bounded operation journal: operation identity,
  phase, last attempt, last response status and monotone aggregate
  `DispatchKnowledge` (`NotDispatched < PossiblyDispatched < ResponseObserved`),
  with a provisional handoff token that only a core-native proof can settle as
  not dispatched. Completion fixes the complete result — including a
  finalization effect such as credential installation — before the single
  final observation (EXEC-04); a synchronous drop guard records cancellation
  without detached work.
- `execute/store.rs` — `store_as_<credential>()` wraps the same bound source
  request; local installation is its finalization step (ADR 0021, review F01).
  `install_material` decides the caps at `CredentialInstall` before and after
  the material's `expires_at()` read, then installs under the intent-epoch
  rule (`SlotSelector::install_with_expiry`); the storage result (`Installed`,
  `Superseded` or the error) is fixed and observed once, and spent material
  (`SpentMaterial`, including `ReleasedMaterial` handed out by the slot) and
  the borrowed store reference are released only afterwards.
- The normative stage order is spec §13.2: first poll, public selection, public
  rendering, body preparation, credential preflight, attempt admission, final
  freshness, native handoff, response head, result branch, completion.
  Planning shares only the selection/rendering stages (EXEC-03).

## Subsystem owners

| Concern | Owner |
|---|---|
| Body preparation and media authority | `body/spec.rs` (`RequestRepresentation`, `PageRequestRepresentation`), `body/prepared.rs` (`PreparedBody`), `body/stream.rs`, `body/multipart.rs` |
| Response drivers, absence, leases | `response/spec.rs` (sealed `Buffered`/`Streaming`/`DiscardDriver`, static `ACCEPT`), `response/absence.rs` (`OptionalOutcome`), `response/lease.rs` (`StreamLease`: total lease and pending-read idle, one ordered `check(now)`), `response/stream.rs` (`StreamCore` precheck and completion check), `response/summary.rs`, `records/stream.rs` (`ItemStream` framing over the same core) |
| Credentials | `auth/slot.rs` (intent epoch versus installed version, provider single flight; `install_with_expiry` hands released material out undropped as `ReleasedMaterial`; a retired flight is told to re-evaluate before retired material is dropped), `auth/version.rs` (`StoreOutcome`, `CredentialVersion`), `auth/provider.rs`, `auth/origin.rs` (provider helper allowlist) |
| Rate limiting | `rate_limit/schema.rs` (policy schemas, `LimiterCapability`, `Enforcement`), `rate_limit/canonical.rs` (tagged length-framed key bytes), `rate_limit/declaration.rs`, `rate_limit/limiter.rs` |
| Retry and server delay | `retry/policy.rs`, `retry/budget.rs`, `retry/server_delay.rs` |
| Pagination | `pagination/controller.rs`, `pagination/engine.rs`, `pagination/terminals.rs` (`Pages`, `CollectPrefix`, `CollectAll`, `Prefix`), `pagination/summary.rs`, `pagination/identity.rs` |
| Errors | `error/journal.rs` (value-free journal facts), `error/category.rs`, `error/status.rs` (complete-only status bodies) |
| Observation and redaction | `hooks/` (read-only `OperationObserver`), `redaction/` |
| Configuration and limits | `runtime/config.rs` (primitive-count setters, ADR 0025), `http/config.rs`, `limits.rs` |
| Inspection model | `inspection/` (`ContractDescription`, `ContractBuilder`, resolver) |

## Time and randomness

Core owns operation deadlines, retry jitter, HTTP-date/server-delay parsing
(one capture per response head), credential expiry, rate windows and stream
leases. The ambient wall clock is read only in `runtime/wall_clock.rs`
(enforced by `release-audit`). Extras do not read the clock independently for a
decision core owns.

Stream time limits (STREAM-02/03, ADR 0011, review F02) are one predicate,
`StreamLease::check(now)`: the total lease first (`now >=` its end is
`LeaseExpired`, even when the idle budget ran out too), then, only while a
pending read is outstanding (`pending_since` set), its cumulative idle
(`>=` the limit is `IdleTimeout`). `StreamCore::precheck` evaluates it at the
start of every poll, after the cancellation check, before the source is polled
again or a staged record is delivered (nothing is read or counted once a limit
has expired). `poll_chunk` reads the clock once per observed read completion
and evaluates it again before any branch accepts that completion (data, empty
chunk, end of body, read failure), whether or not the read was ever pending; a
withheld nonempty chunk was read and counts in `acquired_bytes`. Cancellation
is not judged at completion; it is observed at the next poll. The pending
baseline lives in the handle, so an abandoned `next` future keeps charging it;
`on_empty(now)` folds an empty chunk's span into the cumulative idle without
resetting it (and takes the baseline), and at the cooperative yield after 32
empty chunks in one poll turn `on_yield(now)` sets a new pending baseline at
that same `now` (the acquisition stays pending), so the time until the next
poll counts as idle, also when `next` is dropped at the yield (no timer is
armed; the yield wakes the task at once). Only accepted nonempty data
(`on_data`) resets idle. On expiry the stream yields exactly one correlated
error, records `Failed` in its summary, then returns `None`; nothing is
resent.

## Test architecture

- Macro semantic unit tests (`crates/concord-macros/src/semantic/tests.rs`).
- Trybuild UI suites (`crates/concord/tests/ui/`, `crates/concord-core/tests/ui/`).
- Core unit and runtime tests (executor, journal, auth slots, pagination,
  runtime endpoints, bodies, rate limiting).
- Façade integration tests (`crates/concord/tests/`: `generated`, `runtime`,
  `extensions`, `public_api`, `current_language`, `v3_*`).
- Examples and state-equivalence recordings (`crates/concord-examples/tests/`).
- Two-crate downstream tests (`tests/downstream/`).
- Tool tests (`tools/xtask`: migration, contract CLI) and `concord-inspect`
  tests.

Test support uses the same public and core contracts and never adds a
production-inaccessible native mutation path. Test obligations are indexed by
the spec §27 IDs (V3-L01…V3-O08); many tests carry the ID in a comment.
