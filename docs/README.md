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
10. [`normative/09_CONCORD_REQUIREMENT_CHANGE_RECORD_0.2.md`](normative/09_CONCORD_REQUIREMENT_CHANGE_RECORD_0.2.md)
11. [`normative/10_CONCORD_REQUIREMENT_CHANGE_RECORD_0.3.md`](normative/10_CONCORD_REQUIREMENT_CHANGE_RECORD_0.3.md)
12. [`normative/11_CONCORD_REQUIREMENT_CHANGE_RECORD_0.4.md`](normative/11_CONCORD_REQUIREMENT_CHANGE_RECORD_0.4.md)

Internal design:

- [`architecture/CONCORD_CORE_ARCHITECTURE_0.1.md`](architecture/CONCORD_CORE_ARCHITECTURE_0.1.md)
- [`architecture/CONCORD_MACRO_ARCHITECTURE_0.1.md`](architecture/CONCORD_MACRO_ARCHITECTURE_0.1.md)
- [`architecture/CONCORD_AUTH_STATE_MACHINE_0.1.md`](architecture/CONCORD_AUTH_STATE_MACHINE_0.1.md)


Verification artifacts:

- [`conformance/REQUIREMENT_COVERAGE_PLAN_0.1.csv`](conformance/REQUIREMENT_COVERAGE_PLAN_0.1.csv)
- [`conformance/CONFORMANCE_EVIDENCE_0.1.csv`](conformance/CONFORMANCE_EVIDENCE_0.1.csv)
- [`conformance/RELEASE_REQUIREMENT_REGISTRY_0.1.csv`](conformance/RELEASE_REQUIREMENT_REGISTRY_0.1.csv)
- [`conformance/RELEASE_CLAIM_REGISTRY_0.1.csv`](conformance/RELEASE_CLAIM_REGISTRY_0.1.csv)
- [`conformance/RELEASE_CHANGE_MAP_0.1.csv`](conformance/RELEASE_CHANGE_MAP_0.1.csv)
- [`conformance/NORMATIVE_LANGUAGE_ALLOWLIST_0.1.csv`](conformance/NORMATIVE_LANGUAGE_ALLOWLIST_0.1.csv)
- [`conformance/NORMATIVE_DEPENDENCY_REGISTRY_0.1.csv`](conformance/NORMATIVE_DEPENDENCY_REGISTRY_0.1.csv)
- [`conformance/NORMATIVE_OWNERSHIP_REGISTRY_0.1.csv`](conformance/NORMATIVE_OWNERSHIP_REGISTRY_0.1.csv)
- [`conformance/RELEASE_VALIDATION_REGISTRY_0.1.csv`](conformance/RELEASE_VALIDATION_REGISTRY_0.1.csv)
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
- [`conformance/PHASE_10_REPORT.md`](conformance/PHASE_10_REPORT.md)
- [`conformance/PHASE_11_REPORT.md`](conformance/PHASE_11_REPORT.md)
- [`conformance/PHASE_12_REPORT.md`](conformance/PHASE_12_REPORT.md)
- [`conformance/PHASE_13_REPORT.md`](conformance/PHASE_13_REPORT.md)
- [`conformance/PHASE_14_REPORT.md`](conformance/PHASE_14_REPORT.md)
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

All 12 indexed normative documents are `Normative Release Final`, and all three architecture
documents are `Internal Release Approved`. Phase 14 is `PHASE_14_COMPLETE`; together these states
identify the locally validated **Concord 0.1 Conforming Release**. Contributor and decision
documents remain informative. Any later behavior change must follow the Specification Index and
update both the coverage plan and exact actual evidence.

The release retains generic request/response specifications, sealed buffered/streaming/discard
acquisition, universal metadata, bounded byte streaming, type-neutral generated/prepared
integration, capability-based pagination, integrated records/CSV/NDJSON, and lexically owned
generated rate limits at generated contract `14`. The exact source, validation, evidence, and
anchor chain is recorded in the [`Phase 14 report`](conformance/PHASE_14_REPORT.md); the preceding
phase reports remain immutable historical evidence. The final registries cover 581 active
requirements, 979 claim atoms, and zero waivers or unresolved claims while preserving the exact
1,726-row Phase 0-13 evidence prefix. This repository is not published: no remote publication,
tag, registry upload, or deployment was performed.


## Review closure

This package incorporates two independent technical review rounds dated July 16, 2026. The accepted corrections include fixed client-value snapshots, source-aware multi-auth challenge handling, persistent static rejection only for explicitly challenge-enabled static placements, scope-complete explicit endpoints, immediately evaluated builder closures, mandatory finite I/O limits, HTTPS-only credential materialization, retry-disabled application defaults, final credential revalidation after limiter waits, bounded complete-only terminal status bodies, operational pagination ceilings, exact page identity, removal of custom short-page truncation, operation correlation, operation deadlines, aggregate pagination observation, renamed-dependency macro resolution, model-checked authentication concurrency, and an earlier vertical risk-spike milestone.

See [`REVIEW_RESOLUTION_0.1.md`](REVIEW_RESOLUTION_0.1.md) for the point-by-point disposition.
