# Independent maintenance capsule — SDK45 arXiv

You maintain the SDK in `examples/arxiv-sdk` (package `concord-sdk-arxiv`), which you did not write, after its first
consumer. First attempt the ticket below **before** reading `FRICTION.md` or `FIRST_USE.md`.

## Provider slice: SDK45 · arXiv (small; public data)

Connected workflows (both required):
1. Search papers by term and inspect an entry.
2. Fetch a known list of arXiv IDs and read their metadata.

Contract notes: Public Atom XML; start/max_results pagination, namespaced extensions and usage limits.
Failure that must be exercised: Extension elements and missing optional fields must not invalidate the whole feed.
Excluded: Full-text PDF ingestion, harvesting bulk archives and author account features.
Official documentation (read-only research is allowed; cite the exact pages and the date you read
them):
- https://info.arxiv.org/help/api/user-manual.html
Live provider calls: **NOT_RUN** — never call the real service; every workflow runs against local
fixtures (`examples/testserver`: `TestServer::start(|request: &Recorded| -> Reply)`).

## Fixed rules

- Concord's product is its core, macros, declarations and generated usage; the hierarchy
  client → scope → endpoint is fixed. The frozen baseline is commit `c3fd4f1ee30c1727e26b285270f2cc8d8b694e77`: **do not edit
  any shared crate** (`crates/*`, `checks/*`, other examples, `scripts/*`) — record what you would
  change instead. Your ownership is `examples/arxiv-sdk/` plus adding `"examples/arxiv-sdk"` to the root `Cargo.toml`
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

Callers need precise searches: the arXiv API's `search_query` combines field prefixes (`ti`,
`au`, `abs`, `cat`, `all`, …) with the Boolean operators `AND`, `OR` and `ANDNOT` and grouping,
and results can be sorted with `sortBy` (`relevance`, `lastUpdatedDate`, `submittedDate`) and
`sortOrder` (`ascending`, `descending`). Read the user manual's query construction and sort
sections and cite them with the date:
- https://info.arxiv.org/help/api/user-manual.html

Add a typed query builder that a caller cannot make syntactically invalid (fields and operators
from the documented sets; terms escaped or quoted as the manual describes) and the sort options;
paging, pacing and the 30,000-result window behave as they do for term search. Keep term search
and fetch-by-ids unchanged for existing callers.


## Rules

Make a real change to the SDK implementation or its caller contract, grounded in the provider's
documentation (cite it). Keep the existing consumer (`tests/first_use.rs`) and every existing
assertion; never weaken one. Add a focused regression test and, if the change adds behavior, a
consumer test for it. Then write `examples/arxiv-sdk/MAINTENANCE.md`: what changed, every file/location you
had to edit and which facts had to be changed in several places, whether the compiler or the tests
caught an omitted edit, remaining generic duplication, provider-specific responsibilities, and
candidate Concord improvements. State prior exposure (what you read before starting).

## Report (final message)
Diff summary, locations touched, test results, findings.
