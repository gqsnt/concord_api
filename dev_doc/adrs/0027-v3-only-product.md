# ADR 0027 — V3 is the only supported product version

- Status: Selected by the project owner, 2026-10-16.
- Scope: current V3 source, public API, tools, examples, fixtures and qualification.
- Supersedes: the migration and older-binary continuity requirements in the
  original V3 commission; does not alter unrelated V3 semantics.

## Decision

Ship one canonical V3 grammar, one managed runtime and V3-native public
interfaces. Remove the `migrate-v3` implementation and its V2 parser/resolver,
exclusive migration fixtures, the `RequestBody` and `ApiError::stage()` aliases, and the
conformance store's older-binary markers and schema-import path. Current
performance tooling generates only V3 declarations.

Retain explicit V3 codec adapters and user-declared API deprecation, which are
native extension and authoring capabilities. Rejection tests for unsupported
syntax remain useful language-boundary tests. Historical evidence remains in
Git and dated review records, with its source scope intact.

## Acceptance consequences

The original V3-O04 migration obligation is superseded by this decision; it
must be reported as retired by owner direction, not as a passing converter.
The older-binary continuity portions of V3-R01/R02/R06/R13 are similarly replaced
by current V3 schema and golden-record assertions. Ordinary rate identity,
framing, sharing, atomic admission and fail-closed schema checks still apply.

V3R-01 is closed by removal of the unsupported conversion component. Its
historical reproduction remains valid for the old reviewed commit. V3R-02
through V3R-08 retain their code-correction obligations.

## Verification

Build and test the current feature matrix, compile the public API and UI
boundaries, execute the V3 tools, verify the generated documentation and run
the current recording/schema fixtures. Check that the removed migration
command fails as unsupported, the old body alias cannot be imported, and the
error accessor uses the canonical `failure_stage()` name.
The source-bound correction report records actual results and their limits.
