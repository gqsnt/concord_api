# Concord v3 status summary

Concord v3 is the single active language target. This tree carries
3.0.0-beta.1, an unpublished beta with publish = false in every package.
The language, generated client, managed runtime, extensions, inspection and
v2.1-to-v3 migration tool are substantially implemented.

The [16 October source review](dev_doc/reviews/2026_10_16_review.md) examines
commit ff1be9cb588bcc664c366a55abe555590e7de01e and retains the architecture
with eight code corrections and two documentation corrections still open.
Native Linux all-feature tests and structural audits pass; exact-commit CI
retains POL-01. This does not close final supported-platform qualification,
V3-O07 performance acceptance, integration or stable publication.

R7/R8 validation remains historical and source-bound. The later
[global handoff](dev_doc/reviews/final_global_handoff.md) records Edition 1
availability and points to external final-return receipts. The current source
review did not independently inspect those external inputs.

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
