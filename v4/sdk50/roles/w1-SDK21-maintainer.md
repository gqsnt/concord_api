# Independent maintenance capsule — SDK21 Open Library

You maintain the SDK in `examples/open-library-sdk` (package `concord-sdk-open-library`), which you did not write, after its first
consumer. First attempt the ticket below **before** reading `FRICTION.md` or `FIRST_USE.md`.

## Provider slice: SDK21 · Open Library (small; public data)

Connected workflows (both required):
1. Search works with page and limit, then retrieve a selected work by ID.
2. Search authors, then retrieve that author’s works.

Contract notes: Public JSON; page/limit only on applicable search endpoint.
Failure that must be exercised: An empty search page or a work with missing optional fields must not become a decoding crash.
Excluded: Editing, loans, covers and undocumented schema guarantees.
Official documentation (read-only research is allowed; cite the exact pages and the date you read
them):
- https://openlibrary.org/dev/docs/api/search
- https://openlibrary.org/dev/docs/api/authors
Live provider calls: **NOT_RUN** — never call the real service; every workflow runs against local
fixtures (`examples/testserver`: `TestServer::start(|request: &Recorded| -> Reply)`).

## Fixed rules

- Concord's product is its core, macros, declarations and generated usage; the hierarchy
  client → scope → endpoint is fixed. The frozen baseline is commit `ba6ac11b`: **do not edit
  any shared crate** (`crates/*`, `checks/*`, other examples, `scripts/*`) — record what you would
  change instead. Your ownership is `examples/open-library-sdk/` plus adding `"examples/open-library-sdk"` to the root `Cargo.toml`
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

Open Library work records serve `description` (and some author `bio` fields) either as a plain
string or as a text object `{"type": "/type/text", "value": "..."}` (Open Library data dumps /
works API; cite the page). Make the work lookup (and the author lookup, where the slice reads
`bio`) accept both shapes and expose one plain text value to callers, keeping an explicit "absent"
distinct from an empty string. Add the `sort` option of the search API (e.g. `new`, `old`,
`random`, `key`) to the works search, with the values the documentation lists.


## Rules

Make a real change to the SDK implementation or its caller contract, grounded in the provider's
documentation (cite it). Keep the existing consumer (`tests/first_use.rs`) and every existing
assertion; never weaken one. Add a focused regression test and, if the change adds behavior, a
consumer test for it. Then write `examples/open-library-sdk/MAINTENANCE.md`: what changed, every file/location you
had to edit and which facts had to be changed in several places, whether the compiler or the tests
caught an omitted edit, remaining generic duplication, provider-specific responsibilities, and
candidate Concord improvements. State prior exposure (what you read before starting).

## Report (final message)
Diff summary, locations touched, test results, findings.
