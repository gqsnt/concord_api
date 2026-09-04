# ADR — Concord v2.1 implementation target

- **Status:** Selected for the v2.1 integration candidate / beta; release freeze pending evidence gates
- **Decision owner:** Concord project owner
- **Date:** September 2026

## Decision

Implement the independent hybrid v2.1 candidate directly from the supplied
v2.0.0 baseline at `f7464c5`.

## Decisive properties

1. Rust aliases/container names cannot change DSL presence or cardinality.
2. Presence, defaults, scalar/repeated placement, representation, status, and effect have separate owners.
3. Common endpoints are domain-shaped without repeated codec wrappers.
4. Local representation deviations are visible beside the affected body/result.
5. The result arrow is the exact awaited Rust type, including complete versus lazy lifecycle.
6. Endpoint status is finite static data; auth challenge remains before it.
7. Safe and idempotent effects remain distinct; pagination is safe-only.
8. Policy retry, auth recovery, physical transport recovery, and pagination continuation remain distinct finite authorities.
9. Pagination controller owns page item extraction and continuation without native response access.
10. Body page stability does not silently become aggregate prepared-body reuse.
11. Rate feedback affects future admission rather than rewriting a current result.
12. General observation is read-only and separated from stream termination.
13. Core alone owns native HTTP and every path uses one executor.
14. Effective inheritance is visible through docs, plans, provenance, and semantic diff.
15. Extras implement semantic roles rather than middleware escape hatches.

## Costs accepted

- breaking parser/generated/core/extras migration;
- new atom and representation roles;
- inherited representation metadata/fences;
- two lazy response carriers;
- richer effect model;
- page-response/header-interest/controller-step model;
- broad deterministic/security/DX/performance evidence;
- migration of every codec, record, body, response, hook, and example.

## Unsupported claims

Implementation has not yet proven:

- final representation trait coherence/diagnostics;
- inherited representation cognitive superiority;
- optimal optional convenience methods;
- stream erasure cost;
- full custom-representation sufficiency;
- operational value of the complete observer event set;
- prevalence/usability of optional static credentials;
- compile/binary cost of descriptions.

## Revisit triggers

- representation prototype cannot pass MSRV/coherence/local diagnostics;
- user tasks favor leaf-local representations, punctuation, or no `with_x`;
- a real API needs ordinary non-safe pagination;
- aggregate body reuse is necessary for acceptable performance;
- stream/metadata costs exceed budget;
- a legitimate cross-endpoint policy cannot be assigned to a narrow owner;
- optional auth construction repeatedly fails real tasks;
- an excluded HTTP feature becomes part of Concord’s deliberate product purpose.

## Superseded alternatives

- Agent A complete semantic-compression design;
- Agent B inherited representation with `...`/symbolic patch surface;
- current v1 contract and public API;
- hypothetical v2.0 as a release target.
