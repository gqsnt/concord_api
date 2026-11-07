# Independent maintenance capsule — SDK25 FRED

You maintain the SDK in `examples/fred-sdk` (package `concord-sdk-fred`), which you did not write, after its first
consumer. First attempt the ticket below **before** reading `FRICTION.md` or `FIRST_USE.md`.

## Provider slice: SDK25 · FRED (medium; public data)

Connected workflows (both required):
1. Search series and retrieve the selected series metadata.
2. Read observations as XML or JSON and export the supported zipped CSV.

Contract notes: FRED v1 query API key; XML default, JSON and ZIP containing CSV for observations; limit/offset.
Failure that must be exercised: Compressed CSV is not a plain CSV body; reject oversized archives and preserve missing-value strings.
Excluded: FRED v2 bulk release archive and XLSX parsing.
Official documentation (read-only research is allowed; cite the exact pages and the date you read
them):
- https://fred.stlouisfed.org/docs/api/fred/series_search.html
- https://fred.stlouisfed.org/docs/api/fred/series_observations.html
Live provider calls: **NOT_RUN** — never call the real service; every workflow runs against local
fixtures (`examples/testserver`: `TestServer::start(|request: &Recorded| -> Reply)`).

## Fixed rules

- Concord's product is its core, macros, declarations and generated usage; the hierarchy
  client → scope → endpoint is fixed. The frozen baseline is commit `e1995ff7`: **do not edit
  any shared crate** (`crates/*`, `checks/*`, other examples, `scripts/*`) — record what you would
  change instead. Your ownership is `examples/fred-sdk/` plus adding `"examples/fred-sdk"` to the root `Cargo.toml`
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

FRED series observations accept `units` (lin, chg, ch1, pch, pc1, pca, cch, cca, log) and
`frequency` aggregation (d, w, bw, m, q, sa, a, plus the documented weekly variants) with
`aggregation_method` (avg, sum, eop) (FRED series/observations documentation; cite it). Add them so
they apply identically to the JSON, XML and zipped-CSV forms of observations, validated against the
documented value sets, and keep missing-value strings (".") distinct from numbers in every form.


## Rules

Make a real change to the SDK implementation or its caller contract, grounded in the provider's
documentation (cite it). Keep the existing consumer (`tests/first_use.rs`) and every existing
assertion; never weaken one. Add a focused regression test and, if the change adds behavior, a
consumer test for it. Then write `examples/fred-sdk/MAINTENANCE.md`: what changed, every file/location you
had to edit and which facts had to be changed in several places, whether the compiler or the tests
caught an omitted edit, remaining generic duplication, provider-specific responsibilities, and
candidate Concord improvements. State prior exposure (what you read before starting).

## Report (final message)
Diff summary, locations touched, test results, findings.
