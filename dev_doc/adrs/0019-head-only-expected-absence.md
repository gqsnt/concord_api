# ADR 0019 — Head-only `absent on` expected absence and the buffered `NotFound<R>` adapter

- **Status:** Selected (v3.0.0-beta.1)
- **Extends:** ADR 0004 (endpoint-owned status and effect). ADR 0004 remains in
  force; this record adds a third endpoint-owned status set.
- **Specification:** §7.4, §10.1–§10.4, §25.2 (OUTCOME-01…OUTCOME-03, CALL-03)
- **Date:** 2026-09-26

## Context

ADR 0004 made accepted status finite endpoint data and kept the authentication
challenge ahead of status selection. A common provider contract — "404 means
the resource does not exist" — still had no first-class expression. In v2.1 it
needed a custom buffered representation that acquired and bounded the whole
404 body, so a huge, stalled or malformed irrelevant body could turn an
expected "missing" into a body-limit or decode failure.

## Decision

- `absent on <status>[, …];` declares a separate endpoint-owned absence set,
  disjoint from the accepted payload set and from any effective challenge.
- The result type must satisfy the library-sealed `OptionalOutcome` relation,
  implemented only for `Option<T>` (aliases work; no token matching). The present
  representation converts `T` on payload success and the generated bridge wraps
  `Some(T)`; on declared absence core constructs `None` without invoking the
  present representation.
- Absence is resolved after challenge processing and rate feedback and before
  accepted-head validation or body polling. The native body is dropped unread
  and recorded as `BodyDisposition::DiscardedUnread`, never as consumed.
- Capability restrictions are declaration errors: absence requires a
  `Buffered` present driver and cannot combine with pagination, streaming or
  discard drivers, `store_as_*` credential output, or `expose accepted_bytes`.
  `.response()` still works and reports the real missing status.
- `NotFound<R>` in extras is the alternative when raw bytes or a buffered 404 are
  needed: an ordinary buffered representation for `Option<T>` that acquires the
  complete 404 body under limits, bypasses the wrapped converter's media check
  for 404 and forwards `R::ACCEPT`. Its documentation states that, unlike
  `absent on`, it can fail on a truncated or oversized 404 body.
- `-> Option<T>` without `absent on` has no status shortcut; the selected
  representation decodes `Option<T>` itself.

## Consequences

- Authors choose deliberately between head-only absence (cheap, cannot see the
  body) and the buffered adapter (sees the body, obeys body limits). Migration
  keeps status-coded `Option` custom code as the buffered adapter unless the
  author selects `absent on` (spec §25.2).
- No runtime `absent on` builder exists in v3; runtime endpoints use the
  buffered adapter pattern (spec §16.2).
- Rejected final statuses keep the complete-only status-body states
  (`Disabled`, `Empty`, `Complete`, `OmittedKnownTooLarge`,
  `OmittedLimitExceeded`, `UnavailableReadFailure`, `UnavailableDeadline`);
  capture failure never replaces the `HttpStatus` classification.

## Evidence

Obligations: V3-E08, V3-E09, V3-E10. Tests present in this tree (results: see
`IMPLEMENTATION_REPORT.md`):

- `crates/concord-macros/src/semantic/tests.rs` —
  `absence_is_disjoint_and_exclusive_with_pagination_and_accepted_bytes`.
- `crates/concord-core/src/execute/tests/body_outcomes.rs` —
  `v3_e08_absence_is_head_only_and_never_polls_the_irrelevant_body`,
  `v3_e08_conflicting_absence_capabilities_are_rejected_before_sending`,
  `v3_e10_status_body_states_are_complete_only_and_never_replace_the_status`.
- `crates/concord-core/src/response/absence.rs` —
  `sealed_outcome_accepts_option_aliases_and_constructs_privately`,
  `head_absence_forwards_accept_and_wraps_present_values`.
- `crates/concord/tests/generated/shaping/bodies.rs` —
  `head_only_absence_never_polls_the_missing_body`,
  `terminal_status_bodies_are_complete_only_and_explicitly_decoded`.
- `crates/concord-extras/tests/contracts/outcomes.rs` —
  `v3_e09_not_found_acquires_the_404_body_and_maps_it_to_none`,
  `v3_e09_not_found_404_body_still_obeys_limits_and_completeness`,
  `v3_e09_accepted_bytes_bypass_the_decoder_but_keep_status_negotiation_and_limits`,
  `v3_e10_status_body_json_decoding_is_explicit_and_does_not_mutate_the_error`.

## Revisit

Do not add a general status-match or callback DSL (spec §26.1). More complex
expected outcomes use an application enum with a custom representation.
