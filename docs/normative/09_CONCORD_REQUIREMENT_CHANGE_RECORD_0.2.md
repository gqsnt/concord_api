# CONCORD_REQUIREMENT_CHANGE_RECORD_0.2

**Version:** 0.2
**Status:** Normative Implementation Candidate
**Authority:** Requirement lineage for approved post-Phase-10 contract changes
**Depends on:** `00_CONCORD_SPECIFICATION_INDEX_0.1.md` and the affected normative owners
**Supersedes:** None

## 1. Purpose and authorization

This tracked record distinguishes current active requirements from superseded historical
identifiers without changing historical evidence. Its authorization source is the corrected
`CONCORD_PHASE_MISSION` user directive dated 2026-07-20, which approves Phase 11 — Uniform Typed
Pagination State and the prospective Phases 12-14 roadmap.

Paragraph digests below are lowercase SHA-256 values of the prior normalized requirement paragraph
at the validated Phase 10 source commit
`9317a94cf020d81dbd216ed2a7dc999550586898`. Normalization trims the paragraph and replaces each
whitespace run with one ASCII space, matching the maintained conformance audit.

## 2. Historical evidence rule

Every Phase 0-10 evidence row remains byte-identical and valid for the source commit recorded by
that row. A superseded identifier is resolvable through this record but is not an active current
coverage requirement. Retained identifiers name revised current paragraphs and preserve their old
evidence only as evidence of the prior paragraph at its recorded source identity. Current Phase 11
evidence names the active current identifiers and current normative owners.

## 3. Retained identifiers with explicit version changes

| Prior requirement ID | Prior title | Prior paragraph digest | Disposition | Replacement requirement ID | Affected normative owner | Authorization source | Implementation phase | Effect on historical evidence |
|---|---|---|---|---|---|---|---|---|
| `DSL-PAGE-002` | Unique built-in assignments | `4ea966edd376dc9c47253423c97cb719b1fa1f2305f6c69343517a29f1d88eca` | retained with explicit version change | `DSL-PAGE-002` | `02_CONCORD_DSL_SPEC_0.1.md` | Corrected `CONCORD_PHASE_MISSION`, 2026-07-20 | Phase 11 | Prior rows remain valid only for their recorded source; current proof targets unique controller fields and resolved bindings. |
| `API-PAGE-002` | Controller purity | `fe3ec685b4c3ba4dfa197d3c3ee9b98df0b8e2e8ee6db1e9205d4e1b4cac2663` | retained with explicit version change | `API-PAGE-002` | `03_CONCORD_PUBLIC_API_SPEC_0.1.md` | Corrected `CONCORD_PHASE_MISSION`, 2026-07-20 | Phase 11 | Prior rows remain valid only for their recorded source; current proof targets all three revised controller methods. |
| `SEC-PAGE-001` | Continuation secrecy | `c35edeff32063e2fcd316a98401b304f88f812c03823bedae4a9db0035e77b25` | retained with explicit version change | `SEC-PAGE-001` | `05_CONCORD_SECURITY_AND_LIMITS_SPEC_0.1.md` | Corrected `CONCORD_PHASE_MISSION`, 2026-07-20 | Phase 11 | Prior rows remain valid only for their recorded source; current proof classifies declared bound continuation values. |
| `SEC-PAGE-002` | Pagination diagnostic exclusions | `a1c1ec39b522988542e78f42e586b541a7ed4e171b70a3e6cf0ebe671fbb1c76` | retained with explicit version change | `SEC-PAGE-002` | `05_CONCORD_SECURITY_AND_LIMITS_SPEC_0.1.md` | Corrected `CONCORD_PHASE_MISSION`, 2026-07-20 | Phase 11 | Prior rows remain valid only for their recorded source; current proof covers every listed automatic surface. |
| `EXEC-PAGE-001` | Zero page no-op | `4fbe87f5cfa903789f51f85db5ce327b2c46da99e910e264baa08c46b70386e7` | retained with explicit version change | `EXEC-PAGE-001` | `06_CONCORD_EXECUTION_SPEC_0.1.md` | Corrected `CONCORD_PHASE_MISSION`, 2026-07-20 | Phase 11 | Prior rows remain valid only for their recorded source; current proof also excludes validation, advancement, and binding. |
| `EXEC-PAGE-005` | Controller commit after acceptance | `a0976d5f791e6834b125e6c8718b5dc18e55f7e40d83906afbf32ac164980e9d` | retained with explicit version change | `EXEC-PAGE-005` | `06_CONCORD_EXECUTION_SPEC_0.1.md` | Corrected `CONCORD_PHASE_MISSION`, 2026-07-20 | Phase 11 | Prior rows remain valid only for their recorded source; current proof follows revised termination precedence. |
| `EXEC-PAGE-007` | Same page recovery | `0d13f1a0fff64b94479f13255a4bda536c0d5d8ef5a47e148e65952d98d5b66c` | retained with explicit version change | `EXEC-PAGE-007` | `06_CONCORD_EXECUTION_SPEC_0.1.md` | Corrected `CONCORD_PHASE_MISSION`, 2026-07-20 | Phase 11 | Prior rows remain valid only for their recorded source; current proof retains the bound page identity and controller state. |

## 4. Superseded identifiers

| Prior requirement ID | Prior title | Prior paragraph digest | Disposition | Replacement requirement ID or IDs | Affected normative owner | Authorization source | Implementation phase | Effect on historical evidence |
|---|---|---|---|---|---|---|---|---|
| `DSL-PAGE-004` | Atomic custom construction | `8f2a8a34b610f543a767ad96968792253c9deb0a7dbeb4dbf4a381b8c41e7e02` | superseded | `DSL-PAGE-007` | `02_CONCORD_DSL_SPEC_0.1.md` | Corrected `CONCORD_PHASE_MISSION`, 2026-07-20 | Phase 11 | Historical rows retain their prior ID and recorded source; current coverage uses the replacement. |
| `DSL-PAGE-005` | Built-in binding completeness | `c3708e0c94d8507205fb612719de718f9865e280716b0b6a6152e4d961124929` | superseded | `DSL-PAGE-008` | `02_CONCORD_DSL_SPEC_0.1.md` | Corrected `CONCORD_PHASE_MISSION`, 2026-07-20 | Phase 11 | Historical rows retain their prior ID and recorded source; current coverage uses the replacement. |
| `DSL-PAGE-006` | Custom patch boundary | `94822158e0c60722740c111964c98a406ed872220de8b68697a5fe2c62248272` | superseded | `DSL-PAGE-009`, `DSL-PAGE-010` | `02_CONCORD_DSL_SPEC_0.1.md` | Corrected `CONCORD_PHASE_MISSION`, 2026-07-20 | Phase 11 | Historical rows retain their prior ID and recorded source; current coverage uses both replacements. |
| `API-PAGE-004` | Private controller state | `47094f4e4cb564460c6983f0c98911a468e1f07d3ebfed8a4868bc9ee2ff1f43` | superseded | `API-PAGE-007` | `03_CONCORD_PUBLIC_API_SPEC_0.1.md` | Corrected `CONCORD_PHASE_MISSION`, 2026-07-20 | Phase 11 | Historical rows retain their prior ID and recorded source; current coverage uses the replacement. |
| `API-PAGE-005` | No custom short-page inference | `06b091911c21011d98e6090faf9bd8e27d00602ad94cd3c6f30abaa2c707b3c8` | superseded | `API-PAGE-008` | `03_CONCORD_PUBLIC_API_SPEC_0.1.md` | Corrected `CONCORD_PHASE_MISSION`, 2026-07-20 | Phase 11 | Historical rows retain their prior ID and recorded source; current coverage uses the replacement. |
| `API-PAGE-006` | Safe patch boundary | `419efe495e3b1d5e20bdfa449e60630808ef2f6aae0e5bc15c8c9d988a32e4a6` | superseded | `API-PAGE-009` | `03_CONCORD_PUBLIC_API_SPEC_0.1.md` | Corrected `CONCORD_PHASE_MISSION`, 2026-07-20 | Phase 11 | Historical rows retain their prior ID and recorded source; current coverage uses the replacement. |
| `EXEC-PAGE-008` | Natural stop precedence | `9d5f651e6d9b7c556b26e469510daed000216efcb18e995a5fdb79319a788a3a` | superseded | `EXEC-PAGE-015` | `06_CONCORD_EXECUTION_SPEC_0.1.md` | Corrected `CONCORD_PHASE_MISSION`, 2026-07-20 | Phase 11 | Historical rows retain their prior ID and recorded source; current coverage uses the replacement. |
| `EXEC-PAGE-014` | Custom continuation authority | `404cdecf4d1b5c2372ee3a32a982f3d398fd69e171182109cb76d6eb8685fa56` | superseded | `EXEC-PAGE-016` | `06_CONCORD_EXECUTION_SPEC_0.1.md` | Corrected `CONCORD_PHASE_MISSION`, 2026-07-20 | Phase 11 | Historical rows retain their prior ID and recorded source; current coverage uses the replacement. |

## 5. New Phase 11 active requirements

| Current requirement ID | Current title | Normative owner | Authorization source | Implementation phase |
|---|---|---|---|---|
| `DSL-PAGE-007` | Complete controller struct construction | `02_CONCORD_DSL_SPEC_0.1.md` | Corrected `CONCORD_PHASE_MISSION`, 2026-07-20 | Phase 11 |
| `DSL-PAGE-008` | Inline live binding round trip | `02_CONCORD_DSL_SPEC_0.1.md` | Corrected `CONCORD_PHASE_MISSION`, 2026-07-20 | Phase 11 |
| `DSL-PAGE-009` | Writable binding boundary | `02_CONCORD_DSL_SPEC_0.1.md` | Corrected `CONCORD_PHASE_MISSION`, 2026-07-20 | Phase 11 |
| `DSL-PAGE-010` | Restricted controller initializer grammar | `02_CONCORD_DSL_SPEC_0.1.md` | Corrected `CONCORD_PHASE_MISSION`, 2026-07-20 | Phase 11 |
| `API-PAGE-007` | Declared controller-state visibility | `03_CONCORD_PUBLIC_API_SPEC_0.1.md` | Corrected `CONCORD_PHASE_MISSION`, 2026-07-20 | Phase 11 |
| `API-PAGE-008` | Controller-selected count termination | `03_CONCORD_PUBLIC_API_SPEC_0.1.md` | Corrected `CONCORD_PHASE_MISSION`, 2026-07-20 | Phase 11 |
| `API-PAGE-009` | Binding-only request influence | `03_CONCORD_PUBLIC_API_SPEC_0.1.md` | Corrected `CONCORD_PHASE_MISSION`, 2026-07-20 | Phase 11 |
| `API-PAGE-010` | Request-local controller bounds | `03_CONCORD_PUBLIC_API_SPEC_0.1.md` | Corrected `CONCORD_PHASE_MISSION`, 2026-07-20 | Phase 11 |
| `GEN-PAGE-004` | Associated typed controller integration | `04_CONCORD_GENERATED_API_SPEC_0.1.md` | Corrected `CONCORD_PHASE_MISSION`, 2026-07-20 | Phase 11 |
| `EXEC-PAGE-015` | Controller-selected natural-stop precedence | `06_CONCORD_EXECUTION_SPEC_0.1.md` | Corrected `CONCORD_PHASE_MISSION`, 2026-07-20 | Phase 11 |
| `EXEC-PAGE-016` | Uniform controller authority | `06_CONCORD_EXECUTION_SPEC_0.1.md` | Corrected `CONCORD_PHASE_MISSION`, 2026-07-20 | Phase 11 |
| `EXEC-PAGE-017` | Per-page controller validation | `06_CONCORD_EXECUTION_SPEC_0.1.md` | Corrected `CONCORD_PHASE_MISSION`, 2026-07-20 | Phase 11 |
| `SEC-PAGE-003` | Bound-state continuation classification | `05_CONCORD_SECURITY_AND_LIMITS_SPEC_0.1.md` | Corrected `CONCORD_PHASE_MISSION`, 2026-07-20 | Phase 11 |
| `COMPAT-MACRO-004` | Pagination type neutrality | `07_CONCORD_FEATURE_AND_COMPATIBILITY_SPEC_0.1.md` | Corrected `CONCORD_PHASE_MISSION`, 2026-07-20 | Phase 11 |
| `CONF-PAGE-002` | Downstream controller without macro modification | `08_CONCORD_CONFORMANCE_PLAN_0.1.md` | Corrected `CONCORD_PHASE_MISSION`, 2026-07-20 | Phase 11 |

## 6. Roadmap effect

Phase 10 remains validated for its exact source and requirements. Phase 11 implements this record.
Phases 12, 13, and 14 remain prospective, and only Phase 14 performs whole-product conformance
closure and may return the package to `Normative Release Final`.
