# Concord v2.1 status summary

Concord v2.1 is the single active language target. This checkout is an
**integration candidate / beta** at `2.1.0-beta.1`, with publication disabled.
Beta integration is mergeable after the technical, documentation, Windows,
Linux, and exact-candidate gates pass. Stable release and publication remain
blocked by owner package/license authority and unobtained human-freeze
evidence. macOS is outside the current support and release-gate boundary.

The normative contract lives in the [design invariants](docs/design_invariants.md),
[DSL specification](docs/spec.md), [execution policy](docs/execution_policy.md),
and selected [ADRs](dev_doc/adrs/README.md). The one authoritative lifecycle and
gate decision is [release-gate status](dev_doc/release_gate_status.md); this file
does not define a second status. Claim states are in the
[claim/evidence ledger](dev_doc/claim_evidence_ledger.md).

The maintained aggregate verification command is `just verify`. A passing
subset is evidence only for that subset.
