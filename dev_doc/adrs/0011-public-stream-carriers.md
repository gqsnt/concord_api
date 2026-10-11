# ADR 0011 — Separate `ByteStream` and `ItemStream<T>`

- **Status:** Retained in v3.0.0-beta.1. The carrier decision is unchanged; v3 adds the finite post-acquisition lease, pending-read idle accounting, cancel-safe `next` and `StreamSummary` of v3 specification §15 (STREAM-01…STREAM-03). Amended 2026-09-26 after the external review of the `fc3e4d1` return (finding F02, obligation V3-P10): an expired pending-read idle deadline, and the end of the total lease, now win over data, empty chunks or an end of body that become observable only afterwards (see "v3 lease and idle contract" below).
- **v2.1 status (historical):** Selected and supported on Windows/Linux by comparative lifecycle, allocation, throughput, compile, and size evidence. The comparison below is v2.1 evidence and was not re-run for v3.

## Decision

Expose representation-independent `ByteStream` for non-semantic byte chunks and `ItemStream<T>` for semantic decoded items. Keep decoder/acquisition driver types private.

## Property earned

Caller-visible lifecycle, errors, limits, and item boundary stability are explicit without wrapper towers or driver compatibility commitments.

## Contract

Initial request completes on stream acquisition. Later polling has independent failures/limits/events. No transparent retry after yielding. Drop cancels local consumption only. No `Clone` without multicast/replay semantics.

## v3 lease and idle contract

The lease belongs to the stream handle (`response/lease.rs`, `StreamLease`),
not to the temporary `next` future, and one ordered predicate judges it:
`StreamLease::check(now)` checks the total lease first (`now` at or after its
end is `LeaseExpired`, even when the idle budget ran out too and whichever ran
out first), then, only while an underlying read is pending, that read's
cumulative idle (idle equal to or above the limit is `IdleTimeout`). Without an
outstanding pending read no idle is charged, so consumer time after a
delivered chunk never counts.

`StreamCore` (`response/stream.rs`, shared by `ByteStream` and the framing
layer of `ItemStream<T>` in `records/stream.rs`) evaluates that predicate at
two points and has no detached timer task:

1. **Precheck**, at the start of every poll (after the cancellation check),
   before the source is polled again and before an already staged record is
   delivered. Once a limit has expired nothing is read, so data that had merely
   become available is neither delivered nor counted.
2. **Read completion.** The clock is read once when a completion of an
   underlying read is observed, and the predicate is evaluated before any
   branch accepts it — nonempty data, an empty chunk, the end of the body or a
   read failure alike — and before the pending baseline is cleared. A
   completion observed at or after the lease end is withheld whether or not
   the read was ever pending; the completion of a pending read whose idle
   budget is exhausted is withheld as `IdleTimeout`. A withheld nonempty chunk
   was read from the transport and counts in `acquired_bytes`. Cancellation is
   not judged here; a signal raised during the read is observed at the next
   poll.

Consequently an outstanding pending acquisition whose idle budget ran out —
for example after the caller dropped its `next` future and the producer became
ready only later — yields exactly one `IdleTimeout` error (or `LeaseExpired`,
which takes precedence) before any later data, empty chunk, end of body or read
error is accepted, in byte and record streams alike: late data is not
delivered, a late end of body neither reports a clean end nor completes a
staged final record, the summary records `Failed`, and the next call returns
`None`. Nothing is resent. Resuming before the limit continues normally.

Empty chunks complete a read but acquire no data: `on_empty(now)` adds the
read's span to the cumulative idle without resetting it. At the cooperative
yield after 32 empty chunks in one poll turn, `on_yield(now)` sets a new
pending baseline at that instant, so the acquisition stays pending and the
time until the next poll counts as idle, also when `next` is dropped at the
yield; the yield wakes the task at once and arms no timer. Only accepted
nonempty data resets idle.

User-facing rules: [`docs/streams.md`](../../docs/streams.md#the-finite-lease)
and [cancel-safe reads](../../docs/streams.md#cancel-safe-reads).

## Release evidence

The selected implementation uses core-sealed generic acquisition drivers and
private boxed stream erasure for `ByteStream` and `ItemStream<T>`. The maintained
comparison covers monomorphized, private-enum, and boxed alternatives. Boxed
erasure uses one allocation at construction and none per poll, stays inside the
precommitted byte and item throughput budgets, has identical probe binary size,
and preserves the same lifecycle signature. Runtime tests separately cover
post-return failure, bounds, backpressure, cancellation, and terminal evidence.
See `dev_doc/evidence/stream_driver_comparison.md`.

The public carriers are frozen for v2.1. A future implementation may change
only the private erasure strategy if a maintained measurement crosses a budget.

## v3 lease evidence (V3-P10)

Tests in tree; the F02 tests are recorded in the r2 validation run (results
for the earlier V3-P10 tests: see `IMPLEMENTATION_REPORT.md`):

- `crates/concord-core/src/response/stream/tests.rs` (`response::stream::tests`)
  — reviewer regressions `review_f02_ready_data_cannot_erase_expired_pending_idle`,
  `review_f02_ready_eof_cannot_erase_expired_pending_idle`; post-fix reviewer
  probes `review_postfix_f02_completion_after_total_and_idle_precedence`,
  `review_postfix_f02_completion_after_total_lease_only`,
  `review_postfix_f02_abandon_at_empty_chunk_yield`;
  `v3_p10_overdue_empty_chunks_then_ready_data_are_one_idle_timeout`,
  `v3_p10_empty_chunk_before_the_deadline_does_not_reset_idle_for_overdue_data`,
  `v3_p10_ready_data_at_exactly_the_idle_limit_is_an_idle_timeout`,
  `v3_p10_a_read_completing_past_the_idle_limit_within_one_poll_is_an_idle_timeout`,
  `executor_handoff::v3_p10_overdue_body_after_an_abandoned_read_is_one_idle_timeout_without_resend`,
  `v3_p10_postfix_f02_completion_after_both_limits_is_one_lease_expiry`,
  `v3_p10_postfix_f02_precheck_gives_the_same_verdict_and_reads_nothing`,
  `v3_p10_postfix_f02_completion_after_the_lease_end_is_withheld_not_delivered`,
  `v3_p10_postfix_f02_completion_at_the_lease_end_is_expired_and_just_before_is_delivered`,
  `v3_p10_postfix_f02_any_completion_observed_after_the_lease_end_is_lease_expired`,
  `v3_p10_postfix_f02_time_away_at_an_empty_chunk_yield_counts_as_pending_idle`;
  positive controls
  `v3_p10_ready_data_below_the_idle_limit_resumes_and_later_consumer_pauses_are_free`,
  `v3_p10_empty_chunks_below_the_idle_limit_keep_charging_until_data_arrives_in_time`,
  `v3_p10_postfix_f02_resuming_after_an_empty_chunk_yield_within_the_budget_delivers`;
  `per_poll_empty_chunk_budget_returns_pending_and_wakes_once` (now on the
  paused clock, assertions unchanged).
- `crates/concord-core/src/records/stream/tests.rs` (`records::stream::tests`)
  — `v3_p10_overdue_ready_data_cannot_complete_a_staged_record`,
  `v3_p10_overdue_eof_cannot_finalize_a_staged_tail_record`,
  `v3_p10_overdue_empty_chunks_then_record_data_are_one_idle_timeout`,
  `executor_handoff::v3_p10_overdue_record_body_after_an_abandoned_read_is_one_idle_timeout_without_resend`,
  `v3_p10_postfix_f02_a_read_completing_after_the_lease_end_cannot_complete_a_staged_record`;
  positive control `v3_p10_record_data_below_the_idle_limit_completes_the_staged_record`.
- `crates/concord-extras/tests/contracts/record_streams.rs` (first-party
  codecs over loopback HTTP) — `ndjson::v3_p10_overdue_ndjson_record_data_is_one_idle_timeout_without_resend`,
  `ndjson::v3_p10_overdue_eof_cannot_finalize_an_unterminated_ndjson_line`,
  `csv::v3_p10_overdue_csv_record_data_is_one_idle_timeout_without_resend`,
  `csv::v3_p10_overdue_eof_cannot_finalize_an_unterminated_csv_record`;
  positive control `ndjson::v3_p10_ndjson_record_data_within_the_idle_limit_resumes_an_abandoned_read`.
- Earlier V3-P10 tests kept unchanged: `v3_p10_idle_charges_only_pending_reads_not_consumer_pauses`,
  `v3_p10_dropping_next_cannot_reset_idle_or_counters`,
  `v3_p10_abandoned_next_futures_cannot_extend_the_total_lease`,
  `v3_p10_empty_chunks_do_not_reset_pending_read_idle`,
  `v3_p10_total_lease_charges_consumer_pauses_and_releases_on_next_poll`,
  `v3_p10_total_lease_expires_while_a_read_is_pending`,
  `v3_p10_whole_operation_cap_started_at_the_original_request_bounds_the_lease`
  (same file) and `v3_p10_dropping_a_pending_record_future_cannot_reset_idle_or_counters`
  (records).
