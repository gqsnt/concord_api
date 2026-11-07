# Independent maintenance capsule — SDK28 Zendesk Support

You maintain the SDK in `examples/zendesk-support-sdk` (package `concord-sdk-zendesk-support`), which you did not write, after its first
consumer. First attempt the ticket below **before** reading `FRICTION.md` or `FIRST_USE.md`.

## Provider slice: SDK28 · Zendesk Support (medium; messaging and CRM)

Connected workflows (both required):
1. List tickets using cursor pagination, then retrieve a selected ticket.
2. Create a ticket and retrieve its resulting state.

Contract notes: OAuth bearer credentials; cursor or offset varies by endpoint; 429 can carry Retry-After.
Failure that must be exercised: Honor Retry-After for safe reads; surface an uncertain create without duplicating the ticket.
Excluded: Help Center, incremental exports, attachments and blanket auto-retry.
Official documentation (read-only research is allowed; cite the exact pages and the date you read
them):
- https://developer.zendesk.com/api-reference/ticketing/tickets/tickets/
- https://developer.zendesk.com/api-reference/introduction/pagination/
- https://developer.zendesk.com/documentation/api-basics/best-practices/best-practices-for-avoiding-rate-limiting/
Live provider calls: **NOT_RUN** — never call the real service; every workflow runs against local
fixtures (`examples/testserver`: `TestServer::start(|request: &Recorded| -> Reply)`).

## Fixed rules

- Concord's product is its core, macros, declarations and generated usage; the hierarchy
  client → scope → endpoint is fixed. The frozen baseline is commit `7c2479eef31f13a45d46f876a01222aa06416215`: **do not edit
  any shared crate** (`crates/*`, `checks/*`, other examples, `scripts/*`) — record what you would
  change instead. Your ownership is `examples/zendesk-support-sdk/` plus adding `"examples/zendesk-support-sdk"` to the root `Cargo.toml`
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

Agents need to change a ticket's status and add a comment without overwriting someone else's
concurrent edit. Zendesk's Tickets API documents updating a ticket (`PUT /api/v2/tickets/{id}`)
and "protecting against ticket update collisions" with `safe_update: true` plus the ticket's
`updated_stamp`, answered with `409 Conflict` when the ticket changed meanwhile (read the Tickets
reference sections on updating and on update collisions; cite them with the date). Add an update
operation that takes the stamp from a previously read ticket, reports a collision as its own
error (so the caller can re-read and decide), and treats a lost response like the SDK's other
uncertain writes (never resent automatically). Keep listing, show and create unchanged.


## Rules

Make a real change to the SDK implementation or its caller contract, grounded in the provider's
documentation (cite it). Keep the existing consumer (`tests/first_use.rs`) and every existing
assertion; never weaken one. Add a focused regression test and, if the change adds behavior, a
consumer test for it. Then write `examples/zendesk-support-sdk/MAINTENANCE.md`: what changed, every file/location you
had to edit and which facts had to be changed in several places, whether the compiler or the tests
caught an omitted edit, remaining generic duplication, provider-specific responsibilities, and
candidate Concord improvements. State prior exposure (what you read before starting).

## Report (final message)
Diff summary, locations touched, test results, findings.
