# Independent maintenance capsule — SDK23 USGS Earthquake Catalog

You maintain the SDK in `examples/usgs-earthquake-catalog-sdk` (package `concord-sdk-usgs-earthquake-catalog`), which you did not write, after its first
consumer. First attempt the ticket below **before** reading `FRICTION.md` or `FIRST_USE.md`.

## Provider slice: SDK23 · USGS Earthquake Catalog (medium; public data)

Connected workflows (both required):
1. Query a bounded time window as GeoJSON, then retrieve a chosen event.
2. Export the same filtered window as CSV and process rows incrementally.

Contract notes: FDSN event query supports GeoJSON, CSV and one-based offset; updates can change pages.
Failure that must be exercised: Quoted commas and newlines must survive CSV parsing; do not silently skip bad rows.
Excluded: Full QuakeML model, scientific validation and unbounded global harvesting.
Official documentation (read-only research is allowed; cite the exact pages and the date you read
them):
- https://earthquake.usgs.gov/fdsnws/event/1/
Live provider calls: **NOT_RUN** — never call the real service; every workflow runs against local
fixtures (`examples/testserver`: `TestServer::start(|request: &Recorded| -> Reply)`).

## Fixed rules

- Concord's product is its core, macros, declarations and generated usage; the hierarchy
  client → scope → endpoint is fixed. The frozen baseline is commit `9f59f9a6`: **do not edit
  any shared crate** (`crates/*`, `checks/*`, other examples, `scripts/*`) — record what you would
  change instead. Your ownership is `examples/usgs-earthquake-catalog-sdk/` plus adding `"examples/usgs-earthquake-catalog-sdk"` to the root `Cargo.toml`
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

Add the FDSN event filters `minmagnitude`, `maxmagnitude` and `orderby` (`time`, `time-asc`,
`magnitude`, `magnitude-asc`) to the slice (USGS FDSN event web service documentation; cite it).
They must apply identically to the GeoJSON query and the CSV export of the same filtered window,
validated where the documentation states constraints (e.g. min ≤ max), and the CSV export must
remain incremental.


## Rules

Make a real change to the SDK implementation or its caller contract, grounded in the provider's
documentation (cite it). Keep the existing consumer (`tests/first_use.rs`) and every existing
assertion; never weaken one. Add a focused regression test and, if the change adds behavior, a
consumer test for it. Then write `examples/usgs-earthquake-catalog-sdk/MAINTENANCE.md`: what changed, every file/location you
had to edit and which facts had to be changed in several places, whether the compiler or the tests
caught an omitted edit, remaining generic duplication, provider-specific responsibilities, and
candidate Concord improvements. State prior exposure (what you read before starting).

## Report (final message)
Diff summary, locations touched, test results, findings.
