# Concord v2.1 implementation status

Concord v2.1 is the single active language target in this repository. While release-readiness gates remain open, the workspace version is `2.1.0-rc.1`; the generated/core contract handshake is `33`, and no public v2.0 language is implemented as an intermediate mode.

The normative contract lives in the [design invariants](docs/design_invariants.md), [DSL specification](docs/spec.md), and [execution policy](docs/execution_policy.md), and selected [ADRs](dev_doc/adrs/README.md). Old spellings remain only in rejection diagnostics, migration tooling/tests, and quoted migration documentation.

This checkout remains a **v2.1 implementation prototype**, not a release candidate. Promotion requires every applicable item in the [evidence and release gates](docs/evidence_and_release_gates.md), including recorded unfamiliar-user evidence. Current supported, disconfirmed, deferred, and unknown claims are tracked in the [claim/evidence ledger](dev_doc/claim_evidence_ledger.md); the current ten-gate disposition is in [release-gate status](dev_doc/release_gate_status.md).

The maintained aggregate verification command is `just verify`. A green subset must not be reported as the complete release gate.
