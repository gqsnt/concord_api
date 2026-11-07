# Independent maintenance capsule — SDK49 Europe PMC

You maintain the SDK in `examples/europe-pmc-sdk` (package `concord-sdk-europe-pmc`), which you did not write, after its first
consumer. First attempt the ticket below **before** reading `FRICTION.md` or `FIRST_USE.md`.

## Provider slice: SDK49 · Europe PMC (small; public data)

Connected workflows (both required):
1. Search publications in JSON and fetch one selected citation.
2. Run the same bounded search in XML and continue with the returned cursor mark.

Contract notes: Public REST API supports XML and JSON; cursorMark for deep paging.
Failure that must be exercised: An XML default response must not be fed to the JSON decoder; stop if a cursor repeats.
Excluded: Full-text PDF, text mining annotation APIs and harvesting all literature.
Official documentation (read-only research is allowed; cite the exact pages and the date you read
them):
- https://dev.europepmc.org/RestfulWebService
- https://www.ebi.ac.uk/training/online/courses/embl-ebi-programmatically/wp-content/uploads/sites/128/2020/11/Webinar-slides-Europe-PMC-programmatically_2020.pdf
Live provider calls: **NOT_RUN** — never call the real service; every workflow runs against local
fixtures (`examples/testserver`: `TestServer::start(|request: &Recorded| -> Reply)`).

## Fixed rules

- Concord's product is its core, macros, declarations and generated usage; the hierarchy
  client → scope → endpoint is fixed. The frozen baseline is commit `c3fd4f1ee30c1727e26b285270f2cc8d8b694e77`: **do not edit
  any shared crate** (`crates/*`, `checks/*`, other examples, `scripts/*`) — record what you would
  change instead. Your ownership is `examples/europe-pmc-sdk/` plus adding `"examples/europe-pmc-sdk"` to the root `Cargo.toml`
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

Callers need a publication's reference list: the RESTful web service documents
`GET /{source}/{id}/references` with `page` and `pageSize` (and the JSON/XML `format`), returning
the references and a hit count. Read the RESTful web service page and cite it with the date:
- https://dev.europepmc.org/RestfulWebService

Add a references traversal for one publication: page-number paging until the hit count is
reached (with a page cap and a resume point; the same page size on every page), in the SDK's
existing JSON and XML modes with the same media and size rules, keeping the references of
completed pages when a later page fails. Keep search and citations unchanged for existing
callers.


## Rules

Make a real change to the SDK implementation or its caller contract, grounded in the provider's
documentation (cite it). Keep the existing consumer (`tests/first_use.rs`) and every existing
assertion; never weaken one. Add a focused regression test and, if the change adds behavior, a
consumer test for it. Then write `examples/europe-pmc-sdk/MAINTENANCE.md`: what changed, every file/location you
had to edit and which facts had to be changed in several places, whether the compiler or the tests
caught an omitted edit, remaining generic duplication, provider-specific responsibilities, and
candidate Concord improvements. State prior exposure (what you read before starting).

## Report (final message)
Diff summary, locations touched, test results, findings.
