# Independent maintenance capsule — SDK42 Datadog Events

You maintain the SDK in `examples/datadog-events-sdk` (package `concord-sdk-datadog-events`), which you did not write, after its first
consumer. First attempt the ticket below **before** reading `FRICTION.md` or `FIRST_USE.md`.

## Provider slice: SDK42 · Datadog Events (small; infrastructure and observability)

Connected workflows (both required):
1. List events over a bounded time range, then inspect one returned event.
2. Search events using a filter and follow the documented pagination.

Contract notes: Site-specific API host; read requests can require both API and application keys.
Failure that must be exercised: Wrong-region credentials or missing application key surface as authentication errors without exposing either secret.
Excluded: Metric submission, monitor lifecycle and all event schemas.
Official documentation (read-only research is allowed; cite the exact pages and the date you read
them):
- https://docs.datadoghq.com/api/latest/authentication/
- https://docs.datadoghq.com/api/latest/events/get-a-list-of-events/
- https://docs.datadoghq.com/api/latest/events/search-events/
Live provider calls: **NOT_RUN** — never call the real service; every workflow runs against local
fixtures (`examples/testserver`: `TestServer::start(|request: &Recorded| -> Reply)`).

## Fixed rules

- Concord's product is its core, macros, declarations and generated usage; the hierarchy
  client → scope → endpoint is fixed. The frozen baseline is commit `c3fd4f1ee30c1727e26b285270f2cc8d8b694e77`: **do not edit
  any shared crate** (`crates/*`, `checks/*`, other examples, `scripts/*`) — record what you would
  change instead. Your ownership is `examples/datadog-events-sdk/` plus adding `"examples/datadog-events-sdk"` to the root `Cargo.toml`
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

Callers need to post an event: Datadog's Events API accepts a new event (title, text, tags,
alert type, aggregation key, source) on the caller's site and answers that it was accepted. Read
the events API reference for posting an event and cite it with the date:
- https://docs.datadoghq.com/api/latest/events/

Add the post so a caller gets the provider's acceptance (never a claim that the event is already
searchable); inputs outside the documented limits are refused before any request; it uses the
same site host and both keys as the reads, with the same redaction and wrong-region rules; a post
that times out is uncertain (the event may exist) and is never resent automatically. Keep the
listing and search unchanged for existing callers.


## Rules

Make a real change to the SDK implementation or its caller contract, grounded in the provider's
documentation (cite it). Keep the existing consumer (`tests/first_use.rs`) and every existing
assertion; never weaken one. Add a focused regression test and, if the change adds behavior, a
consumer test for it. Then write `examples/datadog-events-sdk/MAINTENANCE.md`: what changed, every file/location you
had to edit and which facts had to be changed in several places, whether the compiler or the tests
caught an omitted edit, remaining generic duplication, provider-specific responsibilities, and
candidate Concord improvements. State prior exposure (what you read before starting).

## Report (final message)
Diff summary, locations touched, test results, findings.
