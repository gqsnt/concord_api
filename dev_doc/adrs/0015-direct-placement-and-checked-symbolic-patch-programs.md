# ADR 0015 — Direct placement and structurally checked symbolic patch programs

- **Status:** Selected (v3.0.0-beta.1)
- **Supersedes:** ADR 0003
- **Specification:** §2.2, §6.1–§6.5, §25.2–§25.3 (LANG-01, PATCH-01…PATCH-03,
  PRIVACY-01, HEAD-01, MIGRATE-04)
- **Date:** 2026-09-26

## Context

ADR 0003 selected word operations (`when_present`, `append`, `remove`, family
`replace`/`off`) and explicit `each`, with plain assignment clearing and
replacing an inherited key. Three problems remained:

1. every public input needed a separate explicit block statement, even when its
   only use was one query key;
2. a plain assignment silently replaced an inherited key, so a misspelled or
   newly added ancestor key changed wire behavior without a diagnostic;
3. inherited emissions were rendered before later suppression, so an adapter
   for a value that a descendant removed could still run or fail.

## Decision

- **Direct placement.** A value may carry its placement where it is declared
  (`query x?: T`, `query("k") each tags: Vec<T>`, `header("X-K") v: T`). A
  direct placement is shorthand for a normal introduction, not a prohibition:
  the same value may also be used by explicit block statements (LANG-01).
- **One ordered program per family and owner.** Each owner normalizes a family
  in this exact order: the inherited program (or a cleared one under the
  family `replace` form), then direct placements in declaration order, then
  explicit block statements in source order. `query off`/`headers off` cannot
  coexist with a direct placement of that family; `query replace { … }` can.
- **Structural preconditions (PATCH-02).** `"k" = e` requires that no earlier
  writer potentially owns `k`; `replace "k" = e`, `when_present "k" = e` and
  `remove "k"` require one; `append` requires nothing. Introduction conflicts
  are errors for every source shape and name both writers. Potential ownership
  is independent of runtime presence.
- **Symbolic pre-encoding evaluation (PATCH-01, PATCH-03).** The program keeps
  operation, key, emission, presence source, origin and sensitivity until a
  request is planned or executed. Outer presence and surviving occurrences are
  resolved first; only surviving emissions are encoded, each once per
  preparation. A suppressed ancestor adapter never runs.
- **Sticky sensitivity (PRIVACY-01).** Sensitivity of a target key persists
  along the root-to-owner chain, including through replacement, removal and
  family fences.

```text
parent: a=1, locale=fr, z=9
when_present locale := None      => a=1, locale=fr, z=9
when_present locale := Some(de)  => a=1, z=9, locale=de
replace locale := None           => a=1, z=9
append locale := Some(de)        => a=1, locale=fr, z=9, locale=de
```

## Consequences

- v2.1 plain assignment over an inherited key becomes `replace "k" = e` in v3;
  v2.1 `when_present`/`remove` without an earlier writer now fails and needs
  explicit cleanup. The migration tool performs the rewrite with binding
  information and does not reorder mixed operations (spec §25.2).
- Suppressed public encoders no longer run, and a key's sensitivity can no
  longer be removed by a later public override (MIGRATE-04 behavior changes).
- Runtime endpoints use the same operation vocabulary and ordering (ADR 0024).
- Header names compare case-insensitively and query keys case-sensitively;
  protected and singleton headers are rejected at declaration or bind time
  (HEAD-01).

## Evidence

Obligations: V3-L03, V3-L04, V3-S01, V3-S02, V3-S03, V3-S04, V3-S09, V3-O04.
Tests present in this tree (results: see `IMPLEMENTATION_REPORT.md`):

- `crates/concord-macros/src/semantic/tests.rs` —
  `direct_placement_is_a_normal_introduction_and_values_can_have_several_uses`,
  `introduction_conflicts_name_both_writers`,
  `replace_when_present_and_remove_require_an_earlier_writer`,
  `family_off_cannot_coexist_with_direct_placements_but_replace_can`,
  `header_names_compare_case_insensitively_and_singletons_cannot_repeat`.
- `crates/concord/tests/generated/current_language.rs` —
  `one_optional_value_drives_every_intended_target`.
- `crates/concord/tests/generated/shaping/programs.rs` —
  `patch_operations_follow_the_selected_laws_in_exact_order`,
  `conditional_replacement_order_reaches_the_wire_unchanged`,
  `inline_placements_blocks_and_family_replacement_normalize_in_order`,
  `suppressed_parent_adapters_never_run_and_survivors_run_once_per_preparation`,
  `sensitivity_is_sticky_and_propagates_through_every_atom_role`.
- `crates/concord-core/src/request/program.rs` —
  `when_present_none_preserves_order_some_replaces_at_new_position`,
  `suppressed_emissions_are_never_encoded`,
  `sensitivity_is_sticky_across_replacement_and_fences`.
- `crates/concord-core/src/inspection/tests.rs` —
  `when_present_replace_remove_and_family_modes_compose_in_order`.
- UI fixture: `crates/concord/tests/ui/fail/syntax/patch_program_conflicts.rs`.
- Migration: `tools/xtask/src/migration/tests.rs` —
  `direct_placement_never_reorders_mixed_operations`;
  `tools/xtask/tests/migrated_v3.rs` — `direct_placement_keeps_the_v2_1_wire_order`.

## Revisit

If authors routinely need speculative `when_present` without a fallback,
design an explicit operation for it rather than relaxing PATCH-02. Do not
reintroduce punctuation operators or implicit replacement by assignment.
