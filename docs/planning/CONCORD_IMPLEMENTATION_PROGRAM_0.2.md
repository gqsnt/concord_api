# CONCORD_IMPLEMENTATION_PROGRAM_0.2

**Version:** 0.2
**Status:** Internal Implementation Candidate
**Authority:** Current delivery sequencing after Phase 3
**Depends on:** All normative and architecture documents
**Supersedes:** `CONCORD_IMPLEMENTATION_PLAN_0.1.md` for future execution

## Purpose

This program records the implementation state actually reached through Phase 3 and resets future
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

Phase reports remain the detailed evidence owners:

- `docs/conformance/PHASE_0_REPORT.md`
- `docs/conformance/PHASE_1_REPORT.md`
- `docs/conformance/PHASE_2_REPORT.md`
- `docs/conformance/PHASE_3_REPORT.md`

## Phase 3 production baseline

The original Phase 2 source baseline is commit `055d74718c443412a605bd880cfad9415a824f99`.
The integrated-audit-corrected source baseline is
`ea1ce944fe1d2622db7a66600d734ac0dc04784f`.
The tested Phase 3 buffered source is
`c9521f649c91df0a0bae9786cba101f4d526692b`. The controller-reviewed corrected Phase 3 source is
`7ac4a1c6e18a080f4810b93ae4bc6aa02f52ed8f`. The final controller-reviewed Phase 3 source is
`54a64cb4ba062453dea26dd6af1dff97eaa3406e`; the complete maintained release matrix passed
against that immutable commit.

Its executable path is:

```text
DSL
→ normalized macro model
→ generated client/facade/endpoint
→ concord_core::Request<E>
→ first-poll identity/deadline/head preparation
→ safe head and body metadata then before-send hook
→ one buffered body codec and finite request-limit validation
→ private managed Reqwest execution
→ bounded status/Bytes; unpolled NoContent; or media-validated buffered decode
→ completion outside the result-determining deadline
→ direct value or DecodedResponse<T>
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
independent. All source-only core Cargo failures, including development-namespace escape, are now
ordinary Trybuild UI cases. The retained `config_features` and `development_features` projects
contain only genuine feature-graph checks; dependency-topology fixtures remain Cargo projects. A
feature-specific ordinary UI harness proves the development executor is also absent from the
existing dangerous namespace, while rendered core Rustdoc proves that the supported public
`.response()` terminal remains visible without rendering its private capability bound.

The final evidence audit records 76 direct and 50 partial Phase 3 rows. In addition to public
header precedence, compression probes, extension certification, and complete grammar/semantic
coverage, generated credential-storage privacy and compound lifecycle/redaction requirements remain partial where their full text also
requires Phase 4 body modes, Phase 6 providers/recovery, Phase 7 limiter behavior, Phase 8 page
operations, Phase 9 extension certification, or Phase 10 release-wide proof. The coverage plan
names each current proof and future owner without moving public-header completion into Phase 3.

## Current non-goals

Phase 3 intentionally does not provide:

- authentication, credential providers, recovery, or generated credential surfaces;
- limiter waits, governors, or generated rate-limit configuration;
- pagination execution or generated pagination terminals;
- request/response streaming, multipart, or body factories;
- prepared public endpoints or dangerous raw success responses;
- query/public-header/host-prefix DSL clauses, custom methods, or the complete nested value system.

No placeholder public API was added for these capabilities.

## Revised future sequence

Future work should build outward from the single production request path rather than recreate proof
loops or parallel request models. A later phase requires its own approved contract map and mission.

1. **Phase 4 — streaming, factories, multipart, and replayability.** Add these finite and
   cancellation-aware boundaries while retaining the single Phase 3 body/executor architecture.
2. **Phase 5 — remaining client/scope/endpoint values and request shaping.** Complete query,
   public-header, host-prefix, and nested DSL compiler surfaces on the existing generated
   client/core bridge.
3. **Phase 6 — authentication.** Integrate authentication and recovery using the retained reduced
   state-machine invariants.
4. **Phase 7 — rate limiting.** Integrate rate limiting and cooldown observation without exposing a
   public transport.
5. **Phase 8 — pagination.** Integrate sequential pagination using the retained identity/ceiling
   invariants and final hook contract.
6. **Phase 9 — prepared endpoints and extension certification.** Add prepared endpoints only after
   their public contract can reuse the same core executor, and certify the approved extension
   surfaces.
7. **Phase 10 — product completion and release candidate.** Complete examples, compatibility
   matrices, security/property evidence, performance work, and release-final conformance.

This sequence is planning context, not a Phase 4 implementation prompt.

## Continuing constraints

- Keep one production request/head/execution representation.
- Keep Reqwest request/response values and managed clients private.
- Keep deterministic support private and out of generated signatures.
- Use Nextest for unit/integration tests and `cargo test` only for maintained doctest recipes.
- Preserve the immutable source/evidence anchor sequence for each phase.
- Keep normative status at `Normative Implementation Candidate` until full release evidence exists.

`PHASE_3_COMPLETE_WITH_LIMITATIONS`

`PHASE_4_NOT_STARTED`
