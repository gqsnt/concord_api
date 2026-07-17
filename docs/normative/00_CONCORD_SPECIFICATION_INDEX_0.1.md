# CONCORD_SPECIFICATION_INDEX_0.1

**Version:** 0.1  
**Status:** Normative Implementation Candidate  
**Authority:** Documentation governance and precedence  
**Requirement prefix:** `DOC`  
**Depends on:** None  
**Supersedes:** None

## 1. Purpose

This index defines the Concord 0.1 specification system. It identifies which documents may define product behavior, how conflicts are resolved, how normative requirements are identified, and how implementation evidence is linked to those requirements.

## 2. Normative language

The key words **MUST**, **MUST NOT**, **SHOULD**, **SHOULD NOT**, and **MAY** are normative.

- **MUST / MUST NOT** define conformance requirements.
- **SHOULD / SHOULD NOT** define strong recommendations whose exceptions require documented justification.
- **MAY** defines permitted optional behavior.

Informative examples never override normative prose.

## 3. Document hierarchy

The specification has four authority levels.

### 3.1 Product authority

`01_CONCORD_PRODUCT_CONTRACT_0.1.md` defines product identity, user mental model, high-level capabilities, global guarantees, safety classes, and explicit non-goals.

### 3.2 Specialized normative specifications

These documents own exact behavior in their domain:

| Document | Owns |
|---|---|
| `02_CONCORD_DSL_SPEC_0.1.md` | Accepted language, grammar, resolution, composition, diagnostics |
| `03_CONCORD_PUBLIC_API_SPEC_0.1.md` | Stable hand-written public types and extension traits |
| `04_CONCORD_GENERATED_API_SPEC_0.1.md` | Exact generated Rust names, signatures, ownership, and method availability |
| `05_CONCORD_SECURITY_AND_LIMITS_SPEC_0.1.md` | Data classification, redaction, threat model, exact limits and bounds |
| `06_CONCORD_EXECUTION_SPEC_0.1.md` | Runtime phases, side effects, recovery, precedence, cancellation |
| `07_CONCORD_FEATURE_AND_COMPATIBILITY_SPEC_0.1.md` | Features, MSRV, crate topology, stability, build matrix |
| `08_CONCORD_CONFORMANCE_PLAN_0.1.md` | Required evidence proving normative requirements |

### 3.3 Internal architecture

Architecture documents describe the intended implementation but do not define public behavior:

- `architecture/CONCORD_CORE_ARCHITECTURE_0.1.md`
- `architecture/CONCORD_MACRO_ARCHITECTURE_0.1.md`
- `architecture/CONCORD_AUTH_STATE_MACHINE_0.1.md`

When an architecture document conflicts with a normative specification, the normative specification wins.

### 3.4 Planning and supporting documents

- `planning/CONCORD_IMPLEMENTATION_PLAN_0.1.md`
- `decisions/CONCORD_DECISIONS.md`
- `contributor/CONCORD_REPOSITORY_GUIDE.md`

These documents explain sequence, rationale, and repository practice. They cannot introduce product requirements.

## 4. Conflict precedence

Conflicts are resolved in this order:

1. The specialized normative document that owns the topic.
2. The Product Contract for cross-cutting product intent.
3. The Specification Index for documentation governance.
4. Architecture documents.
5. Planning, decision, contributor, README, and examples.

Examples:

- A generated method name is controlled by the Generated API Specification, not an architecture pseudocode block.
- A request phase ordering is controlled by the Execution Specification, not the Product Contract summary.
- A byte-limit value is controlled by the Security and Limits Specification, not an example in another document.

## 5. Topic ownership

| Topic | Authoritative owner |
|---|---|
| Product definition and non-goals | Product Contract |
| DSL tokens, grammar, ordering, and resolution | DSL Specification |
| Generated constructors, facades, requests, setters, and terminals | Generated API Specification |
| Public traits, types, extension lifecycle, and stable namespaces | Public API Specification |
| Runtime phase order, attempt behavior, recovery, and cancellation | Execution Specification |
| Secrecy, sanitization, exact defaults, state and byte bounds | Security and Limits Specification |
| Cargo features, MSRV, dependency baseline, semver surfaces | Feature and Compatibility Specification |
| Test layers, coverage, CI, and release evidence | Conformance Plan |
| Internal ownership, module boundaries, and algorithms | Architecture documents |
| Delivery sequence and task dependencies | Implementation Plan |
| Rationale and rejected alternatives | Decisions log |
| Contributor workflow and repository rules | Repository Guide |

**DOC-OWN-001 — Single authoritative owner.** Every normative subject MUST have exactly one authoritative owner.

**DOC-OWN-002 — Cross-reference rather than redefine.** A document summarizing a requirement owned elsewhere MUST cite its requirement ID and MUST NOT alter its meaning.

## 6. Requirement identifiers

Normative requirements use stable identifiers independent of section numbers.

| Prefix | Owner |
|---|---|
| `DOC-*` | Specification Index |
| `PROD-*` | Product Contract |
| `DSL-*` | DSL Specification |
| `GEN-*` | Generated API Specification |
| `API-*` | Public API Specification |
| `EXEC-*` | Execution Specification |
| `SEC-*` | Security requirements |
| `BOUND-*` | Resource and boundedness requirements |
| `FEAT-*` | Feature behavior |
| `COMPAT-*` | Compatibility and stability |
| `CONF-*` | Conformance-process requirements |

**DOC-ID-001 — Stable IDs.** A normative requirement ID MUST NOT be reused for a different requirement.

**DOC-ID-002 — Requirement changes.** A materially changed requirement MUST either retain its ID with an explicit version change record or receive a new ID when retaining the old identity would be misleading.

**DOC-ID-003 — Coverage mapping.** Every `PROD`, `DSL`, `GEN`, `API`, `EXEC`, `SEC`, `BOUND`, `FEAT`, and `COMPAT` requirement MUST appear exactly once in the requirement coverage plan. Actual conformance evidence MUST be recorded separately only after an implementation artifact exists.

## 7. Shared terminology

### 7.1 Generated client

A concrete Rust type generated from a DSL client declaration.

### 7.2 Facade

A generated scope value containing a client clone and immutable scope-value snapshots.

### 7.3 Endpoint value

A generated typed value containing the public inputs required to prepare one endpoint request.

### 7.4 Pending request

A `Request<E>` value containing a client core, endpoint value, and request-local overrides.

### 7.5 Logical URL

The final public URL before Concord-managed query authentication is inserted.

### 7.6 Native URL

The Reqwest request URL after protected authentication materialization.

### 7.7 User operation

One direct endpoint, metadata, raw-response, stream-creation, or page execution initiated by application code.

### 7.8 Page operation

One user operation executing one pagination page.

### 7.9 Concord attempt

One Concord-controlled application request submission to Reqwest. Authentication recovery may create a second Concord attempt.

### 7.10 Reqwest-internal attempt

A native retry performed inside one Reqwest execution. It is not a Concord attempt.

### 7.11 Credential operation

One credential acquisition or refresh operation.

### 7.12 Provider HTTP

HTTP performed through Concord's isolated provider Reqwest client from a credential provider.

**DOC-TERM-001 — Consistent terminology.** Normative documents MUST use these terms consistently.

## 8. Change process

A behavior change requires all applicable steps:

1. Modify the owning normative document.
2. Update requirement IDs or requirement wording.
3. Update `conformance/REQUIREMENT_COVERAGE_PLAN_0.1.csv`.
4. Update `conformance/CONFORMANCE_EVIDENCE_0.1.csv` after implementation evidence exists.
5. Update automated conformance tests.
6. Update architecture documents when implementation structure changes.
7. Record a decision when the change affects architecture, scope, or public philosophy.
8. Update examples, README, and generated Rustdoc.

**DOC-CHG-001 — No behavior by implementation accident.** Observable behavior MUST NOT be introduced solely by implementation or tests without a corresponding normative requirement.

**DOC-CHG-002 — No silent contract drift.** Public examples and Rustdoc MUST NOT describe behavior absent from the normative specifications.

**DOC-CHG-003 — Planning is not evidence.** A coverage-plan row MUST NOT be represented as passing conformance evidence until an exact implementation artifact and result are recorded in the conformance-evidence catalog.

## 9. Document status

The versioned document set uses these statuses:

- **Normative Draft** — product decisions remain open.
- **Normative Implementation Candidate** — approved design baseline for implementation; conformance evidence is incomplete.
- **Normative Release Final** — release contract backed by complete passing or approved waived evidence.
- **Internal Draft** — implementation guidance under review.
- **Internal Implementation Candidate** — approved guidance for the implementation-candidate baseline.
- **Internal Release Approved** — implementation guidance reconciled with a conforming release.
- **Informative** — non-normative reference.

A Concord 0.1 conforming release requires every normative document to be `Normative Release Final` and every normative requirement to have passing or approved waived actual evidence.

**DOC-STATUS-001 — Candidate is not conformance.** `Normative Implementation Candidate` MUST identify a design baseline and MUST NOT be described as proof of a conforming release.

**DOC-STATUS-002 — Release-final evidence gate.** A document set MUST NOT become `Normative Release Final` while any normative requirement lacks passing or approved waived actual evidence.

## 10. No-duplication rules

**DOC-DUP-001 — Resource defaults.** Exact runtime resource and safety-limit defaults MUST be defined only in the Security and Limits Specification.

**DOC-DUP-002 — Signatures.** Exact hand-written public signatures MUST be defined only in the Public API Specification.

**DOC-DUP-003 — Generated names.** Exact generated names and signatures MUST be defined only in the Generated API Specification.

**DOC-DUP-004 — Grammar.** Exact DSL grammar MUST be defined only in the DSL Specification.

**DOC-DUP-005 — Runtime order.** Exact phase and side-effect ordering MUST be defined only in the Execution Specification.


## 11. Normative dependency order

Normative document dependencies are acyclic and follow this review order:

```text
Specification Index
    ↓
Product Contract
    ↓
DSL Specification + Public API Specification
    ↓
Generated API Specification
    ↓
Security and Limits Specification
    ↓
Execution Specification
    ↓
Feature and Compatibility Specification
    ↓
Conformance Plan
```

Cross-references do not create authority in the opposite direction. A later document may reference an earlier document but may not redefine it.

**DOC-DEP-001 — Acyclic normative ownership.** The declared normative dependency graph MUST remain acyclic.

**DOC-LINT-001 — Normative language tracking.** Every independently testable normative statement using MUST, MUST NOT, SHOULD, SHOULD NOT, or MAY MUST either carry a stable requirement ID or be part of the explicitly allowlisted normative-language definitions in this index.

Specification governance is performed through a recorded manual review of normative ownership and dependency order, followed by coverage-plan review, conformance-evidence review, and release audit. No automated specification checker is part of the archive or release authority.

## 12. Archive contents

A complete Concord 0.1 specification archive contains:

```text
README.md
REVIEW_RESOLUTION_0.1.md
normative/00_CONCORD_SPECIFICATION_INDEX_0.1.md
normative/01_CONCORD_PRODUCT_CONTRACT_0.1.md
normative/02_CONCORD_DSL_SPEC_0.1.md
normative/03_CONCORD_PUBLIC_API_SPEC_0.1.md
normative/04_CONCORD_GENERATED_API_SPEC_0.1.md
normative/05_CONCORD_SECURITY_AND_LIMITS_SPEC_0.1.md
normative/06_CONCORD_EXECUTION_SPEC_0.1.md
normative/07_CONCORD_FEATURE_AND_COMPATIBILITY_SPEC_0.1.md
normative/08_CONCORD_CONFORMANCE_PLAN_0.1.md
architecture/CONCORD_CORE_ARCHITECTURE_0.1.md
architecture/CONCORD_MACRO_ARCHITECTURE_0.1.md
architecture/CONCORD_AUTH_STATE_MACHINE_0.1.md
planning/CONCORD_IMPLEMENTATION_PLAN_0.1.md
decisions/CONCORD_DECISIONS.md
contributor/CONCORD_REPOSITORY_GUIDE.md
conformance/REQUIREMENT_COVERAGE_PLAN_0.1.csv
conformance/CONFORMANCE_EVIDENCE_0.1.csv
conformance/PHASE_0_REPORT.md
conformance/PHASE_1_REPORT.md
```
