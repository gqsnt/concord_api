# concord-example-stripe-app

An external caller of [`concord-example-stripe-sdk`](../stripe-sdk) (a bounded Stripe v1 slice,
API version `2026-10-28.endive`). It depends only on the SDK's public exports, `concord` and
`concord-pagination`. The two required workflows are library functions; the loopback tests run
them end to end against `concord-example-testserver`, and one binary per workflow runs the same
function against the live API only when explicitly enabled.

| Workflow | Function | Outcome type | Test command |
|---|---|---|---|
| ST-A billing reconciliation | `reconciliation::reconcile(&StripeContext, &ReconcileInput)` | `ReconciliationReport` | `cargo test -p concord-example-stripe-app --test st_a_billing_reconciliation` |
| ST-B form and idempotency scenario | `provisioning::provision(&StripeContext, &ProvisioningPlan, &IdempotencyPolicy, &mut KeyLedger)` | `ProvisioningTrace` | `cargo test -p concord-example-stripe-app --test st_b_form_idempotency` |

All tests: `cargo test --locked -p concord-example-stripe-app` (and
`cargo test --locked -p concord-example-stripe-sdk` for the SDK's own wire, form and
pagination tests). A compiled usage example is the crate-level doctest in `src/lib.rs`.

## ST-A — billing reconciliation

Input: an explicit context (secret key, pinned version, connected account), optional customer,
status and creation-period selectors, page size, per-chain page budgets, and per-section resume
points. Output: invoices with their line items, customer and subscription references (id form,
or expanded object form with `expand[]=data.customer` and
`expand[]=data.parent.subscription_details.subscription`), relationship findings, totals kept
separate per currency, balance-transaction totals per currency (`expand[]=data.source`), the
`Request-Id`/rate-limit metadata of every response, and one section per chain (outcome, pages,
resume cursor, typed error).

Traversal: the SDK's `CursorList::pages_from` (`concord_pagination::Pages`, each budget as its
`max_pages`) — dependent forward cursors (`starting_after` = last id), one request at a time,
invoice line chains continuing each invoice's embedded `lines` list, no cursor derived from a
count, no retry. A budget reached while Stripe has more is the section's `Bounded` outcome with its
next cursor, a failed page its `Failed` outcome with the failed cursor and the typed error. See the
module documentation of `src/reconciliation.rs` for the policy.

## ST-B — form and idempotency scenario

Creates a product, a price, a customer (then corrects it), a subscription, and optionally links a
platform-side customer record, with an application-supplied key per write. A lost response is
repeated unchanged with the same key (policy-bounded); key reuse with a different request is
prevented by the application's ledger, and refused by Stripe (`idempotency_error`) when the
ledger is not shared. Neither the SDK nor `concord-pagination` generates keys or repeats writes;
the trace says exactly which writes were attempted and recognized, and never claims exactly-once
execution. See `src/provisioning.rs`.

## Acceptance evidence (contract `SDK_INTEGRATIONS.md`, ST-A and ST-B)

| Requirement | Test (file :: function) |
|---|---|
| ST-A parent and related lists, forward dependent cursors | `st_a_billing_reconciliation` :: `st_a_reconciles_parent_and_related_lists_with_expansions_and_totals`; SDK `tests/pages.rs` :: `items_stop_mid_page_and_resume_by_replaying_the_page`, SDK `tests/wire.rs` :: `cursor_list_pages_forward_and_refuses_inconsistent_pages` |
| Expandable field in id and object forms, repeated `expand[]` | `st_a_reconciles_...` (objects), `st_a_identifier_forms_findings_empty_terminal_and_account_isolation` (ids); SDK `tests/wire.rs` :: `expandable_field_in_both_forms` |
| Totals separate by currency, invoice→line relationships | `st_a_reconciles_...`; findings in `st_a_identifier_forms_...` |
| Incompatible cursor directions | `st_a_rate_limit_interrupts_and_resumed_collection_completes` (backward cursor refused, nothing sent); SDK `cursor_list_pages_forward_...` (direction, preset template cursor); SDK `tests/pages.rs` :: `backward_chain_uses_ending_before_from_the_first_id` |
| Empty terminal result | `st_a_identifier_forms_...` (account BETA balance: `data: []`, `has_more: false`) |
| Malformed non-terminal response | `st_a_malformed_line_page_and_bounded_budget` (`has_more: true`, no objects → typed `EmptyNonTerminalPage`); SDK `cursor_list_pages_...` (missing id → `Decode`, repeated cursor → `CursorDidNotAdvance`) |
| Rate-limit metadata | `st_a_rate_limit_...` (`429`, `Stripe-Rate-Limited-Reason: global-rate`, `Stripe-Should-Retry`, `Request-Id`); SDK `tests/wire.rs` :: `error_documents_rate_limits_and_lost_responses` |
| Interrupted and resumed collection | `st_a_rate_limit_...` (resume from the failed page, no duplicates); `st_a_malformed_...` (bounded budget keeps the next cursor) |
| Two contexts, no scope/header leak | `st_a_identifier_forms_...`, `st_a_rate_limit_...` (cursor of account ALPHA refused for BETA and for another filter); SDK `tests/wire.rs` :: `list_query_headers_and_account_scope` |
| ST-B nested bracketed fields, repeated fields, omitted vs empty | `st_b_form_idempotency` :: `st_b_provisions_with_forms_account_override_and_a_lost_response`; SDK `tests/form.rs` |
| Per-request account override | `st_b_provisions_...` (target account per request over the context's default; platform write without `Stripe-Account`); `st_b_missing_key_and_unrepeated_lost_response` (inherit) |
| Lost response after acceptance, repeat with the same key | `st_b_provisions_...` (`[ResponseLost, Accepted { replayed: true }]`, one subscription executed, identical bytes); `st_b_missing_key_...` (no repeat → `Unresolved`) |
| Changed parameters with a reused key | `st_b_reused_key_with_changed_parameters_is_prevented_then_rejected` (ledger prevention; Stripe `idempotency_error` without the ledger) |
| Redaction of keys, account ids, cursors, error messages | `st_b_provisions_...`, `st_b_reused_key_...`, `st_a_reconciles_...`, `st_a_rate_limit_...`; SDK `tests/wire.rs` :: `values_are_validated_and_redacted`, `write_carries_form_body_idempotency_key_and_metadata` |

Every test that starts a server ends it with `TestServer::shutdown().await`; several assert
`open_after == 0`.

## Evidence labels

| Workflow | Offline | Live |
|---|---|---|
| ST-A | `OFFLINE_VERIFIED` — loopback fake, synthetic fixtures (`tests/fixtures`) | `NOT_RUN` — no `STRIPE_SECRET_KEY` available |
| ST-B | `OFFLINE_VERIFIED` — loopback stateful fake | `NOT_RUN` — no sandbox key and no mutation opt-in |

The fakes implement documented behaviour independently of the SDK (they decode forms and
queries with the test server's own decoder); they are not Stripe. Fixture provenance is in
`tests/fixtures/MANIFEST.json`.

## Live binaries

```sh
# ST-A: read-only, bounded (2 invoice pages, 1 line page per invoice, 2 balance pages, limit 10)
STRIPE_SECRET_KEY=rk_live_... [STRIPE_ACCOUNT=acct_...] \
  [STRIPE_PERIOD_START=<unix> STRIPE_PERIOD_END=<unix>] \
  cargo run -p concord-example-stripe-app --bin st_a

# ST-B: creates objects; only with a test-mode/sandbox key AND the explicit opt-in
STRIPE_SECRET_KEY=sk_test_... STRIPE_SANDBOX_MUTATIONS=1 [STRIPE_TARGET_ACCOUNT=acct_...] \
  cargo run -p concord-example-stripe-app --bin st_b
```

Without the variables each binary prints `NOT_RUN: ...` and exits 0. ST-A prints aggregates and
request ids only, never customer data.
