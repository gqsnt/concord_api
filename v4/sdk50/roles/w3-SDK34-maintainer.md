# Independent maintenance capsule — SDK34 QuickBooks Online

You maintain the SDK in `examples/quickbooks-online-sdk` (package `concord-sdk-quickbooks-online`), which you did not write, after its first
consumer. First attempt the ticket below **before** reading `FRICTION.md` or `FIRST_USE.md`.

## Provider slice: SDK34 · QuickBooks Online (medium; payments and accounting)

Connected workflows (both required):
1. Query customer records and fetch a selected customer within the same realm.
2. Create a sandbox invoice for a customer and retrieve the invoice.

Contract notes: OAuth authorization-code and refresh tokens; realm-scoped Accounting API.
Failure that must be exercised: A replaced or expired refresh token must surface a recoverable authorization error, not silently cross realms.
Excluded: Payroll, tax workflows, full SQL dialect and automatically replaying invoice creation.
Official documentation (read-only research is allowed; cite the exact pages and the date you read
them):
- https://static.developer.intuit.com/output_html/qbo/docs/develop/authentication-and-authorization/oauth-2.0.html
- https://developer.intuit.com/app/developer/qbo/docs/api/accounting/all-entities/customer
- https://developer.intuit.com/app/developer/qbo/docs/api/accounting/all-entities/invoice
Live provider calls: **NOT_RUN** — never call the real service; every workflow runs against local
fixtures (`examples/testserver`: `TestServer::start(|request: &Recorded| -> Reply)`).

## Fixed rules

- Concord's product is its core, macros, declarations and generated usage; the hierarchy
  client → scope → endpoint is fixed. The frozen baseline is commit `14b9e1e0a89775dbe5cbb9f7837bde677ea5878e`: **do not edit
  any shared crate** (`crates/*`, `checks/*`, other examples, `scripts/*`) — record what you would
  change instead. Your ownership is `examples/quickbooks-online-sdk/` plus adding `"examples/quickbooks-online-sdk"` to the root `Cargo.toml`
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

Callers need to record payments against invoices. In the Accounting API a `Payment` entity is
created with `POST /v3/company/{realmId}/payment` (a customer reference, a total amount, and
`Line` entries whose `LinkedTxn` names the invoice being paid); reading the invoice afterwards
shows its reduced `Balance`. Read the Payment entity reference and the `requestid` guidance and
cite them with the date:
- https://developer.intuit.com/app/developer/qbo/docs/api/accounting/all-entities/payment
- https://developer.intuit.com/app/developer/qbo/docs/develop/rest-api-features

Add: recording a payment for one invoice (amount paid; partial payments allowed) and reading the
invoice's balance afterwards, within the connection's realm only. Payment creation follows the
same rule as invoice creation: it always carries a `requestid`, is never resent by the SDK, and a
timed-out create is uncertain with that request id. Amounts keep their exact decimal text. Keep
customer query/read and invoice create/read unchanged for existing callers.


## Rules

Make a real change to the SDK implementation or its caller contract, grounded in the provider's
documentation (cite it). Keep the existing consumer (`tests/first_use.rs`) and every existing
assertion; never weaken one. Add a focused regression test and, if the change adds behavior, a
consumer test for it. Then write `examples/quickbooks-online-sdk/MAINTENANCE.md`: what changed, every file/location you
had to edit and which facts had to be changed in several places, whether the compiler or the tests
caught an omitted edit, remaining generic duplication, provider-specific responsibilities, and
candidate Concord improvements. State prior exposure (what you read before starting).

## Report (final message)
Diff summary, locations touched, test results, findings.
