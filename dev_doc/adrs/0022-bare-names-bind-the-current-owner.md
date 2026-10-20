# ADR 0022 — Bare names bind only the current owner

- **Status:** Selected (v3.0.0-beta.1)
- **Supersedes:** none (new decision; replaces the v2.1 owner-reference rules,
  including redundant current-owner qualification such as `endpoint.x`)
- **Specification:** §3.4, §25.2 (LANG-03, DIAG-01 `C3-NAME`, `C3-MIGRATE`)
- **Date:** 2026-09-26

## Context

A declaration tree lets several owners declare values with the same name. If a
bare reference may fall back to the nearest ancestor with that name, inserting
a same-named value on an intermediate scope silently rebinds existing
references — a parent edit changes descendant wire behavior without a
diagnostic. The v2.1 language also accepted redundant qualification of the
current owner (`endpoint.x`), giving two spellings for one reference.

## Decision

- In a client clause, a bare name binds only a client value; in a scope
  preamble, only that scope's value; in an endpoint clause or path template,
  only that endpoint's value. A missing bare name is an error even when an
  ancestor has a matching name. The `C3-NAME` diagnostic may suggest
  `client.id` or `regional.id` but never chooses automatically.
- A nonlocal reference names an exact visible ancestor: `client.tenant`,
  `regional.language`. Visible scope identifiers are unique along one ancestor
  chain (siblings may repeat). `client`, `endpoint` and the `__concord` prefix are
  reserved owner names.
- Redundant qualification of the current owner, including `endpoint.x`, is not
  canonical v3 and is rejected with a migration diagnostic.
- Inside a named rate policy, bare names refer to the policy's formals;
  attachments bind formals explicitly. Credential names are a separate
  namespace. Raw identifiers use their logical name (`r#type` is `type`).

## Consequences

- Adding a same-named value to an ancestor can no longer capture a descendant's
  reference; the local reference stays local.
- The migration tool removes redundant qualification and keeps exact ancestor
  qualification only after resolving the v2.1 owner graph, never by regex.
- Authors write an explicit owner prefix for every ancestor value, which is
  slightly longer but makes each dependency visible where it is used.

## Evidence

Obligations: V3-L02, V3-O04. Tests present in this tree (results: see
`IMPLEMENTATION_REPORT.md`):

- `crates/concord-macros/src/semantic/tests.rs` —
  `bare_names_bind_only_the_current_owner`,
  `redundant_qualification_of_the_current_owner_is_a_migration_error`,
  `scope_names_are_unique_along_one_ancestor_chain`.
- `crates/concord-core/src/inspection/tests.rs` —
  `references_follow_local_and_explicit_owner_rules`.
- `crates/concord-examples/tests/baseline_recording.rs` —
  `edit_battery_same_named_ancestor_does_not_capture_local_names`.
- UI fixtures: `crates/concord/tests/ui/fail/names/bare_ancestor_reference.rs`,
  `crates/concord/tests/ui/fail/names/redundant_current_owner_qualification.rs`,
  `crates/concord/tests/ui/fail/names/duplicate_scope_identifier.rs`.
- Migration: `tools/xtask/src/migration/tests.rs` —
  `constant_and_owner_qualification_are_resolved_exactly`;
  `tools/xtask/tests/migrated_v3.rs` —
  `constant_paths_and_owner_references_resolve_exactly`.

## Revisit

Do not add nearest-name lookup or implicit ancestor fallback. If qualification
noise becomes a measured problem, consider explicit local aliases that are
themselves declarations.
