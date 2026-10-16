# ADR 0014 — Exact, conditional and reset value controls

- **Status:** Selected (v3.0.0-beta.1)
- **Supersedes:** ADR 0012 (and, through it, ADR 0009)
- **Specification:** v3 implementation specification §4.1–§4.4, §7.1, §25.2
  (VALUE-01…VALUE-04, CALL-01)
- **Date:** 2026-09-26

## Context

ADR 0012 froze the v2.1 optional-value surface at `set_x(Option<T>)`,
`omit_x()` and `reset_x()`. In that surface one method name carried the exact
outer-slot replacement, so the common "set a present payload" call needed a
`Some(...)` wrapper and a partial update ("change it only if I have a value")
had no name at all. Callers forwarding configuration had to write the
conditional themselves, and a nested optional (`x?: Option<U>`) made the
wrapper ambiguous to read.

The v3 specification separates the operations a caller can mean — payload
assignment, exact outer-slot replacement, conditional assignment, omission and
retained reset — and gives each a different name. Rust cannot overload one
name for several of these meanings safely, so the v2.1 exact setter cannot
survive under the old name.

## Decision

Generated value controls follow the structural matrix of spec §4.3, derived
from the declaration's `?` marker and initializer, never from Rust type
spelling:

| Field after construction | `set_x` | `replace_x(Option<T>)` | `set_x_if_some(Option<T>)` | `omit_x` | `reset_x` |
|---|---|---|---|---|---|
| Required, uninitialized | no (positional argument) | no | no | no | no |
| Initialized required | payload | no | yes | no | yes |
| Uninitialized optional | payload | yes | yes | yes | no |
| Initialized optional | payload | yes | yes | yes | yes |
| Required body | no (positional argument) | no | no | no | no |
| Optional body | exact payload | yes | yes | yes | no |

Root builders additionally give required ordinary values `set_x` and
`set_x_if_some`, because no constructor has supplied them yet.

The transitions are exact (VALUE-03):

```text
set_x(v):                current := Present(convert(v))
replace_x(Some(v)):      current := Present(v)
replace_x(None):         current := Absent
set_x_if_some(Some(v)):  current := Present(v)
set_x_if_some(None):     current := current
omit_x():                current := Absent
reset_x():               current := retained_initial_state
```

Payload setters accept the closed `IntoValue<T>` conversion set (identity,
`&str -> String`, `&String -> String`); exact and conditional methods take the
exact `Option<T>` so that `None` has one target type. There is no deprecated
`set_x(Option<T>)` overload, and the retired `with_x`/`clear_x` names stay
absent.

## Consequences

- For `x?: Option<U>`, `set_x(None)` now means *present inner `None`*; the v2.1
  meaning ("absent outer slot") is spelled `replace_x(None)` or `omit_x()`.
  This is a silent-meaning hazard for unmigrated source, so the v2.1 → v3
  migration tool rewrites an old optional `set_x(opt)` to `replace_x(opt)`
  first, using owner resolution rather than a regex (spec §25.2) — but only
  for calls it proves reach the generated setter (MIGRATE-03 receiver
  identity, amended after the external review's finding F03). The proof
  resolves the receiver through the crate's modules, items, imports and
  generic parameters (`tools/xtask/src/migration/resolve.rs`) and is
  default-deny: calls inside macros other than the standard expression
  macros, calls whose names a `#[cfg]` may change, auto-deref or auto-ref
  calls without dispatch completeness, and receivers of another crate are not
  proved (a raw `r#set_x` is renamed to `r#replace_x` when proved). Every
  other candidate is left unchanged and reported with
  `C3-MIGRATE-SETTER-UNRESOLVED` (error) for a manual decision. The model and
  its limits are recorded in the
  [decision record](../decision_record.md#migrate-03-receiver-identity-external-review-f03)
  and in [the historical conversion guide](https://github.com/gqsnt/concord_api/blob/ae6006184f3dc903fdfe66c572098ac810e2c6c5/docs/migration_v3.md#receiver-identity).
- Generated method count grows by one method per configurable value
  (`set_x_if_some`) and one per optional value (`replace_x`); collision analysis
  covers every new prefix (CALL-01).
- Reset restores the retained initializer; it never reruns `Default` or rereads
  a global (VALUE-02).
- ADR 0012's "no convenience alias" rule is kept in spirit: every method has a
  distinct meaning and no two spellings share one.

## Evidence

Obligations: V3-L06, V3-L07, V3-L09, V3-L10, V3-O04. Tests present in this
tree (names confirmed by `grep`; pass/fail results for the reported
environment are recorded in the implementation report `IMPLEMENTATION_REPORT.md`,
produced separately):

- `crates/concord/tests/generated/current_language.rs` —
  `nested_optional_slot_operations_follow_the_exact_transitions` (every
  transition on a nested optional, including `Some(None)`).
- `crates/concord/tests/generated/surface.rs` —
  `value_matrix_and_clone_law_follow_the_declaration`.
- `crates/concord-macros/src/semantic/tests.rs` —
  `value_controls_follow_the_structural_matrix`.
- `crates/concord-core/src/inspection/tests.rs` —
  `value_roles_controls_and_constructor_order_follow_the_matrix`.
- `crates/concord/tests/generated/shaping.rs` —
  `closed_conversions_accept_borrowed_strings_integers_and_exact_none`,
  `trait_default_evaluation_preserves_owner_and_reset_timing`.
- UI fixtures: `crates/concord/tests/ui/fail/capabilities/absent_value_controls.rs`,
  `crates/concord/tests/ui/fail/capabilities/value_conversions_closed.rs`,
  `crates/concord/tests/ui/fail/capabilities/clear_optional_control_absent.rs`,
  `crates/concord/tests/ui/fail/capabilities/with_optional_control_absent.rs`,
  `crates/concord/tests/ui/pass/language/value_conversions.rs`.
- Migration: `tools/xtask/src/migration/tests.rs` —
  `nested_optional_setters_become_exact_replace`; `tools/xtask/tests/migrated_v3.rs` —
  `nested_option_setters_keep_the_v2_1_outer_slot_law`.
- Migration receiver identity (V3-O04, reopened by the external review for F03
  and repaired; tests in tree, recorded in the r2 validation run):
  `migration::tests::review_f03_qualified_unrelated_request_type_is_not_renamed`,
  `migration::tests::review_f03_same_field_name_in_another_struct_is_not_receiver_proof`,
  the `migration::tests::receiver_identity_*`, `migration::tests::postfix_f03_*`
  and `migration::tests::default_deny::*` unit tests, and the compiled controls
  `migrated_v3::unrelated_receivers_keep_their_original_setter_results`,
  `migrated_v3::postfix_unrelated_receivers_keep_their_original_results` and
  `migrated_v3::round3_unrelated_calls_keep_their_original_results` (unrelated
  types offering both `set_x` and `replace_x` keep their v2.1 results). The
  full list is in the [claim ledger](../claim_evidence_ledger.md) (C70).

## Revisit

Only if measured caller tasks show that a distinct method is routinely
misread. Never reintroduce a second spelling for an existing meaning, and
never reuse `set_x` for exact outer-slot replacement.
