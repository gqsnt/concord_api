# Independent maintenance capsule — SDK35 Xero Accounting

You maintain the SDK in `examples/xero-accounting-sdk` (package `concord-sdk-xero-accounting`), which you did not write, after its first
consumer. First attempt the ticket below **before** reading `FRICTION.md` or `FIRST_USE.md`.

## Provider slice: SDK35 · Xero Accounting (complex; payments and accounting)

Connected workflows (both required):
1. Page invoices for an authorized tenant and retrieve a chosen invoice.
2. Create a draft invoice and retrieve its state within the same tenant.

Contract notes: OAuth authorization-code refresh requires offline_access; tenant header; XML default and JSON opt-in; paging metadata varies.
Failure that must be exercised: A rotated refresh token must be saved atomically; never move a token or invoice across tenants.
Excluded: Payroll, bank feeds, full invoice state machine and every Accounting endpoint.
Official documentation (read-only research is allowed; cite the exact pages and the date you read
them):
- https://developer.xero.com/documentation/api/accounting/requests-and-responses/
- https://developer.xero.com/documentation/guides/oauth2/token-types/
- https://developer.xero.com/documentation/guides/oauth2/scopes/
Live provider calls: **NOT_RUN** — never call the real service; every workflow runs against local
fixtures (`examples/testserver`: `TestServer::start(|request: &Recorded| -> Reply)`).

## Fixed rules

- Concord's product is its core, macros, declarations and generated usage; the hierarchy
  client → scope → endpoint is fixed. The frozen baseline is commit `14b9e1e0a89775dbe5cbb9f7837bde677ea5878e`: **do not edit
  any shared crate** (`crates/*`, `checks/*`, other examples, `scripts/*`) — record what you would
  change instead. Your ownership is `examples/xero-accounting-sdk/` plus adding `"examples/xero-accounting-sdk"` to the root `Cargo.toml`
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

Callers need an incremental invoice sync: list only the invoices modified since a point in time.
Xero's Accounting API accepts an `If-Modified-Since` request header (a UTC timestamp) on
`GET Invoices`, combined with paging. Read the invoices reference and the requests-and-responses
page and cite them with the date:
- https://developer.xero.com/documentation/api/accounting/invoices
- https://developer.xero.com/documentation/api/accounting/requests-and-responses/

Add a "modified since" listing for the authorized tenant that keeps the existing page traversal
(resume, cap, completion) and keeps the same timestamp across all pages of one traversal, including
a resumed one; a caller can record the time at which a completed sync started and use it for the
next sync. The timestamp must be valid UTC and must never be sent to another tenant. Keep the
existing listing, retrieval and draft-invoice creation unchanged for existing callers.


## Rules

Make a real change to the SDK implementation or its caller contract, grounded in the provider's
documentation (cite it). Keep the existing consumer (`tests/first_use.rs`) and every existing
assertion; never weaken one. Add a focused regression test and, if the change adds behavior, a
consumer test for it. Then write `examples/xero-accounting-sdk/MAINTENANCE.md`: what changed, every file/location you
had to edit and which facts had to be changed in several places, whether the compiler or the tests
caught an omitted edit, remaining generic duplication, provider-specific responsibilities, and
candidate Concord improvements. State prior exposure (what you read before starting).

## Report (final message)
Diff summary, locations touched, test results, findings.
