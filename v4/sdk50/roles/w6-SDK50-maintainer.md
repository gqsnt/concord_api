# Independent maintenance capsule — SDK50 REST Countries

You maintain the SDK in `examples/rest-countries-sdk` (package `concord-sdk-rest-countries`), which you did not write, after its first
consumer. First attempt the ticket below **before** reading `FRICTION.md` or `FIRST_USE.md`.

## Provider slice: SDK50 · REST Countries (small; public data)

Connected workflows (both required):
1. Search country names and fetch one chosen country by code.
2. Filter countries by a documented property and select requested response fields.

Contract notes: Provider documents versioned Countries v5 endpoints and a response-field allowlist; verify current access terms before use.
Failure that must be exercised: A no-match result or unavailable field must remain an ordinary provider outcome, not trigger pagination or auth machinery.
Excluded: Other provider products, offline mirroring, historical datasets and inferred v3 compatibility.
Official documentation (read-only research is allowed; cite the exact pages and the date you read
them):
- https://restcountries.com/docs/countries
- https://restcountries.com/docs/countries/api-versions
Live provider calls: **NOT_RUN** — never call the real service; every workflow runs against local
fixtures (`examples/testserver`: `TestServer::start(|request: &Recorded| -> Reply)`).

## Fixed rules

- Concord's product is its core, macros, declarations and generated usage; the hierarchy
  client → scope → endpoint is fixed. The frozen baseline is commit `c3fd4f1ee30c1727e26b285270f2cc8d8b694e77`: **do not edit
  any shared crate** (`crates/*`, `checks/*`, other examples, `scripts/*`) — record what you would
  change instead. Your ownership is `examples/rest-countries-sdk/` plus adding `"examples/rest-countries-sdk"` to the root `Cargo.toml`
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

Callers need the countries of a region and of a language: the Countries API documents reading by
the `region` / `subregion` property and by `languages`, alongside the existing name and code
lookups, with the same field selection and paging. Read the Countries API page and cite it with
the date:
- https://restcountries.com/docs/countries

Add typed region, subregion and language lookups (values validated before any request; the same
`response_fields` selection, page options and bounded walk as the existing listing; no-match is
an empty page, not an error). Keep name search, code lookup and the filtered listing unchanged
for existing callers.


## Rules

Make a real change to the SDK implementation or its caller contract, grounded in the provider's
documentation (cite it). Keep the existing consumer (`tests/first_use.rs`) and every existing
assertion; never weaken one. Add a focused regression test and, if the change adds behavior, a
consumer test for it. Then write `examples/rest-countries-sdk/MAINTENANCE.md`: what changed, every file/location you
had to edit and which facts had to be changed in several places, whether the compiler or the tests
caught an omitted edit, remaining generic duplication, provider-specific responsibilities, and
candidate Concord improvements. State prior exposure (what you read before starting).

## Report (final message)
Diff summary, locations touched, test results, findings.
