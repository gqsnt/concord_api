# Independent maintenance capsule — SDK48 Elasticsearch

You maintain the SDK in `examples/elasticsearch-sdk` (package `concord-sdk-elasticsearch`), which you did not write, after its first
consumer. First attempt the ticket below **before** reading `FRICTION.md` or `FIRST_USE.md`.

## Provider slice: SDK48 · Elasticsearch (complex; streaming and bulk)

Connected workflows (both required):
1. Bulk-index a bounded set of documents, then inspect every per-item result.
2. Open a point-in-time view, search with search_after and close it.

Contract notes: Bulk uses NDJSON lines; a successful HTTP response may contain item errors; PIT and sort values drive search_after.
Failure that must be exercised: A partial bulk failure must not be treated as all-success or cause blind replay of successful operations.
Excluded: Cluster administration, every index option, scroll and unbounded memory buffering.
Official documentation (read-only research is allowed; cite the exact pages and the date you read
them):
- https://www.elastic.co/docs/api/doc/elasticsearch/operation/operation-bulk
- https://www.elastic.co/docs/reference/elasticsearch/rest-apis/paginate-search-results
Live provider calls: **NOT_RUN** — never call the real service; every workflow runs against local
fixtures (`examples/testserver`: `TestServer::start(|request: &Recorded| -> Reply)`).

## Fixed rules

- Concord's product is its core, macros, declarations and generated usage; the hierarchy
  client → scope → endpoint is fixed. The frozen baseline is commit `c3fd4f1ee30c1727e26b285270f2cc8d8b694e77`: **do not edit
  any shared crate** (`crates/*`, `checks/*`, other examples, `scripts/*`) — record what you would
  change instead. Your ownership is `examples/elasticsearch-sdk/` plus adding `"examples/elasticsearch-sdk"` to the root `Cargo.toml`
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

Callers need to read one document and update it safely: `GET /<index>/_doc/<id>` returns the
document with its `_seq_no` and `_primary_term`, and indexing with
`PUT /<index>/_doc/<id>?if_seq_no=…&if_primary_term=…` applies only if the document has not
changed since (otherwise `409` with a version-conflict error). Read the get and index document
references and the optimistic concurrency control page, and cite them with the date:
- https://www.elastic.co/docs/api/doc/elasticsearch/operation/operation-get
- https://www.elastic.co/docs/api/doc/elasticsearch/operation/operation-index
- https://www.elastic.co/docs/reference/elasticsearch/rest-apis/optimistic-concurrency-control

Add the read (a missing document is a typed "not found", not an error document) and the
conditional write (the caller passes the sequence number and primary term it read; a conflict is
a typed error the caller can act on; a timed-out write is uncertain and never resent). Keep bulk
and point-in-time search unchanged for existing callers.


## Rules

Make a real change to the SDK implementation or its caller contract, grounded in the provider's
documentation (cite it). Keep the existing consumer (`tests/first_use.rs`) and every existing
assertion; never weaken one. Add a focused regression test and, if the change adds behavior, a
consumer test for it. Then write `examples/elasticsearch-sdk/MAINTENANCE.md`: what changed, every file/location you
had to edit and which facts had to be changed in several places, whether the compiler or the tests
caught an omitted edit, remaining generic duplication, provider-specific responsibilities, and
candidate Concord improvements. State prior exposure (what you read before starting).

## Report (final message)
Diff summary, locations touched, test results, findings.
