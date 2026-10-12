# Concord v3 status summary

Concord v3 is the single active language target. This tree carries
`3.0.0-beta.1`, an **unpublished local beta** with `publish = false` in every
package manifest. The v3 language, generated client, managed runtime,
extensions, inspection tooling and the v2.1 → v3 migration tool are
implemented in this tree; known gaps against the specification are listed in
the [decision record](dev_doc/decision_record.md). R8 machine qualification passes the complete maintained implementation gate on
native Windows and Linux/WSL with pin `1.97.1` and MSRV `1.86.0`. Source-bound
commands, historical failures and limits are in [R8 validation](dev_doc/reviews/r8_validation.md).
The external Edition 1 was absent for R8 and full conformance was not independently
re-established. The ordinary `concord-v3` source-branch push is authorized;
Publication, licensing and remote-release decisions are separate owner
decisions that this implementation does not make.

This file does not define a status. The one authoritative lifecycle and gate
record is [release-gate status](dev_doc/release_gate_status.md); the open owner
questions are in the [owner decision packet](dev_doc/owner_release_decisions.md).
Command evidence (every command, exit status, test counts and the exact tested
source hash) is recorded in the implementation report
(`IMPLEMENTATION_REPORT.md`, produced separately). A passing subset is evidence
only for that subset.

## Where the contract lives

- **Selected contract:** the Concord v3 implementation specification
  (Edition 1, 25 September 2026), supplied with the implementation commission
  and not vendored in this repository. Where older repository text disagrees
  with it, the specification wins.
- **In-repository normative documents:** [design invariants](docs/design_invariants.md),
  [DSL specification](docs/spec.md) and [execution policy](docs/execution_policy.md).
- **Decisions:** the [v3 ADR index](dev_doc/adrs/README.md). ADRs 0014–0026
  record what v3 changed relative to v2.1 and name the tests behind each
  decision; superseded v2.1 ADRs are kept as history.
- **Migration:** the [v2.1 → v3 migration guide](docs/migration_v3.md) and the
  source-aware tool `cargo run -p concord-xtask -- migrate-v3`.
- **Maintainer map:** [internals](dev_doc/internals.md),
  [repository change map](dev_doc/repository_change_map.md),
  [public change matrix](dev_doc/public_change_matrix.md) and the
  [claim/evidence ledger](dev_doc/claim_evidence_ledger.md).

## Design invariants in one place

- One closed client-shaped declaration; bare names bind only the current owner
  and ancestors are qualified explicitly (ADR 0022).
- Direct placement is a normal introduction; query/header programs are ordered,
  structurally checked and evaluated symbolically, so suppressed encoders never
  run and sensitivity is sticky (ADR 0015).
- Payload assignment, exact outer-slot replacement, conditional assignment,
  omission and retained reset have different names (ADR 0014); conversions are
  closed to identity, `&str -> String` and `&String -> String`.
- Common caller controls are an unsealed trait; execution authority stays in
  core-owned bindings; a selected timeout never removes a cap (ADR 0026).
- Representations are selected by direction and trait obligation; negotiation
  is a static `ACCEPT` constant; the prepared body alone owns the exact media
  type (ADRs 0016, 0018).
- Expected absence is explicit and head-only (`absent on`), with a buffered
  `NotFound<R>` alternative (ADR 0019).
- Credential storage follows the latest-started explicit intent; the installed
  version is a separate counter; optional static secrets are not part of v3
  (ADRs 0020, 0021).
- Rate numbers, identities and key schemas are separate declarations;
  admission is atomic; shared adapters check schemas before state use
  (ADR 0023).
- Pagination has one-page await plus distinct lazy, prefix and complete
  terminals under one aggregate deadline (ADR 0017).
- Runtime endpoints use the owner projection and the same symbolic program and
  cannot create private authority (ADR 0024).
- Every path enters one managed executor whose journal owns dispatch knowledge;
  observation is read-only (ADRs 0007, 0013).

The maintained aggregate verification command is `just verify`.
