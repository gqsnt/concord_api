# Concord Phase [ID] — [Title]

**Status:** Proposed | Approved | Implemented | Superseded
**Authority:** Delivery boundary for Phase [ID]
**Depends on:** [approved documents and validated phase]
**Supersedes:** [document or none]

## 1. Objective

State one coherent product outcome. Describe the user-visible or architectural capability completed
by the phase, not the implementation activity.

## 2. Verified starting boundary

Reference, without duplicating:

- latest validated phase report;
- immutable source, evidence, and anchor commits;
- current implementation program;
- relevant accepted limitations and follow-up;
- current production architecture that must be preserved.

## 3. Normative ownership and requirement inventory

List every exact requirement ID the phase intends to implement or strengthen. For each requirement,
identify its normative owner and intended evidence classification.

| Requirement ID | Normative owner | Phase claim | Intended evidence |
|---|---|---|---|
| `...` | `docs/normative/...` | exact clause | direct or partial |

Do not use wildcard requirement groups as evidence claims.

## 4. Acceptance scenarios

Describe complete externally observable workflows. Include normal, error, boundary, feature, public-
API, generated-API, and downstream-extension scenarios as applicable.

Each scenario must identify:

- starting configuration;
- public or generated operation;
- observable result;
- relevant requirement IDs;
- proof mechanism.

## 5. Included scope

List only behavior required for the phase objective and acceptance scenarios.

## 6. Explicit non-goals

Name adjacent later-phase capabilities that must remain absent. Do not add placeholder public APIs
for them.

## 7. Previous guarantees to preserve

Reference the high-risk contracts established by earlier phases, including public compatibility,
sealing or ownership, execution-path uniqueness, finite limits, redaction, feature isolation,
generated privacy, cancellation, and evidence integrity as applicable.

## 8. Public and generated contract

Record exact approved signatures, DSL forms, generated forms, feature ownership, export locations,
and compatibility expectations. Reference normative text rather than paraphrasing when exact syntax
is contractual.

## 9. Architecture invariants

Define semantic invariants such as the single production owner of execution or state. Permit private
helpers and adapters only when they cannot independently bypass the production state machine.

Identify existing concepts to retain, evolve, replace, remove, or defer.

## 10. Implementation outcomes

Define sequential outcomes rather than file-by-file instructions. A typical sequence is:

1. previous-phase follow-up required by this phase;
2. foundational public/private contracts;
3. production integration;
4. generated or public vertical slice;
5. cumulative audit, documentation, planning, and evidence.

## 11. Test oracle map

For each critical invariant, identify the appropriate combination of:

- unit or semantic compiler test;
- Trybuild/UI test;
- deterministic runtime test;
- real local integration test;
- feature/topology fixture;
- downstream consumer fixture;
- Rustdoc or public API audit;
- redaction sentinel;
- limit or cancellation boundary test.

## 12. Validation profile

Reference maintained repository recipes and add only focused phase-specific commands. State the
feature matrix and toolchain configurations that are required.

## 13. Evidence and documentation

Define:

- exact evidence ownership;
- direct versus partial boundaries;
- phase report requirements;
- coverage-plan updates;
- implementation-program and `PROJECT_CONTROL.toml` updates;
- immutable source/evidence/anchor sequence.

Historical accurate evidence remains immutable.

## 14. Strategic stop conditions

Stop only for unresolved public-contract, normative, security, compatibility, toolchain, or phase-
boundary decisions. Ordinary private design choices are implementation work.

## 15. Exit criteria

Separate:

### Product exit criteria

Observable required behavior and preserved contracts.

### Repository-integrity exit criteria

Architecture, feature, test, documentation, Git, and cleanliness requirements.

### Evidence-integrity exit criteria

Exact tested commit, validation record, evidence classification, historical preservation, and next-
phase-not-started requirements.
