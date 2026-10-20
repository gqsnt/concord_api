# ADR 0023 — Rate window sets, explicit policy identity, labelled key schemas, atomic admission and schema continuity

- **Status:** Selected (v3.0.0-beta.1)
- **Supersedes:** the custom-limiter partial-reservation consequence of ADR 0008.
  ADR 0008's other decisions — future-admission feedback that cannot change the
  current result, and one core-parsed server-delay fact — remain in force.
- **Specification:** §12.1–§12.6, §24.1–§24.2, §25.2–§25.4 (RATE-01…RATE-05,
  IDENTITY-01, MIGRATE-01, MIGRATE-02)
- **Date:** 2026-09-26

## Context

In v2.1 a policy bundled its numbers with its identity, so reusing the same
windows meant copying numbers or sharing state. Keys were built through
`&dyn RateKeyPart` arrays without a named encoding contract, and ADR 0008
allowed a custom multi-policy limiter to conservatively consume an earlier
reservation when a later policy rejected. Nothing described what a shared,
persistent backend must check before two processes (or two versions) treat
one policy ID as the same bucket.

## Decision

- **Numbers and state are separate declarations.** `windows NAME { allow N per
  D; … }` names only capacities and durations; it grants no identity and shares
  no state. A `policy` (or inline attachment) has an explicit ID, an ordered
  `key [...]`, an optional `cost` and either inline windows or `using NAME`.

<!-- concord-pseudocode -->
```rust
rate_limits {
    windows standard { allow 20000 per 10s; allow 1200000 per 10m; }
    policy tenant_reads(tenant) {
        id "catalog.tenant.read";
        key [origin, "tenant" = tenant];
        allow 120 per 1m;
        allow 5000 per 1h;
    }
}
rate_limit "concord.riot.21" key [origin] using standard;
```

- **Bucket identity (RATE-03).** A local bucket is `(policy_id, ordered
  [(label, canonical value bytes)], window capacity, window duration)`. Cost is
  charged against every window and is not part of identity. Custom labels are
  written explicitly and are state-affecting (`"client.tenant"` and `"tenant"`
  are different schemas). `origin` is the full `scheme://host:port` payload;
  `endpoint` requires an explicit endpoint ID.
- **Typed canonical encoding.** `RateKeyPart`/`RateKeyEncoder` carry a stable
  `ENCODING_ID` constant; generated code uses typed encoding functions with an
  erased internal callback instead of `&dyn RateKeyPart`. The tagged,
  length-framed baseline bytes are unchanged; the encoding ID is schema
  metadata, not inserted into the value bytes.
- **Coverage (RATE-01).** Attachments are lexical and additive; no descendant,
  runtime endpoint or request control can remove an ancestor constraint. A
  repeated attachment is deduplicated only when identity, typed bindings,
  window schema and cost are structurally identical; conflicts are errors.
- **Atomic conjunctive admission (RATE-04).** Each acquisition admits the whole
  plan or debits nothing; waiting or failure never reserves a prefix. Committed
  admission is non-refundable after later cancellation.
- **Capability and schema continuity (RATE-05).** Limiters advertise a
  capability (`Local`, `Unenforced`, `SharedPersistent`) with namespace and
  algorithm identity. A `SharedPersistent` adapter atomically registers and
  compares policy schemas inside `acquire` before state use and fails closed on
  a mismatch. A client with any effective policy requires an explicit limiter;
  `NoopRateLimiter` is disclosed as unenforced in plans.

## Consequences

R7-04/05 add an independent opaque process-local origin identity for empty-plan
cooldown feedback without exposing previously withheld host text or changing
quota schemas. Capacity pressure retains one finite limiter-object-wide barrier
through the affected expiration; it neither evicts active cooldowns nor debits
quota while waiting. The public context literal gains `cooldown_origin`; existing
custom consumers pass `None` when constructing their own contexts. See
[`docs/rate_limits.md`](../../docs/rate_limits.md) for the extension contract.

- Custom limiters must implement all-or-none admission; this is a documented
  v3 migration change for adapters written to ADR 0008's weaker allowance.
- Custom key parts must name an encoding contract; changing an encoding ID,
  label, order or window under one ID is a schema conflict, not a fresh bucket.
- A different policy ID is a reviewed state reset, not continuity. Pre-v3 peers
  are not assumed handshake-compatible; continuity across versions needs an
  explicit schema lock or operator protocol (spec §25.4). No distributed
  limiter backend is implemented; extras ship a deterministic shared-adapter
  conformance fixture only.
- The Riot conversion keeps all 46 endpoints' IDs, windows, costs, labels and
  the scope-shared groups; factoring numbers into window sets is a separate,
  state-preserving cleanup.

## Evidence

Obligations: V3-R01…V3-R08, V3-R12, V3-R13. Tests present in this tree
(results: see `IMPLEMENTATION_REPORT.md`):

- `crates/concord-macros/src/semantic/tests.rs` —
  `rate_attachments_bind_exactly_deduplicate_identical_repeats_and_reject_conflicts`,
  `rate_attachments_cannot_be_turned_off_and_the_plan_limit_names_the_excess`.
- `crates/concord/tests/generated/rate_limits.rs` —
  `generated_rate_limited_clients_fail_closed_without_an_explicit_limiter`,
  `explicit_noop_authorizes_nonempty_generated_plan_and_is_visibly_unenforced`,
  `shared_limiter_reuses_one_explicit_id_across_generated_contracts`,
  `reused_numeric_windows_under_different_ids_keep_separate_state`,
  `catalog_keys_equal_the_v2_1_baseline_and_labels_are_state_affecting`,
  `generated_formal_and_bound_keys_use_typed_encoding_ids_and_baseline_bytes`,
  `pinned_endpoint_ids_preserve_state_across_a_scope_rename`,
  `exact_repeated_named_attachment_is_deduplicated_and_inherited`,
  `shared_adapter_rejects_changed_schemas_before_admission`,
  `state_from_a_pre_v3_peer_is_not_assumed_handshake_compatible`,
  `waiting_multi_policy_admission_reserves_no_prefix`,
  `committed_admission_is_not_refunded_when_the_request_is_cancelled`.
- `crates/concord-extras/src/rate_limit/local/tests.rs` —
  `same_windows_under_different_ids_are_separate_buckets`,
  `labels_are_part_of_bucket_identity_and_cost_is_not`,
  `multi_policy_state_capacity_failure_debits_no_prefix`,
  `continuous_refill_is_capacity_per_duration_with_exact_boundaries`.
- `crates/concord-extras/src/rate_limit/conformance/tests.rs` —
  `every_schema_change_under_one_id_fails_closed_before_state_use`.
- `crates/concord-examples/tests/baseline_recording.rs` —
  `riot_v3_recording_is_state_equivalent_to_v2_1`,
  `riot_sharing_relation_is_preserved_and_enforced`,
  `catalog_client_tenant_label_is_state_affecting`,
  `edit_battery_quota_hoist_newly_covers_the_sibling`,
  `edit_battery_merging_equal_numbers_is_a_sharing_change`,
  `edit_battery_key_codec_change_changes_schema_and_key_bytes`.
- `crates/concord-examples/tests/riot_contract.rs` —
  `window_sets_factor_numbers_without_sharing_state`,
  `static_policy_coverage_keeps_the_original_sharing_groups`.
- UI fixtures: `crates/concord/tests/ui/fail/rate_limits/rate_limit_endpoint_dimension_requires_id.rs`,
  `crates/concord/tests/ui/fail/rate_limits/rate_limit_explicit_id_effective_duplicate.rs`,
  `crates/concord/tests/ui/fail/rate_limits/rate_limit_off_exclusive.rs`.

V3-R13 asks for an actual generated cross-crate compile of the key-part
contract; the tests above exercise generated keys inside the facade test crate.
Whether a separate two-crate rate fixture exists is recorded by the
implementation report, not asserted here.

## Revisit

Do not add automatic live-counter migration or claim a distributed limiter.
Any future fairness promise needs its own bounded design.
