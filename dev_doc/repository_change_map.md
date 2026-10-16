# V3 repository responsibility map

**Status:** current `3.0.0-beta.1` source map, updated 16 October 2026.
Concord V3 is the sole product target; see [ADR 0027](adrs/0027-v3-only-product.md).
This map locates implementation responsibilities. The
[internal architecture](internals.md) describes their boundaries.

## `crates/concord-macros`


| V3 path | Responsibility |
|---|---|
| `syntax/ast.rs`, `syntax/declaration.rs` | Canonical v3 tokens and tree: client/scope/endpoint, parameter lists with direct placement, bodies, `via`, spans |
| `syntax/values.rs` | Attributes (`#[default]`, `#[sensitive]`, docs, deprecation), `?` presence, stable initializers |
| `syntax/request.rs` | Relative path templates, DNS atom lists, query/header blocks with `replace`/`when_present`/`append`/`remove`, timeouts, atoms |
| `syntax/credentials.rs` | Credential sources and placements; rejects `secret NAME?;` |
| `syntax/rate_limits.rs` | `windows` sets, `policy` with `id`/`key`/`cost`/`using`, attachments |
| `syntax/pagination.rs` | `paginate via C { init … ; cursor -> … ; }` |
| `syntax/diagnostics.rs`, `syntax/keyword.rs` | `[C3-…]` diagnostic codes, unsupported-form diagnostics |
| `semantic/tree.rs` | Owner/endpoint tree flattening and value registration |
| `semantic/head.rs` | Routes, DNS, normalized query/header programs, PATCH-02 preconditions, header ownership |
| `semantic/auth.rs` | Credential graph and effective placements |
| `semantic/rates.rs` | Rate registry, attachments, deduplication, identity |
| `semantic/endpoint.rs` | Representations, status/effect/absence, accepted bytes, identity, pagination facts |
| `semantic/names.rs`, `semantic/surface.rs` | Liveness, value controls, generated names and collisions (CALL-01) |
| `semantic/limits.rs` | Statically certain head-structure bounds |
| `semantic/model.rs` | The one resolved model |
| `emit/client.rs` | Client type, builder, credential façade |
| `emit/owner.rs` | Scope/client owner modules, handles, unbound handles, projections, runtime `bind`/`plan_runtime` |
| `emit/endpoint.rs` | Endpoint specs, named requests, unbound intents, terminals, `RequestControls` impls |
| `emit/head.rs` | Route/DNS/patch-program tables, value readers, auth placements, rate declarations |
| `emit/contract.rs` | Local-fact input to core's `ContractBuilder` |
| `emit/docs.rs` | Generated rustdoc cards (§19.2) |

## `crates/concord-core`

| V3 path | Responsibility |
|---|---|
| `request/program.rs` | Symbolic query/header programs evaluated before encoding |
| `request/owner.rs` | `OwnerProjection` shared by generated and runtime endpoints |
| `request/runtime_endpoint/` (`mod.rs`, `bind.rs`, `emission.rs`, `facts.rs`) | Runtime endpoints as symbolic programs bound through the owner projection |
| `request/head.rs`, `request/prepare.rs`, `request/pending.rs`, `request/options.rs` | Head composition and plans; `Request<E>`, erased `ResponseRequest<O>`, `AcceptedBytesRequest` |
| `controls.rs` | `RequestControls`, `CallOptions`, `CancellationToken`, `ControlSummary` |
| `value.rs` | Closed `IntoValue<T>` conversions |
| `inspection/` | `ContractDescription`, `ContractBuilder`, resolver, owner/endpoint/rate/auth/value views |
| `execute/mod.rs` | One managed engine for direct, page and storage operations: `execute_finalized_operation` with its synchronous `Finalization` step inside the journal (identity form `execute_prepared_operation` for direct requests and pages); `Executed<R>` (`ExecutedPage<T>` alias, no journal snapshot) |
| `execute/journal.rs`, `error/journal.rs` | Bounded operation journal and value-free error facts |
| `execute/store.rs` | `StoreRequest` and explicit storage; installation as the finalization step (`install_material`: caps before and after the `expires_at()` read, then `install_with_expiry`), `SpentMaterial` released after the observed result |
| `auth/slot.rs`, `auth/version.rs`, `auth/recovery.rs`, `auth/origin.rs` | Intent epoch versus installed version (`install_with_expiry` returning `ReleasedMaterial`; retired flights re-evaluate before retired material is dropped), `StoreOutcome`, source-aware recovery, provider helper origins |
| `body/spec.rs`, `body/prepared.rs`, `body/stream.rs`, `body/multipart.rs` | Single `prepare` → `PreparedBody` media authority |
| `response/absence.rs`, `response/lease.rs`, `response/summary.rs` | `absent on` and `OptionalOutcome`; finite stream leases (`StreamLease::check(now)`: total lease, then pending-read idle; `on_empty(now)`, `on_yield(now)`); `StreamSummary` (`acquired_bytes`, `StreamErrorKind`) |
| `response/spec.rs`, `response/handler.rs`, `response/stream.rs`, `response/metadata.rs` | Static `ACCEPT`, sealed drivers, stream handoff (`StreamCore::precheck` and the completion check `reject_overdue_completion`), `BodyDisposition` |
| `records/stream.rs` | `ItemStream<T>` framing over the same `StreamCore` lease and idle checks |
| `pagination/controller.rs`, `engine.rs`, `terminals.rs`, `summary.rs`, `bridge.rs`, `meter.rs`, `identity.rs`, `limits.rs` | Init/Cursor controller contract; `Pages`/`CollectPrefix`/`CollectAll`; `TraversalSummary` |
| `rate_limit/schema.rs`, `rate_limit/declaration.rs`, `rate_limit/canonical.rs`, `rate_limit/limiter.rs` | Policy schemas and limiter capability; typed key encoding with `ENCODING_ID`; atomic admission contract |
| `retry/`, `runtime/config.rs`, `hooks/`, `redaction/`, `http/` | Replay authority, primitive-count configuration (§20.4), read-only observation, redaction, transport |
| `__private.rs`, `generated_contract.rs` | Versioned generated-code bridge (contract version 35) |

## Facade, extensions and inspection

| Path | Responsibility |
|---|---|
| `crates/concord/src/lib.rs` | Public V3 entry points, feature gates and generated-code bridge |
| `crates/concord-extras/src/codec/` | JSON and other explicit representation adapters |
| `crates/concord-extras/src/rate_limit/` | Local limiter and finite V3 shared-store conformance model |
| `crates/concord-extras/src/records/` | CSV and NDJSON record codecs |
| `crates/concord-inspect/src/` | `concord.contract/3` export, semantic check/diff and explanations |

## Examples, tests and tools

| Path | Responsibility |
|---|---|
| `crates/concord-examples/src/contracts/catalog_v3.rs` | Canonical V3 Catalog contract and public signatures |
| `crates/concord-examples/src/application/` | Construction, storage, controls, planning, traversal, streaming and write reconciliation |
| `crates/concord-examples/src/riot/` | 46-endpoint contract with regional routing and explicit policy sharing |
| `fixtures/contracts/` | Current V3 recording and canonical rate-key oracles |
| `crates/concord-examples/tests/baseline_recording.rs` | 92 Riot contexts and eight contract-edit controls |
| `crates/concord/tests/` | Generated public paths, native HTTP/TLS, body/provider regressions and UI diagnostics |
| `crates/concord-macros/tests/fixtures/generated_surface/` | Pure V3 generated-member inventory |
| `tests/downstream/` | Separate API and application crates consuming the public contract |
| `tools/xtask/src/contract/` | V3 export, check, diff and explicit target registry |
| `tools/xtask/src/docs_check.rs`, `doc_audit.rs` | Source-linked documentation and rendered Rustdoc checks |
| `tools/xtask/src/fixtures.rs`, `msrv.rs` | Feature and supported-compiler contract fixtures |
| `tools/xtask/src/source_export.rs`, `release_audit.rs` | Clean commit export and lifecycle/authority checks |
| `tools/perf/` | Current V3 expansion, construction, planning and execution probes |
| `scripts/verify_packaged_downstream.ps1` | Extracted archive consumers and Rustdoc validation |
| `justfile`, `.github/workflows/ci.yml` | Maintained feature/compiler/platform qualification |

## Historical changes

Earlier implementation and review history is retained in dated
[review records](reviews/2026_10_16_review.md) and Git. The conversion tool and
older-binary state import are retired; no active responsibility is assigned to
them. [The closure report](reviews/2026_10_16_corrections.md) records this
correction set and its verification.
