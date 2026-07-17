# CONCORD_IMPLEMENTATION_PROGRAM_0.2

**Version:** 0.2
**Status:** Internal Implementation Candidate
**Authority:** Current delivery sequencing after Phase 2
**Depends on:** All normative and architecture documents
**Supersedes:** `CONCORD_IMPLEMENTATION_PLAN_0.1.md` for future execution

## Purpose

This program records the implementation state actually reached through Phase 2 and resets future
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
| Phase 2 | `PHASE_2_COMPLETE` | First executable generated bodyless request: fallible generated construction, shared private core, stable pending wrappers, first-poll lifecycle, hooks/debug, managed Reqwest, bounded bytes/status handling, no-content, metadata terminal, and final generated vertical slice. | Only the explicitly authorized Phase 2 DSL and response subset is implemented; the omissions below are future product work rather than failed Phase 2 criteria. |

Phase reports remain the detailed evidence owners:

- `docs/conformance/PHASE_0_REPORT.md`
- `docs/conformance/PHASE_1_REPORT.md`
- `docs/conformance/PHASE_2_REPORT.md`

## Phase 2 production baseline

The Phase 2 source baseline is commit `055d74718c443412a605bd880cfad9415a824f99`.

Its executable path is:

```text
DSL
→ normalized macro model
→ generated client/facade/endpoint
→ concord_core::Request<E>
→ first-poll identity/deadline/head preparation
→ lifecycle hooks and debug
→ private managed Reqwest execution
→ bounded status/Bytes or unpolled NoContent handling
→ completion outside the result-determining deadline
→ direct value or DecodedResponse<T>
```

The Phase 0 executor proof and inert generated `RequestHead` are gone. The retained `cfg(test)` auth
and pagination models still prove independent future invariants and do not duplicate a Phase 2
production representation.

## Current non-goals

Phase 2 intentionally does not provide:

- nonempty request bodies, text/JSON codecs, or custom success codecs;
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

1. Complete buffered request-body and success-codec support, including text/JSON feature boundaries
   and the corresponding DSL markers, without changing the Phase 2 lifecycle ownership.
2. Add streaming, body factories, and multipart with finite/cancellation-aware boundaries.
3. Complete the remaining client/scope/endpoint value, query, public-header, host-prefix, and nested
   DSL compiler surfaces on the existing generated client/core bridge.
4. Integrate authentication and recovery using the retained reduced state-machine invariants.
5. Integrate rate limiting and cooldown observation without exposing a public transport.
6. Integrate sequential pagination using the retained identity/ceiling invariants and final hook
   contract.
7. Add prepared endpoints only after their public contract can reuse the same core executor.
8. Complete examples, compatibility matrices, security/property evidence, performance work, and
   release-final conformance.

This sequence is planning context, not a Phase 3 implementation prompt.

## Continuing constraints

- Keep one production request/head/execution representation.
- Keep Reqwest request/response values and managed clients private.
- Keep deterministic support private and out of generated signatures.
- Use Nextest for unit/integration tests and `cargo test` only for maintained doctest recipes.
- Preserve the immutable source/evidence anchor sequence for each phase.
- Keep normative status at `Normative Implementation Candidate` until full release evidence exists.

`PHASE_3_NOT_STARTED`
