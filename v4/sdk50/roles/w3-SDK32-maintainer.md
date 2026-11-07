# Independent maintenance capsule — SDK32 PayPal

You maintain the SDK in `examples/paypal-sdk` (package `concord-sdk-paypal`), which you did not write, after its first
consumer. First attempt the ticket below **before** reading `FRICTION.md` or `FIRST_USE.md`.

## Provider slice: SDK32 · PayPal (complex; payments and accounting)

Connected workflows (both required):
1. Use client credentials to create an order and obtain its current state.
2. Capture an approved order and retrieve the resulting capture.

Contract notes: Client-credentials token acquisition, expiry and caching; PayPal-Request-Id supported per endpoint; 429 policy is not a published fixed quota.
Failure that must be exercised: Do not assume a refresh token, or retry a capture unless that specific operation permits its request ID.
Excluded: Buyer approval UI, refunds, payouts and blanket idempotency of every endpoint.
Official documentation (read-only research is allowed; cite the exact pages and the date you read
them):
- https://developer.paypal.com/api/rest/authentication
- https://developer.paypal.com/api/rest/requests/
- https://developer.paypal.com/api/rest/reference/rate-limiting/
- https://developer.paypal.com/api/rest/integration/orders-api
Live provider calls: **NOT_RUN** — never call the real service; every workflow runs against local
fixtures (`examples/testserver`: `TestServer::start(|request: &Recorded| -> Reply)`).

## Fixed rules

- Concord's product is its core, macros, declarations and generated usage; the hierarchy
  client → scope → endpoint is fixed. The frozen baseline is commit `14b9e1e0a89775dbe5cbb9f7837bde677ea5878e`: **do not edit
  any shared crate** (`crates/*`, `checks/*`, other examples, `scripts/*`) — record what you would
  change instead. Your ownership is `examples/paypal-sdk/` plus adding `"examples/paypal-sdk"` to the root `Cargo.toml`
  workspace `members` (and the `Cargo.lock` entry cargo writes).
- Build into the worktree's default `target/` (never set a relative `CARGO_TARGET_DIR`);
  `CARGO_BUILD_JOBS=4`, one cargo command at a time.
- Adding your package changes `Cargo.lock`: build once without `--locked` (offline if possible),
  then use `--locked`. Prefer dependencies already in `Cargo.lock`; justify any new crate (name,
  version, license, why) in `PROVENANCE.md`. No other package may change version in the lock.
- You cannot commit (`.git` is read-only); leave your work in the working tree.
- No private reasoning transcripts are wanted: record observable actions, diagnostics, files read
  and decisions.
- Read nothing outside your worktree except the official documentation pages above and the
  Rust toolchain / cargo registry sources: in particular, not the campaign files in its parent
  directories (mission, contracts, records, other roles' briefs or worktrees).

## Ticket

Callers need authorize-then-capture. An order created with `intent: AUTHORIZE` is authorized after
buyer approval with `POST /v2/checkout/orders/{id}/authorize`, which returns an authorization; the
authorization is later captured with `POST /v2/payments/authorizations/{authorization_id}/capture`
or released with `POST /v2/payments/authorizations/{authorization_id}/void`. Read the Orders v2
authorize reference and the Payments v2 authorizations reference (capture, void, and which of
these accept `PayPal-Request-Id`) and cite them with the date:
- https://developer.paypal.com/docs/api/orders/v2/
- https://developer.paypal.com/docs/api/payments/v2/

Add: creating an order with intent AUTHORIZE, authorizing it (the caller gets the authorization id
and status), capturing an authorization and voiding one. Each write follows the SDK's existing
rule: repeated only when the operation documents a request id and the caller supplied one, with
identical bytes; a timed-out authorize, capture or void is uncertain. Keep the existing CAPTURE
flow (create → capture order → retrieve capture) unchanged for existing callers.


## Rules

Make a real change to the SDK implementation or its caller contract, grounded in the provider's
documentation (cite it). Keep the existing consumer (`tests/first_use.rs`) and every existing
assertion; never weaken one. Add a focused regression test and, if the change adds behavior, a
consumer test for it. Then write `examples/paypal-sdk/MAINTENANCE.md`: what changed, every file/location you
had to edit and which facts had to be changed in several places, whether the compiler or the tests
caught an omitted edit, remaining generic duplication, provider-specific responsibilities, and
candidate Concord improvements. State prior exposure (what you read before starting).

## Report (final message)
Diff summary, locations touched, test results, findings.
