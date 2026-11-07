# Independent maintenance capsule — SDK38 Mastodon

You maintain the SDK in `examples/mastodon-sdk` (package `concord-sdk-mastodon`), which you did not write, after its first
consumer. First attempt the ticket below **before** reading `FRICTION.md` or `FIRST_USE.md`.

## Provider slice: SDK38 · Mastodon (medium; social and media)

Connected workflows (both required):
1. Read a public timeline then retrieve an individual status.
2. Discover the streaming host and consume the public timeline as SSE.

Contract notes: Instance-dependent host/configuration and access rules; SSE events, comments and heartbeats.
Failure that must be exercised: A UTF-8 character or SSE data field may span chunks; a disconnect must be surfaced without invented replay guarantees.
Excluded: Posting, WebSocket, federation semantics and guaranteed exactly-once events.
Official documentation (read-only research is allowed; cite the exact pages and the date you read
them):
- https://docs.joinmastodon.org/methods/timelines/
- https://docs.joinmastodon.org/methods/streaming/
Live provider calls: **NOT_RUN** — never call the real service; every workflow runs against local
fixtures (`examples/testserver`: `TestServer::start(|request: &Recorded| -> Reply)`).

## Fixed rules

- Concord's product is its core, macros, declarations and generated usage; the hierarchy
  client → scope → endpoint is fixed. The frozen baseline is commit `c3fd4f1ee30c1727e26b285270f2cc8d8b694e77`: **do not edit
  any shared crate** (`crates/*`, `checks/*`, other examples, `scripts/*`) — record what you would
  change instead. Your ownership is `examples/mastodon-sdk/` plus adding `"examples/mastodon-sdk"` to the root `Cargo.toml`
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

Callers need hashtag timelines. Mastodon documents `GET /api/v1/timelines/tag/:hashtag` (with
`any`/`all`/`none` extra tags, `local`, `remote`, `only_media`, `max_id`/`since_id`/`min_id`,
`limit`) and a hashtag stream on the streaming API (`stream=hashtag&tag=…`, and `hashtag:local`).
Read the timelines and streaming pages and cite them with the date:
- https://docs.joinmastodon.org/methods/timelines/
- https://docs.joinmastodon.org/methods/streaming/

Add the hashtag timeline (the hashtag is validated before any request and path-encoded as one
segment) and the hashtag stream (same instance and streaming-host binding, same framing and
terminal-disconnect rules as the public stream, no replay). Keep the public timeline, status
retrieval and the public stream unchanged for existing callers.


## Rules

Make a real change to the SDK implementation or its caller contract, grounded in the provider's
documentation (cite it). Keep the existing consumer (`tests/first_use.rs`) and every existing
assertion; never weaken one. Add a focused regression test and, if the change adds behavior, a
consumer test for it. Then write `examples/mastodon-sdk/MAINTENANCE.md`: what changed, every file/location you
had to edit and which facts had to be changed in several places, whether the compiler or the tests
caught an omitted edit, remaining generic duplication, provider-specific responsibilities, and
candidate Concord improvements. State prior exposure (what you read before starting).

## Report (final message)
Diff summary, locations touched, test results, findings.
