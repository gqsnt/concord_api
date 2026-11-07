# Independent maintenance capsule — SDK22 NASA NeoWs

You maintain the SDK in `examples/nasa-neows-sdk` (package `concord-sdk-nasa-neows`), which you did not write, after its first
consumer. First attempt the ticket below **before** reading `FRICTION.md` or `FIRST_USE.md`.

## Provider slice: SDK22 · NASA NeoWs (small; public data)

Connected workflows (both required):
1. Query a date-range asteroid feed, then look up an asteroid by ID.
2. Browse near-Earth objects, then inspect one returned object.

Contract notes: JSON and query API key, with DEMO_KEY subject to constrained usage.
Failure that must be exercised: A key/quota failure must surface without leaking the key in errors.
Excluded: All other NASA APIs and a claim that demo quotas equal normal quotas.
Official documentation (read-only research is allowed; cite the exact pages and the date you read
them):
- https://api.nasa.gov/
Live provider calls: **NOT_RUN** — never call the real service; every workflow runs against local
fixtures (`examples/testserver`: `TestServer::start(|request: &Recorded| -> Reply)`).

## Fixed rules

- Concord's product is its core, macros, declarations and generated usage; the hierarchy
  client → scope → endpoint is fixed. The frozen baseline is commit `21faa21e`: **do not edit
  any shared crate** (`crates/*`, `checks/*`, other examples, `scripts/*`) — record what you would
  change instead. Your ownership is `examples/nasa-neows-sdk/` plus adding `"examples/nasa-neows-sdk"` to the root `Cargo.toml`
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

api.nasa.gov reports per-key quota on every response with `X-RateLimit-Limit` and
`X-RateLimit-Remaining` (api.nasa.gov "Web Service Rate Limits"; cite it). The application needs
the remaining quota after every successful call **and** after a failed one (including the quota
error itself), without ever exposing the API key. Expose it on the SDK's success and error results
for every endpoint of the slice, with a clear "absent/unparsable" outcome.


## Rules

Make a real change to the SDK implementation or its caller contract, grounded in the provider's
documentation (cite it). Keep the existing consumer (`tests/first_use.rs`) and every existing
assertion; never weaken one. Add a focused regression test and, if the change adds behavior, a
consumer test for it. Then write `examples/nasa-neows-sdk/MAINTENANCE.md`: what changed, every file/location you
had to edit and which facts had to be changed in several places, whether the compiler or the tests
caught an omitted edit, remaining generic duplication, provider-specific responsibilities, and
candidate Concord improvements. State prior exposure (what you read before starting).

## Report (final message)
Diff summary, locations touched, test results, findings.
