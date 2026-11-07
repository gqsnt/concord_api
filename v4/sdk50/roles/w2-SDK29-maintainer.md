# Independent maintenance capsule — SDK29 Intercom

You maintain the SDK in `examples/intercom-sdk` (package `concord-sdk-intercom`), which you did not write, after its first
consumer. First attempt the ticket below **before** reading `FRICTION.md` or `FIRST_USE.md`.

## Provider slice: SDK29 · Intercom (medium; messaging and CRM)

Connected workflows (both required):
1. List contacts using provider pagination, then retrieve one contact.
2. Create a contact and retrieve it by returned identifier.

Contract notes: Bearer token and version header; 429 response may lack rate-limit headers.
Failure that must be exercised: Missing reset headers on a 429 must not produce a busy loop or invented reset time.
Excluded: Conversations, webhooks and automatically retrying ambiguous creates.
Official documentation (read-only research is allowed; cite the exact pages and the date you read
them):
- https://developers.intercom.com/docs/references/rest-api/api.intercom.io/contacts/listcontacts
- https://developers.intercom.com/docs/references/rest-api/errors/rate-limiting
Live provider calls: **NOT_RUN** — never call the real service; every workflow runs against local
fixtures (`examples/testserver`: `TestServer::start(|request: &Recorded| -> Reply)`).

## Fixed rules

- Concord's product is its core, macros, declarations and generated usage; the hierarchy
  client → scope → endpoint is fixed. The frozen baseline is commit `8a8befa6ebed33904bde7d28bbfbcc9acbcc4919`: **do not edit
  any shared crate** (`crates/*`, `checks/*`, other examples, `scripts/*`) — record what you would
  change instead. Your ownership is `examples/intercom-sdk/` plus adding `"examples/intercom-sdk"` to the root `Cargo.toml`
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

Callers need to find contacts by email. Intercom's REST API documents contact search
(`POST /contacts/search`: a query object with `field`/`operator`/`value`, and pagination
`per_page` and `starting_after` inside the JSON request body, not in the URL — read the Search
contacts reference and the pagination section, cite them with the date). Add search by exact
email with the provider's cursor paging so a caller can page through all matches with the same
bounded, resumable traversal the SDK already offers for listing; a headerless 429 during search
must behave like listing (prefix and failed cursor kept, unknown reset, no retry). Keep listing,
retrieval and creation unchanged.


## Rules

Make a real change to the SDK implementation or its caller contract, grounded in the provider's
documentation (cite it). Keep the existing consumer (`tests/first_use.rs`) and every existing
assertion; never weaken one. Add a focused regression test and, if the change adds behavior, a
consumer test for it. Then write `examples/intercom-sdk/MAINTENANCE.md`: what changed, every file/location you
had to edit and which facts had to be changed in several places, whether the compiler or the tests
caught an omitted edit, remaining generic duplication, provider-specific responsibilities, and
candidate Concord improvements. State prior exposure (what you read before starting).

## Report (final message)
Diff summary, locations touched, test results, findings.
