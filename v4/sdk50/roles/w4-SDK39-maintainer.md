# Independent maintenance capsule — SDK39 MediaWiki Action API

You maintain the SDK in `examples/mediawiki-action-api-sdk` (package `concord-sdk-mediawiki-action-api`), which you did not write, after its first
consumer. First attempt the ticket below **before** reading `FRICTION.md` or `FIRST_USE.md`.

## Provider slice: SDK39 · MediaWiki Action API (small; public data)

Connected workflows (both required):
1. Search pages and fetch the chosen page’s content.
2. List category members and follow every returned continuation parameter.

Contract notes: Public query JSON; the continue object may contain multiple fields which all must be carried forward.
Failure that must be exercised: Dropping the generic continue field can repeat pages or silently truncate traversal.
Excluded: Authenticated edits, Wikibase, parsing all wikitext and global cross-wiki support.
Official documentation (read-only research is allowed; cite the exact pages and the date you read
them):
- https://www.mediawiki.org/wiki/API:Continue/en
- https://www.mediawiki.org/wiki/API:Query
Live provider calls: **NOT_RUN** — never call the real service; every workflow runs against local
fixtures (`examples/testserver`: `TestServer::start(|request: &Recorded| -> Reply)`).

## Fixed rules

- Concord's product is its core, macros, declarations and generated usage; the hierarchy
  client → scope → endpoint is fixed. The frozen baseline is commit `c3fd4f1ee30c1727e26b285270f2cc8d8b694e77`: **do not edit
  any shared crate** (`crates/*`, `checks/*`, other examples, `scripts/*`) — record what you would
  change instead. Your ownership is `examples/mediawiki-action-api-sdk/` plus adding `"examples/mediawiki-action-api-sdk"` to the root `Cargo.toml`
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

Callers need the content of several pages in one request. The Query module accepts several page
ids (`pageids=1|2|3`, up to the documented limit for the caller's rights) with
`prop=revisions&rvprop=content&rvslots=main`; the response lists each page, including pages that
are missing or invalid, and a large result can itself be continued. Read API:Query and
API:Revisions and cite them with the date:
- https://www.mediawiki.org/wiki/API:Query
- https://www.mediawiki.org/wiki/API:Revisions

Add a batch content fetch: the caller passes up to the documented limit of page ids and gets, per
requested id, either the page's content or a typed "unavailable" reason (missing, invalid, no
content), in the caller's order; a continued response is followed until every requested page is
answered. Keep search, single-page content and category listing unchanged for existing callers.


## Rules

Make a real change to the SDK implementation or its caller contract, grounded in the provider's
documentation (cite it). Keep the existing consumer (`tests/first_use.rs`) and every existing
assertion; never weaken one. Add a focused regression test and, if the change adds behavior, a
consumer test for it. Then write `examples/mediawiki-action-api-sdk/MAINTENANCE.md`: what changed, every file/location you
had to edit and which facts had to be changed in several places, whether the compiler or the tests
caught an omitted edit, remaining generic duplication, provider-specific responsibilities, and
candidate Concord improvements. State prior exposure (what you read before starting).

## Report (final message)
Diff summary, locations touched, test results, findings.
