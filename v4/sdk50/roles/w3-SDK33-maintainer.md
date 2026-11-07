# Independent maintenance capsule — SDK33 Square Payments

You maintain the SDK in `examples/square-payments-sdk` (package `concord-sdk-square-payments`), which you did not write, after its first
consumer. First attempt the ticket below **before** reading `FRICTION.md` or `FIRST_USE.md`.

## Provider slice: SDK33 · Square Payments (medium; payments and accounting)

Connected workflows (both required):
1. Create a sandbox payment with a caller-supplied idempotency key, then retrieve the payment.
2. Page account payments by cursor and inspect one returned payment.

Contract notes: Bearer token and versioned API; CreatePayment idempotency_key in body; opaque cursor.
Failure that must be exercised: Reuse the exact caller key and body after a timeout; treat a mismatched replay as an error.
Excluded: Card collection, refunds, catalog and universal retries for all writes.
Official documentation (read-only research is allowed; cite the exact pages and the date you read
them):
- https://developer.squareup.com/reference/square/payments-api
- https://developer.squareup.com/docs/build-basics/common-api-patterns/idempotency
Live provider calls: **NOT_RUN** — never call the real service; every workflow runs against local
fixtures (`examples/testserver`: `TestServer::start(|request: &Recorded| -> Reply)`).

## Fixed rules

- Concord's product is its core, macros, declarations and generated usage; the hierarchy
  client → scope → endpoint is fixed. The frozen baseline is commit `14b9e1e0a89775dbe5cbb9f7837bde677ea5878e`: **do not edit
  any shared crate** (`crates/*`, `checks/*`, other examples, `scripts/*`) — record what you would
  change instead. Your ownership is `examples/square-payments-sdk/` plus adding `"examples/square-payments-sdk"` to the root `Cargo.toml`
  workspace `members` (and the `Cargo.lock` entry cargo writes).
- Build into the worktree's default `target/` (never set a relative `CARGO_TARGET_DIR`);
  `CARGO_BUILD_JOBS=4`, one cargo command at a time.
- Adding your package changes `Cargo.lock`: build once without `--locked` (offline if possible),
  then use `--locked`. Prefer dependencies already in `Cargo.lock`; justify any new crate (name,
  version, license, why) in `PROVENANCE.md`. No other package may change version in the lock.
- You cannot commit (`.git` is read-only); leave your work in the working tree.
- No private reasoning transcripts are wanted: record observable actions, diagnostics, files read
  and decisions.

## Ticket


Callers need delayed capture. Square's Payments API lets a payment be created with
`autocomplete: false` (an authorization only) and later completed with
`POST /v2/payments/{payment_id}/complete` or voided with `POST /v2/payments/{payment_id}/cancel`;
complete accepts an optional `version_token` so a stale caller cannot complete a payment that
changed. Read the CompletePayment and CancelPayment references and the delayed-capture guide and
cite them with the date:
- https://developer.squareup.com/reference/square/payments-api/complete-payment
- https://developer.squareup.com/reference/square/payments-api/cancel-payment
- https://developer.squareup.com/docs/payments-api/take-payments/card-payments/delayed-capture

Add: creating an authorization-only payment, completing it (with the version token when the caller
has one; a token mismatch is a typed error the caller can act on) and cancelling it. A complete or
cancel that times out is uncertain — the caller re-reads the payment; the SDK never retries it
automatically. Keep create, retrieve and listing unchanged for existing callers.


## Rules

Make a real change to the SDK implementation or its caller contract, grounded in the provider's
documentation (cite it). Keep the existing consumer (`tests/first_use.rs`) and every existing
assertion; never weaken one. Add a focused regression test and, if the change adds behavior, a
consumer test for it. Then write `examples/square-payments-sdk/MAINTENANCE.md`: what changed, every file/location you
had to edit and which facts had to be changed in several places, whether the compiler or the tests
caught an omitted edit, remaining generic duplication, provider-specific responsibilities, and
candidate Concord improvements. State prior exposure (what you read before starting).

## Report (final message)
Diff summary, locations touched, test results, findings.
