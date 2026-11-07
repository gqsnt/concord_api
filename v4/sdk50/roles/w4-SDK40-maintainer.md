# Independent maintenance capsule — SDK40 Cloudflare Zones

You maintain the SDK in `examples/cloudflare-zones-sdk` (package `concord-sdk-cloudflare-zones`), which you did not write, after its first
consumer. First attempt the ticket below **before** reading `FRICTION.md` or `FIRST_USE.md`.

## Provider slice: SDK40 · Cloudflare Zones (small; infrastructure and observability)

Connected workflows (both required):
1. List zones for an account with paging, then fetch one zone by ID.
2. List DNS records within the selected zone, then fetch one record.

Contract notes: Bearer API token; per-endpoint pagination envelopes; global and service limits can coexist.
Failure that must be exercised: HTTP success containing success=false must surface provider errors rather than an empty result.
Excluded: Zone mutation, Workers, WAF, GraphQL and global token-bucket assumptions.
Official documentation (read-only research is allowed; cite the exact pages and the date you read
them):
- https://developers.cloudflare.com/api/resources/zones/methods/list/
- https://developers.cloudflare.com/api/resources/dns/subresources/records/methods/list/
- https://developers.cloudflare.com/fundamentals/api/reference/limits/
Live provider calls: **NOT_RUN** — never call the real service; every workflow runs against local
fixtures (`examples/testserver`: `TestServer::start(|request: &Recorded| -> Reply)`).

## Fixed rules

- Concord's product is its core, macros, declarations and generated usage; the hierarchy
  client → scope → endpoint is fixed. The frozen baseline is commit `c3fd4f1ee30c1727e26b285270f2cc8d8b694e77`: **do not edit
  any shared crate** (`crates/*`, `checks/*`, other examples, `scripts/*`) — record what you would
  change instead. Your ownership is `examples/cloudflare-zones-sdk/` plus adding `"examples/cloudflare-zones-sdk"` to the root `Cargo.toml`
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

Callers need to create a DNS record in a zone: `POST /zones/{zone_id}/dns_records` with the
record's type, name, content, TTL and (where the type allows) `proxied`; the envelope's `result`
is the created record. Read the create-record reference and cite it with the date:
- https://developers.cloudflare.com/api/resources/dns/subresources/records/methods/create/

Add the create so a caller gets the created record or a typed provider error (an envelope with
`success: false`, including "record already exists", is never a success); inputs outside the
documented constraints are refused before any request; a timed-out create is uncertain (the
caller lists the zone's records to reconcile) and is never resent automatically, including by the
SDK's opt-in 429 retry. Keep zone and record listing and retrieval unchanged for existing callers.


## Rules

Make a real change to the SDK implementation or its caller contract, grounded in the provider's
documentation (cite it). Keep the existing consumer (`tests/first_use.rs`) and every existing
assertion; never weaken one. Add a focused regression test and, if the change adds behavior, a
consumer test for it. Then write `examples/cloudflare-zones-sdk/MAINTENANCE.md`: what changed, every file/location you
had to edit and which facts had to be changed in several places, whether the compiler or the tests
caught an omitted edit, remaining generic duplication, provider-specific responsibilities, and
candidate Concord improvements. State prior exposure (what you read before starting).

## Report (final message)
Diff summary, locations touched, test results, findings.
