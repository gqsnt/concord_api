# Repository change map — v2.1 → v3

**Status:** Active v3 source map for `3.0.0-beta.1`, organized by the v3
implementation specification §21.2. It records where each v3 responsibility
lives in this tree relative to the v2.1 baseline (branch `baseline-v2.1`). The
planned v2.0 → v2.1 map that previously occupied this file is historical and
retrievable with `git show baseline-v2.1:dev_doc/repository_change_map.md`.
Responsibility boundaries are described in [`internals.md`](internals.md).

## `crates/concord-macros`

The v2.1 file split was replaced; responsibilities moved as follows.

| v3 path | Responsibility | v2.1 predecessor |
|---|---|---|
| `syntax/ast.rs`, `syntax/declaration.rs` | Canonical v3 tokens and tree: client/scope/endpoint, parameter lists with direct placement, bodies, `via`, spans | same files |
| `syntax/values.rs` | Attributes (`#[default]`, `#[sensitive]`, docs, deprecation), `?` presence, stable initializers | same file |
| `syntax/request.rs` | Relative path templates, DNS atom lists, query/header blocks with `replace`/`when_present`/`append`/`remove`, timeouts, atoms | same file (path arrays retired) |
| `syntax/credentials.rs` | Credential sources and placements; rejects `secret NAME?;` | same file |
| `syntax/rate_limits.rs` | `windows` sets, `policy` with `id`/`key`/`cost`/`using`, attachments | same file |
| `syntax/pagination.rs` | `paginate via C { init … ; cursor -> … ; }` | same file |
| `syntax/diagnostics.rs`, `syntax/keyword.rs` | `[C3-…]` diagnostic codes, retired-form migration diagnostics | same files |
| `semantic/tree.rs` | Owner/endpoint tree flattening and value registration | `semantic/symbols.rs` |
| `semantic/head.rs` | Routes, DNS, normalized query/header programs, PATCH-02 preconditions, header ownership | `semantic/inheritance.rs` (in part) |
| `semantic/auth.rs` | Credential graph and effective placements | same file |
| `semantic/rates.rs` | Rate registry, attachments, deduplication, identity | `semantic/rate_limits.rs` |
| `semantic/endpoint.rs` | Representations, status/effect/absence, accepted bytes, identity, pagination facts | `semantic/pagination.rs`, `semantic/inheritance.rs` (in part) |
| `semantic/names.rs`, `semantic/surface.rs` | Liveness, value controls, generated names and collisions (CALL-01) | `semantic/surface.rs`, `semantic/symbols.rs` |
| `semantic/limits.rs` | Statically certain head-structure bounds | `semantic/head_limits.rs` |
| `semantic/model.rs` | The one resolved model | same file |
| `emit/client.rs` | Client type, builder, credential façade | `emit/client.rs`, `emit/credentials.rs` |
| `emit/owner.rs` | Scope/client owner modules, handles, unbound handles, projections, runtime `bind`/`plan_runtime` | `emit/scopes.rs`, `emit/unbound.rs` |
| `emit/endpoint.rs` | Endpoint specs, named requests, unbound intents, terminals, `RequestControls` impls | `emit/endpoints.rs`, `emit/requests.rs` |
| `emit/head.rs` | Route/DNS/patch-program tables, value readers, auth placements, rate declarations | `emit/shaping.rs`, `emit/bridge.rs` |
| `emit/contract.rs` | Local-fact input to core's `ContractBuilder` | `emit/descriptions.rs` |
| `emit/docs.rs` | Generated rustdoc cards (§19.2) | same file |

## `crates/concord-core`

| v3 path | Responsibility | Change from v2.1 |
|---|---|---|
| `request/program.rs` | Symbolic query/header programs evaluated before encoding | new |
| `request/owner.rs` | `OwnerProjection` shared by generated and runtime endpoints | new |
| `request/runtime_endpoint/` (`mod.rs`, `bind.rs`, `emission.rs`, `facts.rs`) | Runtime endpoints as symbolic programs bound through the owner projection | replaces `request/prepared.rs` |
| `request/head.rs`, `request/prepare.rs`, `request/pending.rs`, `request/options.rs` | Head composition and plans; `Request<E>`, erased `ResponseRequest<O>`, `AcceptedBytesRequest` | updated |
| `controls.rs` | `RequestControls`, `CallOptions`, `CancellationToken`, `ControlSummary` | new |
| `value.rs` | Closed `IntoValue<T>` conversions | new |
| `inspection/` | `ContractDescription`, `ContractBuilder`, resolver, owner/endpoint/rate/auth/value views | replaces `contract.rs` |
| `execute/mod.rs` | One managed engine for direct, page and storage operations: `execute_finalized_operation` with its synchronous `Finalization` step inside the journal (identity form `execute_prepared_operation` for direct requests and pages); `Executed<R>` (`ExecutedPage<T>` alias, no journal snapshot) | updated |
| `execute/journal.rs`, `error/journal.rs` | Bounded operation journal and value-free error facts | new |
| `execute/store.rs` | `StoreRequest` and explicit storage; installation as the finalization step (`install_material`: caps before and after the `expires_at()` read, then `install_with_expiry`), `SpentMaterial` released after the observed result | new |
| `auth/slot.rs`, `auth/version.rs`, `auth/recovery.rs`, `auth/origin.rs` | Intent epoch versus installed version (`install_with_expiry` returning `ReleasedMaterial`; retired flights re-evaluate before retired material is dropped), `StoreOutcome`, source-aware recovery, provider helper origins | `version.rs`, `recovery.rs`, `origin.rs` new |
| `body/spec.rs`, `body/prepared.rs`, `body/stream.rs`, `body/multipart.rs` | Single `prepare` → `PreparedBody` media authority; deprecated `RequestBody` alias | updated |
| `response/absence.rs`, `response/lease.rs`, `response/summary.rs` | `absent on` and `OptionalOutcome`; finite stream leases (`StreamLease::check(now)`: total lease, then pending-read idle; `on_empty(now)`, `on_yield(now)`); `StreamSummary` (`acquired_bytes`, `StreamErrorKind`) | new |
| `response/spec.rs`, `response/handler.rs`, `response/stream.rs`, `response/metadata.rs` | Static `ACCEPT`, sealed drivers, stream handoff (`StreamCore::precheck` and the completion check `reject_overdue_completion`), `BodyDisposition` | updated |
| `records/stream.rs` | `ItemStream<T>` framing over the same `StreamCore` lease and idle checks | updated |
| `pagination/controller.rs`, `engine.rs`, `terminals.rs`, `summary.rs`, `bridge.rs`, `meter.rs`, `identity.rs`, `limits.rs` | Init/Cursor controller contract; `Pages`/`CollectPrefix`/`CollectAll`; `TraversalSummary` | `engine.rs`, `terminals.rs`, `summary.rs`, `bridge.rs`, `meter.rs` new; `execute.rs`, `pending.rs` (`PageLimit`) removed |
| `rate_limit/schema.rs`, `rate_limit/declaration.rs`, `rate_limit/canonical.rs`, `rate_limit/limiter.rs` | Policy schemas and limiter capability; typed key encoding with `ENCODING_ID`; atomic admission contract | `schema.rs`, `declaration.rs` new |
| `retry/`, `runtime/config.rs`, `hooks/`, `redaction/`, `http/` | Replay authority, primitive-count configuration (§20.4), read-only observation, redaction, transport | updated |
| `__private.rs`, `generated_contract.rs` | Versioned generated-code bridge (contract version 34) | updated |

## Other crates and tools

| Path | v3 work |
|---|---|
| `crates/concord-extras/src/codec/not_found.rs` | New buffered `NotFound<R>` adapter |
| `crates/concord-extras/src/rate_limit/conformance.rs` | New deterministic shared-persistent limiter conformance fixture |
| `crates/concord-extras/src/pagination.rs`, `codec/json.rs`, `records/`, `auth/oauth2_client.rs`, `rate_limit/local.rs` | Public `Init` types; Json media rule and static `ACCEPT`; atomic local admission; provider origin declaration |
| `crates/concord/src/lib.rs` | §7.2 root re-exports and role modules |
| `crates/concord-inspect/` | New crate: `concord.contract/3` export, canonical JSON, diff axes, explanations, fixtures |
| `crates/concord-examples/src/contracts/catalog_v3.rs`, `src/application/`, `src/pagination/envelope.rs`, `src/riot/` | Canonical §22 Catalog, §23 application examples, §17.2 controller, meaning-preserving Riot conversion |
| `crates/concord-examples/tests/baseline_recording.rs` | v2.1 → v3 state-equivalence recordings and edit battery |
| `tests/downstream/` | New two-crate downstream package |
| `tools/xtask/src/contract/` | New `contract export/check/diff/list` with an explicit target registry (replaces v2.1 `semantic_diff.rs`) |
| `tools/xtask/src/migration/` | New `migrate-v3` source-aware migration tool (replaces the v2.1 `migration.rs` retired-pattern report) |
| `tools/xtask/src/migration/resolve.rs` | New: bounded crate name-resolution model (`CrateModel`) — module tree from crate roots and `#[path]`, items, `use` imports and globs with visibility, generic parameters and shadowing, the v2.1 generated surface placed at each `api!`, fields through their owning struct, functions through the named path, wrappers kept, `#[cfg]` uncertainty, crate completeness and foreign traits in scope (MIGRATE-03, review F03) |
| `tools/xtask/src/migration/callers.rs` | Caller rewrites behind one default-deny gate: setters (including raw `r#set_x`), `paginate` terminals, runtime `NonZero` arguments and `ClientBuilder` paths are edited only when proved to reach the generated item; macros, `#[cfg]`, dispatch completeness and cross-crate receivers are reported; setter candidates are discovered by v2.1 declared value and method role, with no `_if_some` suffix exemption (R3, B01); calls of names only v3 generates are reported (`C3-MIGRATE-GENERATED-NAME-SHADOW`, R3; bound-request `description` and the custom-provider builder setter, R4), with removed v2.1 members reported (`C3-MIGRATE-GENERATED-MEMBER-REMOVED`, R5) and the two-way inventory guarded by `migration::tests::surface_parity` against the generator's emitted members (`tools/xtask/fixtures/generated_surface/`, `crates/concord-macros/src/generated_surface.rs`, test-only; R4) |
| `tools/xtask/src/migration/rust_file.rs` | `api!` proof (an `api`-named invocation is migrated only when proved to be Concord's macro, else `C3-MIGRATE-API-MACRO-UNRESOLVED`); iterative nesting check before parsing (`MAX_NESTING` = 256, `C3-MIGRATE-NESTING-LIMIT`) |
| `tools/xtask/src/migration/mod.rs`, `report.rs`, `syntax.rs` | Per-crate `Layout` (package, library crate name, standalone file), the other crates' value names for cross-crate reports, the 256 MiB migration stack; manifest-based Concord dependency identity (`concord_identity`, shared `workspace_root`, R3); new codes `C3-MIGRATE-DEPENDENCY-IDENTITY`, `C3-MIGRATE-GENERATED-NAME-SHADOW`, `C3-MIGRATE-GENERATED-NAME-COLLISION` (R3), `C3-MIGRATE-PAGINATE-UNRESOLVED`, `C3-MIGRATE-BUILDER-PATH`, `C3-MIGRATE-BUILDER-PATH-UNRESOLVED`, `C3-MIGRATE-API-MACRO-UNRESOLVED`, `C3-MIGRATE-NESTING-LIMIT`; the declaration parser records `secret` names and `custom` credential sources |
| `tools/xtask/src/migration/tests.rs`, `tools/xtask/src/migration/tests/default_deny.rs` | Receiver-identity unit tests (`review_f03_*`, `receiver_identity_*`, `postfix_f03_*`) and the new default-deny module (`round3_f03_*`), each checking the complete caller text and every diagnostic |
| `tools/xtask/fixtures/migrate_v3/receiver_identity_v2_1.rs.txt`, `default_deny_v2_1.rs.txt` | New v2.1 compile-through fixtures in which unrelated types offer both `set_x` and `replace_x` (and `paginate`/`collect_prefix`) beside genuine generated calls; not baseline captures, so not listed in `SHA256SUMS` |
| `tools/xtask/tests/migrated_v3_identity/receiver_identity.rs`, `default_deny.rs` | Goldens: the expected tool output for those fixtures, compiled and executed by `tools/xtask/tests/migrated_v3.rs` through the real v3 `api!` |
| `tools/xtask/src/release_audit.rs` | Lifecycle check accepts `3.0.0-beta.N` and requires `dev_doc/release_gate_status.md` to name the exact version |
| `tools/perf/` | Expansion-scale measurement for v3 (`measurements/expansion_scale_linux_2026-09-26.txt`) |

## External review repairs (F01–F03)

Paths changed after the external review of the `fc3e4d1` return
(`git diff fc3e4d1 HEAD -- crates tools docs`). Status of the reopened
obligations: [`release_gate_status.md`](release_gate_status.md#external-review-of-the-fc3e4d1-return).

| Finding | Paths | Change |
|---|---|---|
| F01 — storage finalizes after local installation (V3-E15, V3-E21) | `crates/concord-core/src/execute/mod.rs`, `execute/store.rs`, `auth/slot.rs`, `auth/mod.rs` | Installation runs in the executor's finalization step inside the open journal; caps decided before installing; one final observation of the fixed storage result; spent material and the last store reference released afterwards; a cancelled leader publishes `Reevaluate` before dropping retired material (ADR 0021) |
| F01 tests | `crates/concord-core/src/execute/auth_tests/store.rs`, `crates/concord-core/src/auth/slot/tests.rs`, `crates/concord/tests/generated/authentication.rs` | `review_f01_*`, corrected `deadline_applies_through_local_installation`, `slow_final_observer_cannot_change_*`, cancellation and drop-panic tests, `v3_e15_*` |
| F02 — expired pending-read idle and lease end win at completion (V3-P10) | `crates/concord-core/src/response/lease.rs`, `response/stream.rs`, `response/summary.rs`, `records/stream.rs` | `StreamLease::check(now)` at precheck and at every observed completion; withheld chunks count as acquired; the empty-chunk yield keeps the acquisition pending (ADR 0011) |
| F02 tests | `crates/concord-core/src/response/stream/tests.rs` (with the paused-clock `TimedSource` added to the `test_support` module of `response/stream.rs`), `crates/concord-core/src/records/stream/tests.rs`, `crates/concord-extras/tests/contracts/record_streams.rs` | `review_f02_*`, `review_postfix_f02_*`, `v3_p10_overdue_*`, `v3_p10_postfix_f02_*` |
| F03 — migration receiver identity (V3-O04) | `tools/xtask/src/migration/resolve.rs` (new), `callers.rs`, `rust_file.rs`, `mod.rs`, `report.rs`, `syntax.rs` | Default-deny caller rewrites over a bounded name-resolution model (decision record, MIGRATE-03 receiver identity) |
| F03 tests and fixtures | `tools/xtask/src/migration/tests.rs`, `tools/xtask/src/migration/tests/default_deny.rs` (new), `tools/xtask/fixtures/migrate_v3/receiver_identity_v2_1.rs.txt` and `default_deny_v2_1.rs.txt` (new), `tools/xtask/tests/migrated_v3_identity/` (new goldens), `tools/xtask/tests/migrated_v3.rs` | Unit tests over complete caller text and diagnostics; compiled controls in which unrelated calls keep their v2.1 results |
| User documentation | `docs/auth.md`, `docs/execution_policy.md`, `docs/streams.md`, `docs/migration_v3.md` | The user-visible rules of the three repairs |
