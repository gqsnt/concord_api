# Independent maintenance capsule — SDK30 HubSpot CRM

You maintain the SDK in `examples/hubspot-crm-sdk` (package `concord-sdk-hubspot-crm`), which you did not write, after its first
consumer. First attempt the ticket below **before** reading `FRICTION.md` or `FIRST_USE.md`.

## Provider slice: SDK30 · HubSpot CRM (complex; messaging and CRM)

Connected workflows (both required):
1. Search contacts, follow paging.next.after, then retrieve a chosen contact.
2. Create a contact and later look it up by ID.

Contract notes: OAuth access/refresh token with possible replacement refresh token; search continuation; write ambiguity.
Failure that must be exercised: Concurrent refresh must not overwrite a newer replacement token; a timed-out create remains uncertain.
Excluded: Marketing automation, all object types and exactly-once contact creation.
Official documentation (read-only research is allowed; cite the exact pages and the date you read
them):
- https://developers.hubspot.com/blog/production-ready-oauth-token-management-hubspot-integrations
- https://developers.hubspot.com/blog/a-developers-guide-to-hubspot-crm-objects-contacts-object
Live provider calls: **NOT_RUN** — never call the real service; every workflow runs against local
fixtures (`examples/testserver`: `TestServer::start(|request: &Recorded| -> Reply)`).

## Fixed rules

- Concord's product is its core, macros, declarations and generated usage; the hierarchy
  client → scope → endpoint is fixed. The frozen baseline is commit `a171f3779a7d139204a8bf61d3341df43edff0de`: **do not edit
  any shared crate** (`crates/*`, `checks/*`, other examples, `scripts/*`) — record what you would
  change instead. Your ownership is `examples/hubspot-crm-sdk/` plus adding `"examples/hubspot-crm-sdk"` to the root `Cargo.toml`
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

Callers need to read several contacts by ID in one request. HubSpot's CRM API documents batch read
(`POST /crm/v3/objects/contacts/batch/read` with `inputs: [{id}]` and `properties`; read the
contacts/batch reference and cite it with the date). The response can be complete (`200`) or
partial (`207 Multi-Status`, results plus an `errors` array naming the IDs that failed). Add batch
read so a caller gets, per requested ID, either the contact or a typed per-ID error (with the
provider's category, no provider message in `Display`/`Debug`), using the existing OAuth refresh
path; a batch read is a safe read but must not be retried automatically by the SDK. Keep search,
retrieval and creation unchanged.


## Rules

Make a real change to the SDK implementation or its caller contract, grounded in the provider's
documentation (cite it). Keep the existing consumer (`tests/first_use.rs`) and every existing
assertion; never weaken one. Add a focused regression test and, if the change adds behavior, a
consumer test for it. Then write `examples/hubspot-crm-sdk/MAINTENANCE.md`: what changed, every file/location you
had to edit and which facts had to be changed in several places, whether the compiler or the tests
caught an omitted edit, remaining generic duplication, provider-specific responsibilities, and
candidate Concord improvements. State prior exposure (what you read before starting).

## Report (final message)
Diff summary, locations touched, test results, findings.
