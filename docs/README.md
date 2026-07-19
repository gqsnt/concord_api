# Concord 0.1 Specification

This archive defines Concord 0.1 as a new project: a generated, strongly typed Reqwest API client with managed authentication, bounded I/O, explicit page-based pagination, optional rate limiting, and narrow extension points.

The archive is specification-first. No previous implementation or compatibility behavior is assumed.

## Minimal intended usage

```rust
let client = Api::builder()
    .tenant(tenant)
    .api_key(api_key)
    .http(|http| http.retry_off())
    .runtime(|runtime| runtime.debug(DebugLevel::Basic))
    .build()?;

let user = client.users(region).by_id(user_id).await?;
let items = client.items().list().paginate(10).await?;
```

## Documentation map

Start with:

1. [`normative/00_CONCORD_SPECIFICATION_INDEX_0.1.md`](normative/00_CONCORD_SPECIFICATION_INDEX_0.1.md)
2. [`normative/01_CONCORD_PRODUCT_CONTRACT_0.1.md`](normative/01_CONCORD_PRODUCT_CONTRACT_0.1.md)
3. [`normative/02_CONCORD_DSL_SPEC_0.1.md`](normative/02_CONCORD_DSL_SPEC_0.1.md)
4. [`normative/03_CONCORD_PUBLIC_API_SPEC_0.1.md`](normative/03_CONCORD_PUBLIC_API_SPEC_0.1.md)
5. [`normative/04_CONCORD_GENERATED_API_SPEC_0.1.md`](normative/04_CONCORD_GENERATED_API_SPEC_0.1.md)
6. [`normative/05_CONCORD_SECURITY_AND_LIMITS_SPEC_0.1.md`](normative/05_CONCORD_SECURITY_AND_LIMITS_SPEC_0.1.md)
7. [`normative/06_CONCORD_EXECUTION_SPEC_0.1.md`](normative/06_CONCORD_EXECUTION_SPEC_0.1.md)
8. [`normative/07_CONCORD_FEATURE_AND_COMPATIBILITY_SPEC_0.1.md`](normative/07_CONCORD_FEATURE_AND_COMPATIBILITY_SPEC_0.1.md)
9. [`normative/08_CONCORD_CONFORMANCE_PLAN_0.1.md`](normative/08_CONCORD_CONFORMANCE_PLAN_0.1.md)

Internal design:

- [`architecture/CONCORD_CORE_ARCHITECTURE_0.1.md`](architecture/CONCORD_CORE_ARCHITECTURE_0.1.md)
- [`architecture/CONCORD_MACRO_ARCHITECTURE_0.1.md`](architecture/CONCORD_MACRO_ARCHITECTURE_0.1.md)
- [`architecture/CONCORD_AUTH_STATE_MACHINE_0.1.md`](architecture/CONCORD_AUTH_STATE_MACHINE_0.1.md)


Verification artifacts:

- [`conformance/REQUIREMENT_COVERAGE_PLAN_0.1.csv`](conformance/REQUIREMENT_COVERAGE_PLAN_0.1.csv)
- [`conformance/CONFORMANCE_EVIDENCE_0.1.csv`](conformance/CONFORMANCE_EVIDENCE_0.1.csv)
- [`conformance/PHASE_0_REPORT.md`](conformance/PHASE_0_REPORT.md)
- [`conformance/PHASE_1_REPORT.md`](conformance/PHASE_1_REPORT.md)
- [`conformance/PHASE_2_REPORT.md`](conformance/PHASE_2_REPORT.md)
- [`conformance/PHASE_3_REPORT.md`](conformance/PHASE_3_REPORT.md)
- [`conformance/PHASE_4_REPORT.md`](conformance/PHASE_4_REPORT.md)
- [`conformance/PHASE_5_REPORT.md`](conformance/PHASE_5_REPORT.md)
- [`conformance/PHASE_6_REPORT.md`](conformance/PHASE_6_REPORT.md)
- [`conformance/PHASE_7_REPORT.md`](conformance/PHASE_7_REPORT.md)
- [`conformance/PHASE_8_REPORT.md`](conformance/PHASE_8_REPORT.md)
- [`conformance/PHASE_9_REPORT.md`](conformance/PHASE_9_REPORT.md)
- [`REVIEW_RESOLUTION_0.1.md`](REVIEW_RESOLUTION_0.1.md)

Specification governance is review-based: reviewers inspect the normative documents, verify the coverage plan, verify exact executed evidence, and audit the maintained release command. No automated specification checker is authoritative or maintained.

Rust unit and integration tests use Nextest as the canonical runner. `cargo test` is used only for
Rust doctests. The maintained release graph intentionally exercises default, all-feature, and core
no-default surfaces; manifest-bearing Cargo fixtures are limited to tests of package metadata,
dependency renaming/topology, feature selection, and compile-fail boundaries that require a
separate dependency graph.

Delivery and contribution:

- [`planning/CONCORD_IMPLEMENTATION_PROGRAM_0.2.md`](planning/CONCORD_IMPLEMENTATION_PROGRAM_0.2.md)
- [`planning/CONCORD_IMPLEMENTATION_PLAN_0.1.md`](planning/CONCORD_IMPLEMENTATION_PLAN_0.1.md)
  (superseded historical plan)
- [`decisions/CONCORD_DECISIONS.md`](decisions/CONCORD_DECISIONS.md)
- [`contributor/CONCORD_REPOSITORY_GUIDE.md`](contributor/CONCORD_REPOSITORY_GUIDE.md)

## Status

The normative documents are `Normative Implementation Candidate`: they are the approved design baseline for implementation, not evidence that a conforming release exists. Architecture and planning documents are `Internal Implementation Candidate`; contributor and decision documents are informative. A release becomes `Normative Release Final` only after actual conformance evidence passes. Any behavior change must follow the Specification Index and update both the coverage plan and, when implemented, actual evidence.

Phase 9 is the current delivered implementation boundary. Exact stable advanced prepared endpoints
now reuse the one generated-client request/executor architecture, including ordinary bodies,
responses, authentication, limiting, deadlines, hooks, status handling, hidden-retry containment,
cancellation, and completion. One downstream crate certifies the complete stable extension surface
through public namespaces under default, no-default, JSON, and multipart feature ownership. See the
[`Phase 9 report`](conformance/PHASE_9_REPORT.md) for exact source identity, validation, evidence,
and limitations. Phase 10 — Product Completion and Release Candidate is next and remains
`NOT_STARTED`; prepared pagination and positive dangerous raw success remain absent.


## Review closure

This package incorporates two independent technical review rounds dated July 16, 2026. The accepted corrections include fixed client-value snapshots, source-aware multi-auth challenge handling, persistent static rejection only for explicitly challenge-enabled static placements, scope-complete explicit endpoints, immediately evaluated builder closures, mandatory finite I/O limits, HTTPS-only credential materialization, retry-disabled application defaults, final credential revalidation after limiter waits, bounded complete-only terminal status bodies, operational pagination ceilings, exact page identity, removal of custom short-page truncation, operation correlation, operation deadlines, aggregate pagination observation, renamed-dependency macro resolution, model-checked authentication concurrency, and an earlier vertical risk-spike milestone.

See [`REVIEW_RESOLUTION_0.1.md`](REVIEW_RESOLUTION_0.1.md) for the point-by-point disposition.
