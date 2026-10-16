# v3 ADR index

**Status:** Active index of architecture decisions for Concord `3.0.0-beta.1`
(unpublished local beta). Lifecycle and release gates are owned by
[`release_gate_status.md`](../release_gate_status.md), not by this index.

ADRs 0014–0026 record the consequential decisions that the v3 implementation
specification selected over v2.1. A v2.1 ADR that v3 reverses is not rewritten:
its status line names the superseding ADR and keeps the v2.1 status as history.
"Selected (v3.0.0-beta.1)" means the decision is implemented in this tree and
names its evidence tests; it does not claim that a release gate passed. Test
results for the reported environment are recorded in the implementation report
(`IMPLEMENTATION_REPORT.md`, produced separately).

## Current v3 decisions

| ADR | Decision | v3 status | Relation |
|---|---|---|---|
| [0014](0014-exact-conditional-and-reset-value-controls.md) | Exact (`replace_x`), conditional (`set_x_if_some`), omit and reset value controls | Selected (v3.0.0-beta.1) | supersedes 0012 (and 0009) |
| [0015](0015-direct-placement-and-checked-symbolic-patch-programs.md) | Direct placement, structural patch preconditions, symbolic pre-encoding evaluation | Selected (v3.0.0-beta.1) | supersedes 0003 |
| [0016](0016-directional-representation-selection-v3.md) | Directional representation selection with fences, inline `via`, static `ACCEPT`, driver-based page eligibility | Selected (v3.0.0-beta.1); per-endpoint const `ACCEPT` check emitted | supersedes 0001 (ownership carried forward) |
| [0017](0017-init-cursor-pagination-and-distinct-terminals.md) | Init/Cursor pagination; distinct `pages`/`collect_prefix`/`collect_all` terminals | Selected (v3.0.0-beta.1) | supersedes 0005 |
| [0018](0018-single-prepared-body-authority.md) | One `PreparedBody` media authority, prepared once per operation or page | Selected (v3.0.0-beta.1) | supersedes 0006 (frequency rule carried forward) |
| [0019](0019-head-only-expected-absence.md) | Head-only `absent on` expected absence plus buffered `NotFound<R>` | Selected (v3.0.0-beta.1) | extends 0004 |
| [0020](0020-optional-static-secrets-remain-rejected.md) | Optional static secrets remain rejected | Selected (v3.0.0-beta.1) | supersedes 0010's disposition |
| [0021](0021-credential-intent-epoch-installed-version-and-store-outcome.md) | Installation intent epoch versus installed version; `StoreOutcome`; installation finalizes the storage result before its final observation | Selected (v3.0.0-beta.1); amended after the external review (F01) | new |
| [0022](0022-bare-names-bind-the-current-owner.md) | Bare names bind only the current owner (LANG-03) | Selected (v3.0.0-beta.1) | new |
| [0023](0023-rate-window-sets-policy-identity-and-atomic-admission.md) | Rate window sets, explicit policy IDs, labelled key schemas, atomic admission, schema continuity | Selected (v3.0.0-beta.1) | supersedes 0008's partial-reservation clause |
| [0024](0024-runtime-endpoints-as-owner-projected-symbolic-programs.md) | Runtime endpoints as owner-projected symbolic programs without new private authority | Selected (v3.0.0-beta.1) | new |
| [0025](0025-primitive-count-runtime-configuration-setters.md) | Primitive-count runtime configuration setters (§20.4) | Selected (v3.0.0-beta.1); implemented | new |
| [0026](0026-unsealed-request-controls-with-core-owned-authority.md) | Unsealed `RequestControls`; core-owned authority; timeout preference versus caps | Selected (v3.0.0-beta.1) | new |
| [0027](0027-v3-only-product.md) | V3-only product; remove older-version conversion, aliases and state import | Selected by owner, 2026-10-16 | supersedes original migration and older-binary continuity obligations |

## v2.1 decisions retained in v3

| ADR | Decision | v3 status |
|---|---|---|
| [0002](0002-canonical-request-atoms.md) | Canonical request atoms, not implicit `Display` | Retained unchanged |
| [0004](0004-endpoint-status-and-effect.md) | Endpoint-owned status and three effect classes | Retained; extended by 0019 |
| [0007](0007-observation-without-policy-authority.md) | Read-only operation observation | Retained unchanged |
| [0008](0008-rate-feedback-and-server-delay.md) | Future-admission rate feedback and one server-delay fact | Retained except the partial-reservation clause (superseded by 0023) |
| [0011](0011-public-stream-carriers.md) | Separate `ByteStream` and `ItemStream<T>` | Retained; v3 adds finite leases and `StreamSummary` (spec §15); amended after the external review (F02): expired pending-read idle and the lease end win at completion |
| [0013](0013-observer-panic-diagnostics.md) | Contained observer panics with a private bounded diagnostic | Retained unchanged; public reporting deferred |

## Superseded v2.1 decisions (history)

| ADR | Decision | Superseded by |
|---|---|---|
| [0001](0001-representation-ownership.md) | Inherited directional representations with inline exceptions | 0016 |
| [0003](0003-query-header-patch-language.md) | Word patch operations and explicit `each` | 0015 |
| [0005](0005-pagination-controller-ownership.md) | Pagination controller owns page interpretation | 0017 |
| [0006](0006-body-preparation-scope.md) | Body preparation once per direct/page request | 0018 |
| [0009](0009-optional-control-surface.md) | Exact optional controls with provisional `with_x` | 0012 (v2.1), then 0014 |
| [0010](0010-optional-static-credentials.md) | Optional initial static secret deferred for v2.1 | 0020 |
| [0012](0012-freeze-exact-optional-control-surface.md) | Exact optional controls without convenience aliases | 0014 |

Platform statements inside v2.1 ADRs ("mechanically supported on
Windows/Linux") are v2.1 evidence. The v3 tree was validated only in the
environment recorded in [`release_gate_status.md`](../release_gate_status.md);
native Windows was not run for v3.

Supersede an ADR rather than editing its history invisibly after a
consequential reversal. Update a status only when the named evidence exists.
