# Independent maintenance capsule — SDK31 Adyen Checkout

You maintain the SDK in `examples/adyen-checkout-sdk` (package `concord-sdk-adyen-checkout`), which you did not write, after its first
consumer. First attempt the ticket below **before** reading `FRICTION.md` or `FIRST_USE.md`.

## Provider slice: SDK31 · Adyen Checkout (complex; payments and accounting)

Connected workflows (both required):
1. List available payment methods and submit a sandbox payment.
2. Retrieve or submit the next payment action/details for an existing payment flow.

Contract notes: Versioned Checkout endpoints, X-API-Key, idempotency-key support for POST; payment states may be asynchronous.
Failure that must be exercised: A retry must reuse the same key and identical request; a changed body or expired guarantee requires reconciliation.
Excluded: Recurring payments, live card capture, webhook verification and universal 3DS orchestration.
Official documentation (read-only research is allowed; cite the exact pages and the date you read
them):
- https://docs.adyen.com/api-explorer/Checkout/latest/overview
- https://docs.adyen.com/development-resources/api-idempotency/
Live provider calls: **NOT_RUN** — never call the real service; every workflow runs against local
fixtures (`examples/testserver`: `TestServer::start(|request: &Recorded| -> Reply)`).

## Fixed rules

- Concord's product is its core, macros, declarations and generated usage; the hierarchy
  client → scope → endpoint is fixed. The frozen baseline is commit `14b9e1e0a89775dbe5cbb9f7837bde677ea5878e`: **do not edit
  any shared crate** (`crates/*`, `checks/*`, other examples, `scripts/*`) — record what you would
  change instead. Your ownership is `examples/adyen-checkout-sdk/` plus adding `"examples/adyen-checkout-sdk"` to the root `Cargo.toml`
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

Callers need manual capture. With manual capture configured, an authorised payment is captured
later with `POST /payments/{paymentPspReference}/captures` (the merchant account, the amount — the
full authorised amount or less for a partial capture — and an optional reference), sent with an
`Idempotency-Key`. Adyen answers `201` with status `received`: the capture is accepted for
processing and its final outcome arrives asynchronously (webhooks are out of scope). Read the
captures endpoint reference and the capture guide and cite them with the date:
- https://docs.adyen.com/api-explorer/Checkout/71/post/payments/(paymentPspReference)/captures
- https://docs.adyen.com/online-payments/capture

Add capture so a caller gets a typed "capture received" result (its PSP reference and the amount
requested), never a claim that money moved. A capture obeys the same identical-request rule as a
payment: a retry reuses its key with identical bytes; a changed amount under a reused key is
refused before sending; a timed-out capture is uncertain and is never replayed automatically
unless the SDK's existing guarantee rules allow it. Keep payment methods, payments and details
unchanged for existing callers.


## Rules

Make a real change to the SDK implementation or its caller contract, grounded in the provider's
documentation (cite it). Keep the existing consumer (`tests/first_use.rs`) and every existing
assertion; never weaken one. Add a focused regression test and, if the change adds behavior, a
consumer test for it. Then write `examples/adyen-checkout-sdk/MAINTENANCE.md`: what changed, every file/location you
had to edit and which facts had to be changed in several places, whether the compiler or the tests
caught an omitted edit, remaining generic duplication, provider-specific responsibilities, and
candidate Concord improvements. State prior exposure (what you read before starting).

## Report (final message)
Diff summary, locations touched, test results, findings.
