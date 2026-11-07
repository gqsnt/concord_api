# Fresh consumer capsule — SDK25 FRED

You are a **fresh consumer** of the SDK in `examples/fred-sdk` (package `concord-sdk-fred`), which you did not write.
Read only its public documentation first: `examples/fred-sdk/README.md` and the rustdoc you generate
(`cargo doc -p concord-sdk-fred --no-deps`), plus Concord's public docs if needed. Do **not** read
`examples/fred-sdk/src`, its `tests/`, `FRICTION.md` or `PROVENANCE.md` before your first working call. If
you must, write down what and why first — it changes the evidence label, not your permission.

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
  client → scope → endpoint is fixed. The frozen baseline is commit `4f3cc65f`: **do not edit
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

## Before writing code: predict

Write `examples/fred-sdk/FIRST_USE.md` with a section "Predictions" answering: for each workflow, which calls
you will make, what they send on the wire (method, path, query, headers), what they return, and
when/how they fail (status errors, provider errors, malformed bodies, pagination end, secrets).

## Then build a small application (in `examples/fred-sdk/tests/first_use.rs`)

Two caller tasks, one per workflow, written as tests against the loopback server with realistic
values and assertions on wire requests, errors, metadata, completion and partial results. Use the
ordinary caller path; use advanced APIs only where the task needs them.

Append to `FIRST_USE.md`: your first working call (what it took), every friction incident in
order (unneeded imports or types, unclear completion, surprising generated names, misleading
diagnostics, source lookups), a comparison of each prediction with observed behaviour, and the
assistance label (`docs-only` or `source-assisted: <files>`). Do not edit the SDK's `src/`.

## Report (final message)
Test results, prediction mismatches, friction list, assistance label.
