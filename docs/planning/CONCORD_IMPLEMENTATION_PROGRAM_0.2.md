# CONCORD_IMPLEMENTATION_PROGRAM_0.2

**Version:** 0.2
**Status:** Internal Implementation Candidate
**Authority:** Current delivery sequencing after Phase 5
**Depends on:** All normative and architecture documents
**Supersedes:** `CONCORD_IMPLEMENTATION_PLAN_0.1.md` for future execution

## Purpose

This program records the implementation state actually reached through Phase 5 and resets future
sequencing around the production vertical slice now in the repository. It does not change normative
behavior, authorize a later phase, or claim full Concord 0.1 conformance. The normative documents
remain `Normative Implementation Candidate`.

The 0.1 implementation plan remains available as historical evidence. Its unchecked tasks are not
an execution queue because several foundations were delivered in a different order.

## Completed phase ledger

| Phase | Verdict | Durable outcome | Accepted limitations |
|---|---|---|---|
| Phase 0 | `PHASE_0_COMPLETE_WITH_LIMITATIONS` | Workspace, macro/core contract spike, reduced auth/pagination models, Reqwest probes, and typed-executor proof. | Proof architecture was not a production endpoint runtime; governance and Rustdoc gates were later corrected. |
| Phase 1 | `PHASE_1_COMPLETE_WITH_LIMITATIONS` | Safe errors/redaction, finite configuration, private managed application/provider clients, deterministic final-call seam, and canonical Nextest/release architecture. | No generated client core, pending request, lifecycle execution, or successful-response pipeline existed. |
| Phase 2 | `PHASE_2_COMPLETE_WITH_LIMITATIONS` | First executable generated bodyless request: fallible generated construction, shared private core, stable pending wrappers, first-poll lifecycle, hooks/debug, managed Reqwest, bounded bytes/status handling, no-content, metadata terminal, and an API-specific sealed explicit-endpoint capability. | Only the explicitly authorized Phase 2 DSL and response subset is implemented; broad 0.1 evidence remains partial where later categories are absent, and the PR 2.3 uncommitted review patch omitted one subsequently reviewed committed file. |
| Phase 3 | `PHASE_3_COMPLETE_WITH_LIMITATIONS` | One buffered codec vertical slice: stable media/encode/decode contracts, Text, feature-owned JSON, downstream custom codecs, finite request bodies, bounded decoded successes, coherent media headers, POST body DSL, typed direct/metadata results, and preserved Bytes/NoContent/status behavior. The final corrected source type-gates `.response()` to metadata-capable modes, semantically proves that terminal in rendered Rustdoc, and completes source-only namespace proof across prelude, advanced, and dangerous. | Fifty broader requirement rows remain partial. Their unimplemented clauses include credential-storage privacy, public-header precedence, Brotli/Deflate runtime probes, full grammar/semantic coverage, Phase 4 streaming/factories/multipart, and provider/limiter/pagination/extension/release-wide variants owned by Phases 4-10. |
| Phase 4 | `PHASE_4_COMPLETE_WITH_LIMITATIONS` | One finite body architecture now covers buffered values, lazy request streams, reconstructible factories, inferred replayability, lazy typed response streams, and feature-gated request multipart. Generated `Stream<M>`/`Multipart` markers, Reqwest-owned multipart framing, stream-open completion, cancellation, no-default support, and maintained independent dangerous-feature isolation all use the existing request executor. | Forty-six source rows and one audit row remain partial where their text also requires Phase 5 shaping, Phase 6 recovery/providers, Phase 7 limiter behavior, Phase 8 pagination, Phase 9 prepared endpoints/extension certification, or Phase 10 raw successful responses and release-wide grammar, security, documentation, platform, and evidence closure. |
| Phase 5 | `PHASE_5_COMPLETE_WITH_LIMITATIONS` | One normalized compiler and generated runtime now support multiple clients, root endpoints, nested scopes, required/optional/default client-scope-endpoint values, immutable snapshots, sealed request extension traits through generated preludes, explicit endpoint parity, host/path/ordered-query/public-header shaping, timeout/retry, all standard methods, custom methods, aliases, and deterministic names on the existing first-poll executor. | Seventy-three source requirements and one audit row remain partial for complete compound clauses owned by Phase 6 credentials/auth/providers/recovery, Phase 7 limiting, Phase 8 pagination, Phase 9 prepared endpoints/extension certification, or Phase 10 positive raw success and release-wide closure. An independent controller correction reclassified 18 Phase 5 rows without changing product source. |

Phase reports remain the detailed evidence owners:

- `docs/conformance/PHASE_0_REPORT.md`
- `docs/conformance/PHASE_1_REPORT.md`
- `docs/conformance/PHASE_2_REPORT.md`
- `docs/conformance/PHASE_3_REPORT.md`
- `docs/conformance/PHASE_4_REPORT.md`
- `docs/conformance/PHASE_5_REPORT.md`

## Phase 4 production baseline

The original Phase 2 source baseline is commit `055d74718c443412a605bd880cfad9415a824f99`.
The integrated-audit-corrected source baseline is
`ea1ce944fe1d2622db7a66600d734ac0dc04784f`.
The tested Phase 3 buffered source is
`c9521f649c91df0a0bae9786cba101f4d526692b`. The controller-reviewed corrected Phase 3 source is
`7ac4a1c6e18a080f4810b93ae4bc6aa02f52ed8f`. The final controller-reviewed Phase 3 source is
`54a64cb4ba062453dea26dd6af1dff97eaa3406e`; the complete maintained release matrix passed
against that immutable commit. The immutable tested Phase 4 product source is
`6472bc01e55bbd4282a8d5e06deeb9126650f9c4`. Controller correction source/test commit
`909021e7b82436bdfdff1e1ffea67f3255e63c77` adds only dangerous-feature isolation tests and
release wiring; its complete maintained and focused matrices pass. The complete evidence-state is
the documentation commit containing this program and every final Phase 4 evidence ID; the
subsequent documentation-only anchor reanchors the five audit rows without changing counts or
classifications. Exact hashes are recorded in the anchored Phase 4 report and Git history without
requiring impossible commit self-reference.

Its executable path is:

```text
DSL
→ normalized macro model
→ generated client/facade/endpoint
→ concord_core::Request<E>
→ first-poll identity/deadline/head preparation
→ safe public head and instance body metadata then before-send hook
→ one buffered, streamed, factory, or Reqwest-owned multipart materialization
→ one private managed native-body accounting and error boundary
→ private managed Reqwest execution
→ bounded status/Bytes; unpolled NoContent; media-validated buffered decode; or bounded lazy StreamResponse<M>
→ completion outside the result-determining deadline
→ direct semantic value, DecodedResponse<T>, or StreamResponse<M>
```

The Phase 0 executor proof and inert generated `RequestHead` are gone. The retained `cfg(test)` auth
and pagination models still prove independent future invariants and do not duplicate a Phase 2
production representation.

Each generated API now owns a locally sealed endpoint capability. Facade endpoint methods and
explicit `client.request(endpoint)` calls use the same capability-constrained path, so foreign
generated endpoints, arbitrary application values, and downstream capability implementations fail
at compile time without a runtime client identifier.

Text and downstream custom codecs work without optional JSON dependencies. With `json`, the same
generic buffered path owns `Json<T>` and activates only `mime`, `serde`, and `serde_json`. Generated
POST endpoints accept body inputs last and use the same API-specific sealed endpoint capability for
facade and explicit execution. A generated-private sealed `MetadataResponse` capability exposes
`.response()` only for buffered, Bytes, and NoContent response modes; direct await remains
independent.

The one evolved `BodySpec`, `BodyMetadata`, `PreparedBody`, and managed native-body adapter serve
empty/buffered bodies, one-shot streams, reconstructible factories, and multipart recipes. Public
`StreamBody` supports bytes, arbitrary streams, async readers, files, and exact length. Managed
polling enforces checked exact/aggregate limits, preserves safe body-error precedence, and stops on
drop. Replayability is private and derived; no application control exists. Public `RequestBody`
wires factories into the production executor through internal endpoints without adding a factory
DSL marker or prepared public endpoint.

`StreamResponse<M>` is one response handler in the existing architecture. Known-length and media
validation occur before return, unknown bodies remain lazy, cumulative post-decompression limits
withhold the first excess chunk, and caller polling is outside the endpoint deadline. Returning a
stream emits `StreamOpened` exactly once; later failure/drop emits no endpoint completion. The
handler deliberately lacks `MetadataResponse`, so streaming endpoints expose neither `.response()`
nor pagination.

Under `multipart`, public advanced recipes/factories support text, bytes, and streams. Reqwest owns
serialization and boundary generation; Concord accounts the entire emitted body including framing
through the same managed wrapper. The feature activates no JSON, compression, or dangerous
capability and adds no response mode. The parser/model/codegen pipeline maps `body: Stream<M>`,
`body: Multipart`, and `-> Stream<M>` without a second compiler path; generated contract version 5
is synchronized with core. Renamed dependencies, nested/repeated invocations, ownership sealing,
feature diagnostics, local generated execution, and rendered Rustdoc all pass on Rust 1.85.

All source-only core Cargo failures, including development-namespace escape, are now
ordinary Trybuild UI cases. The retained `config_features` and `development_features` projects
contain only genuine feature-graph checks; dependency-topology fixtures remain Cargo projects. A
feature-specific ordinary UI harness proves the development executor is also absent from the
existing dangerous namespace, while rendered core Rustdoc proves that the supported public
`.response()` terminal remains visible without rendering its private capability bound. Phase 4
now makes a complete default/custom-retry/development-tools/raw-flag isolation matrix a mandatory
dependency of `just release`; every dangerous configuration uses `--no-default-features` and
enables exactly one dangerous flag. The raw flag remains topology-only in this phase.

The corrected Phase 4 source-evidence audit records 72 direct and 46 partial rows across 118 exact
requirements, plus four direct and one partial catalog/report-integrity rows. The final 123 Phase 4
rows are 76 direct and 47 partial; the full catalog remains 568 unique IDs. `P4-FEAT-006` is partial
because positive raw-response APIs are absent even though all dangerous flags are isolated. Partial clauses name
their future owner and do not claim public-header shaping, recovery, limiter, pagination, prepared
endpoints, complete extension certification, full grammar/security evidence, or release-platform
closure. Historical Phase 0-3 evidence is unchanged.

## Phase 5 production baseline

The approved endpoint-setter decision is commit
`255e1fd94f4614b5a3c02cda0f820c4eb56146c5`. The immutable tested Phase 5 source is
`c513710c1e6ae2333a58ec481c2b74a4a0529a5f`; every maintained recipe, including `just release`,
passes against that exact commit. The original evidence state is
`a8353a151a35b3ba8a3d8ebd689ad269e7b1d948`, and its original anchor is
`10139ef98319d52a7fa6c874c72883e61427e813`. The corrected complete evidence state is
`PENDING_CORRECTED_PHASE_5_EVIDENCE_STATE`; its later documentation-only anchor and full
classification details are recorded in `docs/conformance/PHASE_5_REPORT.md`.

The production compiler and execution path is now:

```text
DSL tokens
→ source AST
→ lexical symbol tables and resolved semantic references
→ normalized inherited client/scope/endpoint IR
→ deterministic generated names
→ immutable generated client/facade/endpoint snapshots
→ concord_core::Request<E>
→ optional endpoint-specific sealed extension methods from the generated client prelude
→ first-poll operation identity and deadline
→ one HeadBuilder and PreparedHead
→ validated host, path, ordered public query, public headers, and safe URL
→ body metadata and media ownership validation
→ before-send hook
→ existing body materialization and private managed Reqwest execution
→ existing bounded buffered, NoContent, or lazy stream response handling
→ completion and typed result
```

One `api!` invocation may generate multiple independently owned clients. Client-root endpoints,
nested scopes to depth 64, all required/optional/default public values, facade and explicit endpoint
parity, all standard methods, validated custom methods, aliases, and deterministic collision
diagnostics use one compiler pipeline. Client values resolve during build, scope values during
facade creation, and endpoint values during pending request creation; final public shaping remains
deferred to first poll. Arc-backed immutable snapshots avoid user-value `Clone` bounds.

Endpoint-specific request controls use public generated sealed extension traits. A single
`use client_support::prelude::*` imports deterministic client/full-scope/endpoint-qualified trait
aliases. The facade still returns `concord_core::Request<E>`, generic timeout/debug setters remain
inherent, and a single private consume-and-map bridge preserves the request core, options, body,
handler, and terminal capabilities without polling. Downstream trait forgery and cross-endpoint use
fail at compile time; no private core import appears in public Rustdoc.

The one evolved `HeadBuilder` preserves fixed scheme, port, and base path while validating dynamic
DNS labels and path segments, serializing ordered scalar/optional/vector public query pairs, and
building the logical and safe URLs. The existing `PreparedHead` owns the only public-header map.
Public `Accept` wins over codec defaults; codec/stream/multipart `Content-Type` ownership conflicts
fail before hooks. Inherited timeout declarations and request-local overrides remain distinct.
Application retry is disabled by omission/off, Reqwest default is explicit, dangerous custom retry
is feature-gated, and an overriding `.http(...)` selection prevents the DSL factory from running.

Generated contract version 6 is synchronized between macro and core. Normal/renamed dependencies,
nested/repeated invocations, contract mismatch, rendered Rustdoc, no-default, JSON, multipart,
streaming, ownership, and the maintained dangerous-feature matrix all pass. No authentication,
limiter, pagination execution, prepared endpoint, or positive raw-success API was added.

Phase 5 contributes 198 source-behavior rows (125 direct, 73 partial) plus five audit rows (four
direct, one partial). The complete Phase 5 total is 203 rows: 129 direct and 74 partial. The catalog
contains 771 unique IDs, all 504 normative IDs remain mapped exactly once in coverage, unknown IDs
are zero, and all 568 historical rows are byte-identical to the Phase 4 anchor.

### Phase 5 compound-evidence correction

An independent controller found that five rows classified only their Phase 5 portion instead of the
complete compound requirement. A clause-by-clause audit of all 143 direct Phase 5 source rows and
four direct audit rows found 13 additional overclaims. The following evidence rows are therefore
partial: `P5-PROD-003`, `P5-PROD-013`, `P5-PROD-014`, `P5-DSL-007`, `P5-DSL-044`,
`P5-DSL-050`, `P5-DSL-055`, `P5-DSL-056`, `P5-GEN-003`, `P5-GEN-018`, `P5-GEN-029`,
`P5-GEN-030`, `P5-GEN-032`, `P5-EXEC-009`, `P5-EXEC-012`, `P5-EXEC-013`, `P5-FEAT-008`,
and `P5-CONF-012`.

Their deferred clauses remain assigned to the established sequence: credentials, providers,
authentication, and recovery in Phase 6; limiter selection and execution in Phase 7; pagination in
Phase 8; prepared endpoints and full extension certification in Phase 9; and release-final Reqwest,
platform, documentation, security, evidence, and positive raw-success closure in Phase 10. The
correction changes evidence, coverage, report, and Program documentation only. Phase 5 product,
manifest, lockfile, recipe, and test source remain exactly the immutable
`c513710c1e6ae2333a58ec481c2b74a4a0529a5f` state. Phase 6 remains next and unstarted.

## Current non-goals

Phase 5 intentionally does not provide:

- authentication, credential providers, recovery, or generated credential surfaces;
- limiter waits, governors, or generated rate-limit configuration;
- pagination execution or generated pagination terminals;
- prepared public endpoints or dangerous raw success responses (the latter is explicitly owned by
  Phase 10);
- streaming terminal-consumption hooks, SSE, WebSocket, record/CSV streaming, bidirectional
  streaming, or multipart response parsing.

No placeholder public API was added for these capabilities.

## Revised future sequence

Future work should build outward from the single production request path rather than recreate proof
loops or parallel request models. A later phase requires its own approved contract map and mission.

1. **Phase 6 — authentication.** Integrate authentication and recovery using the retained reduced
   state-machine invariants.
2. **Phase 7 — rate limiting.** Integrate rate limiting and cooldown observation without exposing a
   public transport.
3. **Phase 8 — pagination.** Integrate sequential pagination using the retained identity/ceiling
   invariants and final hook contract.
4. **Phase 9 — prepared endpoints and extension certification.** Add prepared endpoints only after
   their public contract can reuse the same core executor, and certify the approved extension
   surfaces.
5. **Phase 10 — product completion and release candidate.** Implement the positive dangerous raw
   successful-response surface and complete examples, compatibility matrices, security/property
   evidence, performance work, and release-final conformance.

This sequence is planning context, not a Phase 6 implementation prompt. Phase 6 is not started.

## Continuing constraints

- Keep one production request/head/execution representation.
- Keep Reqwest request/response values and managed clients private.
- Keep deterministic support private and out of generated signatures.
- Use Nextest for unit/integration tests and `cargo test` only for maintained doctest recipes.
- Preserve the immutable source/evidence anchor sequence for each phase.
- Keep normative status at `Normative Implementation Candidate` until full release evidence exists.

`PHASE_5_COMPLETE_WITH_LIMITATIONS`

`PHASE_6_NOT_STARTED`
