# Independent maintenance capsule — SDK43 Sentry

You maintain the SDK in `examples/sentry-sdk` (package `concord-sdk-sentry`), which you did not write, after its first
consumer. First attempt the ticket below **before** reading `FRICTION.md` or `FIRST_USE.md`.

## Provider slice: SDK43 · Sentry (medium; infrastructure and observability)

Connected workflows (both required):
1. List organization issues by cursor and retrieve a chosen issue.
2. Page an issue’s events using the Link header and retrieve an event.

Contract notes: Bearer token; Link header contains cursor and results flag; distinct frequency and concurrency limits.
Failure that must be exercised: A next Link with results=false must stop; retry scheduling must not ignore concurrency.
Excluded: Event ingestion SDK, releases mutation and organization administration.
Official documentation (read-only research is allowed; cite the exact pages and the date you read
them):
- https://docs.sentry.io/api/pagination/
- https://docs.sentry.io/api/ratelimits/
- https://docs.sentry.io/api/events/
Live provider calls: **NOT_RUN** — never call the real service; every workflow runs against local
fixtures (`examples/testserver`: `TestServer::start(|request: &Recorded| -> Reply)`).

## Fixed rules

- Concord's product is its core, macros, declarations and generated usage; the hierarchy
  client → scope → endpoint is fixed. The frozen baseline is commit `c3fd4f1ee30c1727e26b285270f2cc8d8b694e77`: **do not edit
  any shared crate** (`crates/*`, `checks/*`, other examples, `scripts/*`) — record what you would
  change instead. Your ownership is `examples/sentry-sdk/` plus adding `"examples/sentry-sdk"` to the root `Cargo.toml`
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

Callers need to resolve or ignore an issue: Sentry's organization issue endpoint accepts a
`PUT` that updates an issue's status (`resolved`, `ignored`, `unresolved`, with the documented
status details) and returns the updated issue. Read the issue update reference and cite it with
the date:
- https://docs.sentry.io/api/events/update-an-issue/

Add the status update so a caller sends only the change it means and gets the updated issue back;
a status or detail outside the documented set is refused before any request; the write respects
the same frequency and concurrency limits as reads; a timed-out update is uncertain (the caller
re-reads the issue) and is never retried automatically. Keep issue and event listing and
retrieval unchanged for existing callers.


## Rules

Make a real change to the SDK implementation or its caller contract, grounded in the provider's
documentation (cite it). Keep the existing consumer (`tests/first_use.rs`) and every existing
assertion; never weaken one. Add a focused regression test and, if the change adds behavior, a
consumer test for it. Then write `examples/sentry-sdk/MAINTENANCE.md`: what changed, every file/location you
had to edit and which facts had to be changed in several places, whether the compiler or the tests
caught an omitted edit, remaining generic duplication, provider-specific responsibilities, and
candidate Concord improvements. State prior exposure (what you read before starting).

## Report (final message)
Diff summary, locations touched, test results, findings.
