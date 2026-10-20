# ADR 0017 — Init/Cursor pagination with distinct complete, prefix and lazy terminals

- **Status:** Selected (v3.0.0-beta.1)
- **Supersedes:** ADR 0005
- **Specification:** §14.1–§14.6, §17.2, §25.2 (PAGE-01…PAGE-04, REP-02)
- **Date:** 2026-09-26

## Context

ADR 0005 made the controller the owner of page interpretation and continuation.
That ownership is correct and is retained. What v3 changes is the contract
around it:

- in v2.1 the declaration initialized a controller through its fields and bound
  live endpoint values to controller state, so a controller's private layout
  was part of the DSL contract;
- one terminal, `.paginate(PageLimit)`, returned a bare `Vec` for both
  `Prefix(n)` and `CompleteWithin(n)`, so a partial result and a complete
  result had the same type;
- there was no caller-driven page-by-page traversal.

## Decision

- **Public Init, private state.** `paginate via C { init InitType { … };
  cursor -> slot; }` constructs an ordinary public Rust type equal to
  `C::Init`; the controller's fields stay private and can change without a
  DSL edit.
- **Whole-slot cursor (PAGE-01).** `cursor -> after` maps the controller's
  entire `Cursor` to the endpoint's entire effective slot (for `after?: String`
  the cursor is `Option<String>`); a tuple destination maps a tuple cursor of
  exactly matching arity. Destinations are unique endpoint-owned scalar slots —
  never bodies, ancestor values, credentials, `each` sources or rate-key inputs.
- **Controller contract.** `PaginationController<P>` with `Item`, `Init`,
  `Cursor`, `DESCRIPTION`, `start`, `cursor` and `advance` returning
  `PageStep::{Done, Continue}`. Page eligibility requires a safe effect, a
  `Buffered` response representation for the page type and, for a declared
  body, an explicit `PageRequestRepresentation` (ADR 0016, ADR 0018).
- **Three terminals, three types.** `pages(n) -> Pages<Item>` (lazy,
  caller-driven), `collect_prefix(n) -> CollectPrefix<Item>` resolving to
  `Prefix<Item>`, and `collect_all(NonZeroU32) -> CollectAll<Item>` resolving to
  `Vec<Item>` only after the controller proves exhaustion. Reaching the bound
  with continuation offered is a completion-limit error for `collect_all`,
  never a success containing an undisclosed prefix. Ordinary `.await` still
  fetches exactly one page.
- **Truthful summaries.** `TraversalSummary` reports `StopReason`
  (`NotStarted`, `Active`, `Exhausted`, `PageBound`, `Application`,
  `Cancelled`, `Failed`), `ContinuationKnowledge`, requested/effective bounds and
  checked counters.
- **One aggregate deadline and strict cancellation (PAGE-03, PAGE-04).** The
  first polled fetch starts one absolute deadline covering caller pauses.
  Dropping a started page fetch cancels the traversal; later calls return
  `Ok(None)` with `Cancelled`. Zero bounds perform no work at all.

## Consequences

- `.paginate(Prefix(n))` migrates to `.collect_prefix(n)` and
  `.paginate(CompleteWithin(n))` to `.collect_all(n)`; the output types change
  intentionally. `PageLimit` and `paginate` no longer exist. The migration
  tool makes this rewrite (with its `.await`, annotation and import edits) only
  when the call is proved to reach a paginated endpoint's generated `paginate`
  and `PageLimit` resolves to Concord's type; any other `.paginate(..)` is
  left unchanged and reported with `C3-MIGRATE-PAGINATE-UNRESOLVED` (error),
  and a runtime `PageLimit` on a generated request keeps
  `C3-MIGRATE-PAGINATE-DYNAMIC` (MIGRATE-03 receiver identity, see the
  [decision record](../decision_record.md#migrate-03-receiver-identity-external-review-f03)).
- Every built-in controller (offset, page, cursor, next-header, Link) has a
  public `Init` type; controllers written against v2.1 public fields need an
  adapter (the migration tool reports these sites rather than guessing).
- Response bytes are counted across all application attempts of a traversal,
  including rejected and retried pages (a broadened accounting relative to
  v2.1, MIGRATE-04).

## Evidence

Obligations: V3-P01…V3-P08. Tests present in this tree (results: see
`IMPLEMENTATION_REPORT.md`):

- `crates/concord-core/src/pagination/runtime_tests/terminals.rs` —
  `initial_cursor_mismatch_is_rejected_before_send`,
  `invalid_next_cursor_withholds_the_current_page`,
  `exact_bound_exhaustion_is_distinct_from_offered_continuation`,
  `complete_prefix_and_lazy_terminals_report_distinct_truths`,
  `aggregate_deadline_spans_caller_pauses`,
  `send_and_replay_budgets_persist_across_pages_and_retries`,
  `acquired_bytes_include_failed_partial_acquisitions`,
  `dropping_an_unpolled_next_page_future_changes_nothing`,
  `dropping_a_started_fetch_cancels_the_traversal`,
  `fetch_error_is_returned_once_then_none_with_failed`.
- `crates/concord-core/src/pagination/runtime_tests/limits.rs` —
  `zero_bounds_perform_no_work_and_report_page_bound_unknown`,
  `immediate_two_state_and_long_cycles_are_withheld_before_delivery`,
  `identity_budget_persists_across_pages`.
- `crates/concord-core/src/pagination/runtime_tests/envelope.rs` —
  `downstream_envelope_controller_pages_a_buffered_json_envelope`.
- `crates/concord/tests/generated/pagination.rs` —
  `generated_offset_pagination_collects_only_after_exhaustion`,
  `aggregate_deadline_spans_time_spent_between_polls`,
  `dropping_an_unpolled_fetch_changes_nothing`,
  `zero_bounds_perform_no_operation_body_provider_controller_or_limiter_work`,
  `next_header_controller_follows_one_header_and_rejects_duplicates`,
  `link_controller_owns_only_the_same_origin_cursor_slot`.
- `crates/concord-extras/src/pagination/tests.rs` —
  `offset_pagination_continues_full_pages_and_proves_done_on_short_pages`,
  `link_pagination_rejects_duplicates_and_every_forbidden_change`.
- UI fixtures: `crates/concord/tests/ui/fail/pagination/pagination_types.rs`,
  `crates/concord/tests/ui/fail/pagination/pagination_destinations.rs`,
  `crates/concord/tests/ui/fail/pagination/numeric_page_limit_absent.rs`.
- Canonical downstream controller: `crates/concord-examples/src/pagination/envelope.rs`
  (spec §17.2).

## Revisit

A resumable-cancellation mode would need its own state machine with durable
pending-page accounting; it is not silently provided. Do not reintroduce one
runtime-enum terminal that returns the same type for complete and partial
success.
