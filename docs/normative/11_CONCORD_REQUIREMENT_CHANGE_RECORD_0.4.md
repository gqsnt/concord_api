# CONCORD_REQUIREMENT_CHANGE_RECORD_0.4

**Version:** 0.4
**Status:** Normative Implementation Candidate
**Authority:** Phase 13 requirement lineage for lexically scoped rate limits
**Depends on:** Specification Index, Product Contract, Requirement Change Records 0.2 and 0.3, owning normative documents
**Supersedes:** None

## 1. Purpose and authority

This record establishes exact lineage for the user-approved `CONCORD_PHASE_MISSION`:
`Phase 13 — Lexically Scoped Rate Limits`.

The authorizing Phase 13 directive changes the normal generated rate-limit declaration model and
generated contract from 13 to 14. It preserves the proven limiter/governor lifecycle and the
advanced explicit-key API. A retained row below keeps its identifier with a new current paragraph
version. A superseded row remains historically resolvable but is not active. New rows have no prior
paragraph. Historical evidence remains valid only for the exact old paragraph version and immutable
source identity recorded by that evidence. No Phase 0–12 evidence row is modified by this record.

Prior paragraph digests are lowercase SHA-256 digests of the exact UTF-8 normative paragraph at the
Phase 13 starting source. They are recorded here to make old evidence resolvable without treating it
as proof of the current paragraph.

## 2. Retained with explicit version change

| Prior ID | Prior title | Prior paragraph digest | Normative owner | Disposition / replacement | Implementation phase |
|---|---|---|---|---|---|
| `PROD-GUAR-010` | Fail closed | `8065c15cfe2e1f95ced9c7b730b63ff043e8bb613e2f461acf46d446a47522a7` | `01_CONCORD_PRODUCT_CONTRACT_0.1.md` | retained with explicit version change as `PROD-GUAR-010` | 13 |
| `PROD-STATE-001` | Immutable runtime selection | `f28be095bc3f94b3eb0ceabcacdec772e7439d657e738dcfcb0fe761d1741ded` | `01_CONCORD_PRODUCT_CONTRACT_0.1.md` | retained with explicit version change as `PROD-STATE-001` | 13 |
| `DSL-RATE-001` | Positive windows | `3911cceda481abbe972760f8947779e33c3b45d2feb09a5d3eae46ed02b38f79` | `02_CONCORD_DSL_SPEC_0.1.md` | retained with explicit version change as `DSL-RATE-001` | 13 |
| `DSL-RATE-002` | Valid cost | `51c69f6f4e6322d106d759856ab6344b988eb58b4c78b095c0ad4d695837749c` | `02_CONCORD_DSL_SPEC_0.1.md` | retained with explicit version change as `DSL-RATE-002` | 13 |
| `API-RATE-004` | Plan inspection | `9e20a93998ea0f6addf72aee5f8c7663cac2077fd04d871dfc417da863cfc55f` | `03_CONCORD_PUBLIC_API_SPEC_0.1.md` | retained with explicit version change as `API-RATE-004` | 13 |
| `GEN-BUILD-004` | Observer ownership | `9035bfa8c706cb013088c002c50f8564283def726cc577edb585c54c9e81d8e6` | `04_CONCORD_GENERATED_API_SPEC_0.1.md` | retained with explicit version change as `GEN-BUILD-004` | 13 |
| `GEN-DOC-001` | Public-only documentation | `0f2590b3c9addbddd7bb46c36bbbb560e2ed645a214f0fa5283bc420b33497e9` | `04_CONCORD_GENERATED_API_SPEC_0.1.md` | retained with explicit version change as `GEN-DOC-001` | 13 |
| `GEN-DOC-002` | Rustdoc secret exclusion | `e6b9d1f8e2369442c1521e863f23dfc8fe874c752f6c3ec37f6d35523d2324d9` | `04_CONCORD_GENERATED_API_SPEC_0.1.md` | retained with explicit version change as `GEN-DOC-002` | 13 |
| `SEC-RATE-001` | Built-in key redaction | `65e4203e84586843850a1db05f6b11763759617249dff0dfa8979a13128b1ce0` | `05_CONCORD_SECURITY_AND_LIMITS_SPEC_0.1.md` | retained with explicit version change as `SEC-RATE-001` | 13 |
| `SEC-RATE-002` | Rate-key diagnostic exclusion | `85d5b658a85907f3433d349b32cacd6d90d91a245668e9cd2b1367d0143f1409` | `05_CONCORD_SECURITY_AND_LIMITS_SPEC_0.1.md` | retained with explicit version change as `SEC-RATE-002` | 13 |
| `BOUND-RATE-005` | Key bytes | `e47b823bd3c8849cc116b7d4c5a27feba28375abd78f67c1b9362c278dc9cb07` | `05_CONCORD_SECURITY_AND_LIMITS_SPEC_0.1.md` | retained with explicit version change as `BOUND-RATE-005` | 13 |
| `EXEC-RATE-001` | Per-attempt acquisition | `b46544821a2c97bd6717ce6004cc0575c0132f50012db280ee61536ab76c8722` | `06_CONCORD_EXECUTION_SPEC_0.1.md` | retained with explicit version change as `EXEC-RATE-001` | 13 |
| `EXEC-RATE-002` | Atomic multi-rule reservation | `781942c4b2244e33b1f76106aaa039655d238e536a38dc94f1ef8c0094519fc3` | `06_CONCORD_EXECUTION_SPEC_0.1.md` | retained with explicit version change as `EXEC-RATE-002` | 13 |
| `EXEC-RATE-003` | No quota while waiting | `31c8e5a2e85ed7de9830a4d7c4de801e8310d33665571d618a03546a5cd6e69c` | `06_CONCORD_EXECUTION_SPEC_0.1.md` | retained with explicit version change as `EXEC-RATE-003` | 13 |
| `FEAT-GOV-001` | Fail-closed absence | `52cfeae11b28764dd7f01031c82bd476cb49039cb93c3d65c01e8224490aeb72` | `07_CONCORD_FEATURE_AND_COMPATIBILITY_SPEC_0.1.md` | retained with explicit version change as `FEAT-GOV-001` | 13 |
| `FEAT-GOV-002` | Advanced-plan fallback | `9ea2955974d0d8a6e8d33529b498894b5016814255af5e89da183acd68765384` | `07_CONCORD_FEATURE_AND_COMPATIBILITY_SPEC_0.1.md` | retained with explicit version change as `FEAT-GOV-002` | 13 |
| `CONF-COMPILE-001` | Canonical generated use | `b015e340a2446d85fc193584a95096a7e2b9d1acf4405921b15cce61989ac1bc` | `08_CONCORD_CONFORMANCE_PLAN_0.1.md` | retained with explicit version change as `CONF-COMPILE-001` | 13 |
| `CONF-COMPILE-002` | Advanced extension use | `47e19ec27a8c16f94a0ec6b25b19e93a80af843d066b961e63dce09a7a0d3dff` | `08_CONCORD_CONFORMANCE_PLAN_0.1.md` | retained with explicit version change as `CONF-COMPILE-002` | 13 |
| `CONF-COMPILE-003` | Diagnostic fixtures | `14ae664a44193d67b81654b605b5d976a8d38290294645de66624a38c150c4d8` | `08_CONCORD_CONFORMANCE_PLAN_0.1.md` | retained with explicit version change as `CONF-COMPILE-003` | 13 |
| `CONF-DOWNSTREAM-002` | Extension surface sufficiency | `07c1d9f40102031172a6e099fb0256989a9539f263f80936b163125143c51031` | `08_CONCORD_CONFORMANCE_PLAN_0.1.md` | retained with explicit version change as `CONF-DOWNSTREAM-002` | 13 |
| `CONF-SEC-001` | Source-chain scan | `207062759609172ff5e363fa2191bc9e7e71b5f780184cad088f8f643bb2b5ef` | `08_CONCORD_CONFORMANCE_PLAN_0.1.md` | retained with explicit version change as `CONF-SEC-001` | 13 |
| `CONF-MSRV-001` | Public MSRV | `8dd7e4be0ef86cb2fe1c4b2631f6edd1ea1341171e114eebd4f46a3ae34792a7` | `08_CONCORD_CONFORMANCE_PLAN_0.1.md` | retained with explicit version change as `CONF-MSRV-001` | 13 |
| `CONF-DOC-001` | No uncompiled canonical example | `061747fb0d65563f1dcd3e9d6d389bd27662545eccf2563fd8805fbd067186c0` | `08_CONCORD_CONFORMANCE_PLAN_0.1.md` | retained with explicit version change as `CONF-DOC-001` | 13 |

## 3. Superseded identifiers

| Prior ID | Prior title | Prior paragraph digest | Normative owner | Disposition | Replacement ID |
|---|---|---|---|---|---|
| `DSL-RATE-003` | Safe dimensions | `ad234a6dde43150be15f315e5602cf66a305cad76a4e4dec5fd56deef7f33666` | `02_CONCORD_DSL_SPEC_0.1.md` | superseded | `DSL-RATE-011` |
| `DSL-RATE-004` | Append semantics | `cf684b508bac8139525e3ce4893b67254f110d5ef92dffed7ff4faabeccd1377` | `02_CONCORD_DSL_SPEC_0.1.md` | superseded | `DSL-RATE-009` |
| `DSL-RATE-005` | Replace semantics | `ebe91dcbf521f7151184ff4ff69e013bdd85b2c710d8ce1bb7261f98a61b5351` | `02_CONCORD_DSL_SPEC_0.1.md` | superseded | `DSL-RATE-010` |
| `DSL-RATE-006` | Off exclusivity | `37d40c633364c5c47260427699f23a669136af8265bb3c169eb9cfcb336d4c97` | `02_CONCORD_DSL_SPEC_0.1.md` | superseded | `DSL-RATE-010` |
| `DSL-RATE-007` | Unique rate-limit template names | `95a75a77f21029c80f9d366b756a4d8f465e032a31f94ae65d619fedbdf82c36` | `02_CONCORD_DSL_SPEC_0.1.md` | superseded | `DSL-RATE-011` |

The five old paragraphs are removed from the current DSL owner during the authority amendment; the
implementation records their exact starting-source digest before the authority commit. Their old
Phase 0–12 evidence stays byte-identical and historical.

## 4. New Phase 13 active identifiers

| ID | Exact title | Normative owner | Authorizing directive | Implementation phase |
|---|---|---|---|---|
| `PROD-MODEL-009` | Lexical rate-limit ownership | `01_CONCORD_PRODUCT_CONTRACT_0.1.md` | Phase 13 directive | 13 |
| `PROD-GUAR-015` | Value-independent generated quotas | `01_CONCORD_PRODUCT_CONTRACT_0.1.md` | Phase 13 directive | 13 |
| `PROD-NONGOAL-003` | No discontiguous generated quota grouping | `01_CONCORD_PRODUCT_CONTRACT_0.1.md` | Phase 13 directive | 13 |
| `DSL-RATE-008` | Complete lexical rule declaration | `02_CONCORD_DSL_SPEC_0.1.md` | Phase 13 directive | 13 |
| `DSL-RATE-009` | Lexical subtree composition | `02_CONCORD_DSL_SPEC_0.1.md` | Phase 13 directive | 13 |
| `DSL-RATE-010` | Ancestor preservation and no local opt-out | `02_CONCORD_DSL_SPEC_0.1.md` | Phase 13 directive | 13 |
| `DSL-RATE-011` | No templates or generated key dimensions | `02_CONCORD_DSL_SPEC_0.1.md` | Phase 13 directive | 13 |
| `DSL-RATE-012` | Independent client rate-limit observer | `02_CONCORD_DSL_SPEC_0.1.md` | Phase 13 directive | 13 |
| `API-RATE-005` | Generated and advanced key boundary | `03_CONCORD_PUBLIC_API_SPEC_0.1.md` | Phase 13 directive | 13 |
| `GEN-RATE-001` | Lexical rate-limit plan generation | `04_CONCORD_GENERATED_API_SPEC_0.1.md` | Phase 13 directive | 13 |
| `GEN-RATE-002` | Deterministic generated rule identity | `04_CONCORD_GENERATED_API_SPEC_0.1.md` | Phase 13 directive | 13 |
| `GEN-RATE-003` | Global generated rate-limit keys | `04_CONCORD_GENERATED_API_SPEC_0.1.md` | Phase 13 directive | 13 |
| `GEN-RATE-004` | No generated rate-limit Display bounds | `04_CONCORD_GENERATED_API_SPEC_0.1.md` | Phase 13 directive | 13 |
| `GEN-RATE-005` | Static limiter requirement | `04_CONCORD_GENERATED_API_SPEC_0.1.md` | Phase 13 directive | 13 |
| `GEN-RATE-006` | Lexical rate-limit Rustdoc | `04_CONCORD_GENERATED_API_SPEC_0.1.md` | Phase 13 directive | 13 |
| `SEC-RATE-003` | Generated plans contain no request values | `05_CONCORD_SECURITY_AND_LIMITS_SPEC_0.1.md` | Phase 13 directive | 13 |
| `EXEC-RATE-004` | Static lexical generated plan | `06_CONCORD_EXECUTION_SPEC_0.1.md` | Phase 13 directive | 13 |
| `EXEC-RATE-005` | Pagination reuses the lexical plan | `06_CONCORD_EXECUTION_SPEC_0.1.md` | Phase 13 directive | 13 |
| `COMPAT-MACRO-006` | Generated rate-limit value neutrality | `07_CONCORD_FEATURE_AND_COMPATIBILITY_SPEC_0.1.md` | Phase 13 directive | 13 |
| `CONF-RATE-001` | Lexical DSL and migration diagnostics | `08_CONCORD_CONFORMANCE_PLAN_0.1.md` | Phase 13 directive | 13 |
| `CONF-RATE-002` | Generated lexical bucket semantics | `08_CONCORD_CONFORMANCE_PLAN_0.1.md` | Phase 13 directive | 13 |
| `CONF-RATE-003` | Advanced explicit-key preservation | `08_CONCORD_CONFORMANCE_PLAN_0.1.md` | Phase 13 directive | 13 |
| `CONF-RATE-004` | Governor and limiter lifecycle preservation | `08_CONCORD_CONFORMANCE_PLAN_0.1.md` | Phase 13 directive | 13 |

## 5. Historical evidence effect

Phase 0–12 evidence rows and reports are immutable. Old rate-limit evidence resolves to its old
source and paragraph version, including generated contract 13 and template/dimension semantics.
Phase 13 evidence must identify the new paragraph, final immutable Phase 13 source commit, exact
tests/commands/features, and direct classification. The only permitted remaining limitation after
Phase 13 is Phase 14 product-completion and release-final closure.
