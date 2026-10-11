# ADR 0021 — Credential installation intent epoch, installed version and `StoreOutcome`

- **Status:** Selected (v3.0.0-beta.1). Amended 2026-09-26 after the external
  review of the `fc3e4d1` return (finding F01): explicit storage now finalizes
  after local installation and emits its final observation only for the fixed
  storage result (see "Finalization and final observation" below).
- **Supersedes:** none (new decision; replaces the v2.1 unconditional
  `store_as_*` installation behavior)
- **Specification:** §11.3–§11.4, §13.1–§13.3, §25.2 (AUTH-03, AUTH-04,
  AUTH-05, EXEC-02, EXEC-04)
- **Date:** 2026-09-26

## Context

v2.1 slots were versioned, but explicit endpoint-backed storage installed its
result unconditionally when the source endpoint succeeded. With overlapping
storage starts, manual replacement or logout, "whichever finished last" decided
the installed material, and a late challenge for an old version could be
confused with a user clear. One undifferentiated generation counter cannot tell
"the user started something newer" from "the server rejected what was applied".

## Decision

Every credential slot owns two separate, checked, never-wrapping counters:

- an **installation intent epoch**, advanced when explicit storage is first
  polled, by `clear_x()` and by `replace_x(material)` on
  `client.credentials()`, and by a provider-refresh leader;
- an **installed version**, allocated only when material is actually published.

Rules (strict latest-started explicit intent):

1. The first poll of `store_as_x()` reserves a new intent epoch before executing
   the source endpoint; a newer start supersedes every earlier pending intent.
   Reserving an intent does not clear currently installed material.
2. After successful complete decoding, installation happens atomically only if
   that intent is still current; otherwise the material is not installed (it
   is dropped after the result has been observed, see below) and the terminal
   returns `StoreOutcome::Superseded` (the material is never handed back
   through that terminal).
3. Failed or cancelled acquisition does not roll back the epoch and cannot
   revive an older pending intent.
4. An attributed challenge rejects only the installed version actually applied
   to that attempt; it cannot clear a newer version or invalidate a newer
   pending intent (AUTH-04).
5. Provider refresh is single-flight: followers join the leader; a cancelled
   leader retires its flight so followers do not hang; waiter counts and
   failure cooldowns are finite.
6. Store failures use the validated operational `EndpointId` from the request
   journal and never parse a display path (AUTH-05). Counter exhaustion or a
   poisoned store fails closed with a value-free `AuthError`.

`StoreRequest` resolves to `Result<StoreOutcome, ApiError>` with
`StoreOutcome::{Installed { version }, Superseded}`. `clear_x()`/`replace_x()`
are synchronous, local, return `Result` and perform no network work.

### Finalization and final observation (§11.4, EXEC-04)

The review of the `fc3e4d1` return found that `execute_store` let the shared
executor complete the journal — emitting `OperationFinished(Success)` — and
only then checked the deadline and installed, so a slow final observer could
turn a fixed success into a Timeout and a failed installation was observed as
Success. Local installation is now part of the storage operation's result:

1. **The journal stays open through installation.** `execute/store.rs` runs
   the source request through `execute_finalized_operation`
   (`execute/mod.rs`; direct requests and pages use its identity form
   `execute_prepared_operation`). Its synchronous finalization step
   (`Finalization`) runs inside the operation's journal and cancellation
   scope, after the exchange produced the decoded material and before
   `OperationJournal::complete`.
2. **Caps are decided before installation.** `Finalization::decide_caps`
   enters `FailureStage::CredentialInstall` and decides an attached
   cancellation signal, then the absolute deadline. `install_material` calls
   it immediately before and immediately after the material's
   application-defined `expires_at()` read (a synchronous extension call,
   §13.3), and then installs through the atomic, intent-checked
   `SlotSelector::install_with_expiry`, whose lock-guarded decision runs no
   application code. The caps are decided before the intent check, so a
   deadline or cancellation observed there is the result even when the intent
   was also superseded; neither installs anything or rolls the intent back.
   Nothing is rechecked after the final observation.
3. **The storage result is fixed first, then observed once.** The
   finalization's value — `Installed { version }`, `Superseded` or the error —
   is the operation's result; `journal.complete` fixes it and only then emits
   the single `OperationFinished`. `Installed` and `Superseded` are observed as
   `Success` (the returned `StoreOutcome` distinguishes them). A failure after
   a successful exchange (deadline, cancellation, poisoned store, exhausted
   version counter) is contextualized by the still-open journal at
   `CredentialInstall` and observed with its category, for the same operation
   ID and with the retained response status, attempt count and
   `ResponseObserved` knowledge; AUTH-05 endpoint identity is unchanged. A slow
   final observer can delay delivery but cannot change Installed, Superseded or
   a fixed error; a genuine delay before installation can prevent it.
4. **Spent material is released after the observed result.** Whatever
   material the finalization no longer holds leaves it as `SpentMaterial` and
   is released by `execute_store` only after the result is fixed and
   observed: decoded material the caps kept from being installed, and the
   material the lock-guarded installation decision hands out undropped as
   `ReleasedMaterial` — the value it did not install (intent superseded or
   store failed closed), the material removed from a slot that this
   installation exhausted, or the installed value it replaced (dropped once
   nothing else still uses it). The finalization only borrows the credential
   store, so when the storage request holds the store's last reference, the
   store and the material installed in it are released after the observation
   too. A panicking application `Drop` therefore propagates to whoever awaits
   the storage request without changing the result or its observation.
5. **Retired flights are told before retired material is dropped.** Every
   local slot operation that retires a provider flight (installation, clear,
   replace, intent reservation, exhaustion) publishes the flight's
   `Reevaluate` before its retired material is dropped (`Retired::hand_out`),
   and a cancelled provider leader (`LeaderGuard` drop →
   `CredentialSlot::cancel_leader`) publishes `Reevaluate` before dropping the
   challenged material it retired, so a panicking or slow `Drop` cannot leave
   followers waiting on an unpublished flight.

Internally, `ExecutedPage<T>` is now an alias of the generic `Executed<R>` and
no longer carries a journal field: the post-fixation `ErrorJournal` snapshot
that only storage used is gone, because installation failures are
contextualized by the open journal itself.

## Consequences

- Callers of `store_as_*` must handle `Superseded`; a normal `.await` on the
  source endpoint still returns the material and stores nothing.
- A successful remote login followed by a failed local installation keeps the
  remote contact and status facts on the returned error, and its one final
  observation reports that failure (never `Success`) for the same operation.
- An unpolled storage request created before a clear is a new post-clear intent
  when first polled, not permanently banned.
- An observer that sees `Success` for an installed result already sees the
  returned version installed; a slow final callback delays delivery of a
  storage result beyond its nominal deadline but never changes it
  ([`docs/auth.md`](../../docs/auth.md#explicit-credential-storage),
  [`docs/execution_policy.md`](../../docs/execution_policy.md)).

## Evidence

Obligations: V3-E12, V3-E13, V3-E14, V3-E15, and the storage case of V3-E21
(V3-E15 and V3-E21 were reopened by the external review for F01 and repaired
in this tree; see [`release_gate_status.md`](../release_gate_status.md)).
Tests present in this tree (results for the tests below that predate the
repair: see `IMPLEMENTATION_REPORT.md`; the F01 tests are in tree and recorded
in the r2 validation run):

- `crates/concord-core/src/auth/slot/intent_tests.rs` — one test per required
  ordering: `start_a_clear_a_succeeds_is_superseded_and_store_stays_empty`,
  `start_a_replace_b_a_succeeds_is_superseded_and_b_remains`,
  `start_a_start_b_a_succeeds_b_succeeds_b_installs`,
  `start_a_start_b_b_fails_a_succeeds_no_revival_and_existing_value_remains`,
  `old_version_rejected_while_newer_intent_pending_keeps_the_intent_eligible`,
  `intent_reserved_after_clear_is_a_new_eligible_intent`; plus
  `late_challenge_for_old_version_cannot_clear_a_newer_installed_version`,
  `intent_epoch_and_installed_version_are_separate_counters`,
  `reserving_an_intent_does_not_clear_installed_material`,
  `intent_epoch_exhaustion_fails_closed_and_removes_material`,
  `poisoned_store_fails_closed_with_value_free_errors`,
  `provider_leader_reserves_one_epoch_and_followers_join`,
  `cancelled_leader_retires_its_flight_and_the_follower_takes_over`,
  `follower_waiters_are_bounded_and_cancellation_releases_the_slot`.
- `crates/concord-core/src/execute/auth_tests/store.rs` —
  `unpolled_store_created_before_clear_is_a_new_post_clear_intent`,
  `start_a_start_b_then_b_fails_then_a_succeeds_keeps_existing_value`,
  `late_challenge_rejects_only_its_applied_version`,
  `failed_local_install_after_success_retains_contact_status_and_endpoint`
  and `exhausted_version_counter_fails_install_with_correlated_error` (both
  now also assert the one final observation of the failure),
  `decode_failure_installs_nothing_and_does_not_revive_older_intents`.
- F01 (`execute::auth_tests::store`, same file): the reviewer regressions
  `review_f01_final_observer_cannot_turn_store_success_into_timeout` and
  `review_f01_failed_install_is_not_observed_as_final_success`;
  `deadline_applies_through_local_installation` (oracle corrected: the delay
  is a genuine one inside the installation's own expiry read, not a slow final
  observer, and the final observation reports that Timeout);
  `delay_during_the_exchange_prevents_installation`,
  `final_observation_follows_installation_of_the_returned_version`,
  `slow_final_observer_cannot_change_installed`,
  `slow_final_observer_cannot_change_superseded`,
  `slow_final_observer_cannot_change_a_fixed_install_error`,
  `slow_final_observer_cannot_change_a_fixed_exchange_error`,
  `concurrent_replacement_during_the_exchange_is_superseded_and_observed_once`,
  `cancellation_before_installation_prevents_it`,
  `cancellation_during_the_exchange_prevents_installation`,
  `cancellation_while_the_exchange_is_in_flight_installs_nothing`,
  `deadline_before_installation_is_reported_over_a_superseded_intent`,
  `retired_material_drop_panic_is_observed_after_the_installed_result`,
  `superseded_material_drop_panic_is_observed_after_the_superseded_result`,
  `uninstalled_material_drop_panic_is_observed_after_a_timeout_before_installation`,
  `uninstalled_material_drop_panic_is_observed_after_a_failed_installation`,
  `last_store_reference_is_released_after_the_installed_result_is_observed`.
- `crates/concord-core/src/auth/slot/tests.rs` —
  `cancelled_leader_publishes_reevaluation_before_dropping_retired_material`.
- `crates/concord/tests/generated/authentication.rs` —
  `v3_e12_start_then_clear_supersedes_and_leaves_the_slot_empty`,
  `v3_e12_two_overlapping_starts_install_only_the_latest_started`,
  `v3_e14_followers_join_the_single_leader_acquisition`,
  `v3_e14_cancelled_leader_retires_its_flight_and_followers_do_not_hang`,
  `v3_e15_failed_local_install_returns_a_correlated_error_with_contact_facts`
  (now a genuine delay before installation; also asserts the final Timeout
  observation for the same operation),
  `v3_e15_slow_final_observer_cannot_change_a_fixed_storage_result`.

## Revisit

First-success-wins or latest-successful-result-wins semantics are not
selected. Any change must keep "reject installed version" separate from "user
clear/logout" and must not merge the two counters.
