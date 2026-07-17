# Concord 0.1 Independent Review Resolution

**Version:** 0.1  
**Status:** Informative  
**Reviewed input:** Concord Specification 0.1 drafts  
**Review date:** July 16, 2026  
**Authority:** Change record only; normative behavior remains in `normative/`

## 1. Purpose

This document records the disposition of the two independent deep technical reviews performed before implementation. It explains which findings changed the implementation-candidate specification, which remedies were narrowed, and which proposed capabilities remain outside Concord 0.1.

The review record is informative. Requirement wording and behavior are authoritative only in the normative specifications.

## 2. First review disposition

| Finding | Disposition | Implemented resolution |
|---|---|---|
| CR-001 | Accepted with simpler remedy | Client-level values are fixed after independent construction. Same-snapshot clones share credentials; separately built clients do not. No credential-partition DSL exists in 0.1. |
| CR-002 | Accepted and later narrowed | Built-in static credentials enter persistent rejection only when the static placement was explicitly challenge-enabled. Source-aware defaults do not reject static credentials implicitly. |
| CR-003 | Accepted | Explicit endpoint constructors include required enclosing scopes outermost-to-innermost and expose equivalent optional/default scope controls. |
| CR-004 | Accepted | `.http(F)` and `.runtime(F)` invoke closures immediately and store resolved configuration; duplicate closures are not invoked. |
| CR-005 | Accepted | All ordinary application and provider body limits remain finite. No unbounded-I/O API exists in 0.1. |
| CR-006 | Accepted in part | Pagination has finite ceilings for requested pages, collected items, and retained identity bytes. Incremental pagination remains outside 0.1. |
| CR-007 | Accepted | Complete canonical page identity is owned by the normative Execution Specification. |
| CR-008 | Accepted and later simplified | Public custom-controller short-page capacity was removed. Exact short-page optimization is private to sealed built-in controllers. |
| CR-009 | Accepted | Endpoint DSL/request `timeout` is a Concord operation deadline. Connect/read timeouts remain HTTP-client settings. Each page has its own deadline. |
| CR-010 | Accepted in part | Stream construction reports `StreamOpened`; terminal stream-consumption hooks remain outside 0.1. |
| CR-011 | Accepted | Pagination emits one aggregate completion event in addition to per-page events. |
| CR-012 | Clarified | The built-in governor is bounded best-effort process-local pacing. Strict/distributed enforcement remains a custom-limiter responsibility. |
| CR-013 | Reconsidered after second review | Automatic typed status schemas remain outside 0.1, but a terminal non-success response may expose one finite complete body through an opaque explicit accessor. |
| CR-014 | Accepted | Credential `is_set()` is synchronous and point-in-time. |
| CR-015 | Accepted in part | A pre-1.0 compatibility policy was added. Dangerous custom retry remains intentionally coupled to the selected Reqwest type. |
| CR-016 | Accepted | Initial MSRV changed from Rust 1.97 to evidence-based Rust 1.85. |
| CR-017 | Accepted | Macro-generated core paths use package-aware dependency resolution and support renamed dependencies and nested expansions. |
| CR-018 | Accepted | Custom pagination uses one atomic factory and a safe request patch; no `Default` or public controller fields are required. |
| CR-019 | Accepted | Authentication conformance requires Loom, Shuttle, or equivalent model checking plus deterministic integration tests. |
| CR-020 | Accepted | Milestone 0 contains vertical risk spikes before broad runtime implementation. |
| CR-021 | Accepted | Substantive normative language is requirement-ID tracked and normative dependencies are acyclic; manual normative-document, coverage-plan, evidence-catalog, and release-command review enforce both. |
| CR-022 | Accepted | Observable algorithms such as page identity, challenge attribution, and credential finalization were promoted into normative documents. |
| CR-023 | Accepted | Defaults use a restricted side-effect-resistant grammar; overridden client defaults are not evaluated. |
| CR-024 | Accepted with bounded scope | Body processing separates early metadata validation from expensive materialization while retaining one-phase limiter acquisition. |

## 3. Second review disposition

### 3.1 Multi-auth challenge attribution

Accepted as a correctness defect.

- Static API key, static bearer, Basic, and endpoint-backed placements default to `challenge never`.
- Built-in refreshable OAuth bearer defaults to `challenge unauthorized`.
- Custom providers require an explicit challenge policy.
- An effective authentication plan may contain at most one challenge-enabled placement.
- Persistent static rejection occurs only after explicit static challenge enablement.

This avoids attributing one ambiguous `401` to every attached credential.

### 3.2 Credential changes during limiter waits

Accepted as a correctness and revocation defect.

Credential handling now has:

1. preflight before limiter acquisition; and
2. final validation after limiter acquisition and immediately before authentication materialization.

Final validation checks current slot identity, version, rejection, clear/replacement, and expiration. A clear or replacement completed before final validation prevents the superseded material from being sent. A race after final validation may proceed because the native-send boundary has already been linearized.

### 3.3 Aggregate buffered-body memory

Clarified rather than redesigned.

Concord limits each operation but does not provide a process-wide memory scheduler. The one-phase limiter API is retained. Applications are expected to use bounded concurrency for large buffered requests. A two-phase limiter is not part of 0.1.

### 3.4 Native retry and limiter accounting

Accepted.

Application Reqwest retry is disabled by default. Explicit `retry reqwest_default;`, the equivalent safe builder method, or dangerous custom retry is required to enable native retry. When enabled, rate-limit authority governs Concord attempts rather than hidden physical attempts.

### 3.5 Terminal non-success bodies

Accepted with a narrow secure design.

A final application HTTP failure may retain a small finite complete body through `HttpResponseFailure` and `StatusBody`. The body:

- is complete or unavailable; no truncated prefix is exposed;
- has a separate finite configurable limit;
- is accessed only explicitly;
- never appears in automatic `Debug`, `Display`, hooks, completion events, limiter contexts, or source chains;
- may be explicitly decoded through an existing `DecodeBody` implementation;
- does not replace the known status when body capture fails.

Recoverable challenge bodies remain unread. A terminal challenge may use the bounded status-body contract.

Automatic generated typed status schemas remain outside 0.1.

### 3.6 Cleartext credential transmission

Accepted as a security defect.

Concord does not materialize credentials onto an HTTP application request. Protected application requests and all provider requests require HTTPS. No implicit localhost exception exists.

### 3.7 Event correlation

Accepted.

Request/page events carry an opaque `OperationId`. Pagination page events additionally carry the parent aggregate operation ID. Completion metadata contains monotonic elapsed duration and the final application status when one existed.

### 3.8 Custom-controller short-page inference

Accepted as a safe-extension defect.

The public custom pagination controller no longer exposes an exact-capacity or short-page optimization. Custom continuation is decided by `next(&Page)` plus universal runtime rules. Sealed built-in offset and page controllers may use private exact-capacity optimization.

### 3.9 Specification status

Accepted.

The current package is `Normative Implementation Candidate`, not release-final evidence. `Normative Release Final` is assigned only after a conforming implementation and complete passing or explicitly waived evidence exist.

### 3.10 Coverage planning versus evidence

Accepted.

- `REQUIREMENT_COVERAGE_PLAN_0.1.csv` maps every requirement to intended verification work.
- `CONFORMANCE_EVIDENCE_0.1.csv` records only exact implemented passing or waived evidence.
- The implementation-candidate package may have complete coverage planning and an empty evidence catalog.
- Release-final status requires actual evidence for every normative requirement.

## 4. Deliberately retained boundaries

Concord 0.1 still does not add:

- automatically generated endpoint-specific typed status-error enums;
- unbounded, streaming, or truncated terminal status bodies;
- a strict built-in rate-limit capacity mode;
- incremental page/item streams;
- terminal stream-consumption hooks;
- a Concord-owned general retry engine;
- credential partition-key DSL;
- process-wide request-memory scheduling;
- cleartext authenticated requests, including implicit localhost exceptions.

These capabilities require a later contract revision if real usage demonstrates the need.

## 5. Validation model

Specification governance uses an explicit review procedure rather than an automated specification checker:

1. review the normative documents in the dependency order declared by the Specification Index, including requirement-ID ownership and normative-language placement;
2. review `REQUIREMENT_COVERAGE_PLAN_0.1.csv` for one intentional planned mapping per requirement;
3. review `CONFORMANCE_EVIDENCE_0.1.csv` for exact executed or approved waived evidence without converting plans into results;
4. audit and run the maintained release command, then record its exact outcome in the applicable phase report.

An implementation-candidate review establishes document consistency and planned coverage only. It does not claim that implementation evidence exists. Release-final status requires complete actual conformance evidence.
