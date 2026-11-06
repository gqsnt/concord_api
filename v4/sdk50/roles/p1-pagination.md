# P1 capsule: pagination — provider-error access and lossless item metadata/completion

You change `crates/concord-pagination` in the Concord Rust workspace (your working directory, a
git worktree on branch `sdk50/p1-pagination`). Read `AGENTS.md`/`CLAUDE.md` if present,
`crates/concord-pagination/src/{lib,pages,types,sequential}.rs` and its README/tests, and
`docs/RECIPES.md` (pagination). You cannot commit; leave the change in the working tree. Network
is enabled (loopback tests run); contact no external service. Keep the crate independent of
reqwest and every Concord crate (its `link` feature only adds `url`).

## Evidence (current source)

1. **Provider-error access.** `PagesError<E, K>` (`pages.rs`) exposes `outcome()`, `resume()`,
   `progress()` but no typed access to the page's own error `E`, so consumers match nested variants
   by hand: `examples/microsoft-graph-sdk` (its consumer boxes it and matches repeatedly),
   `examples/dropbox-sdk`, `examples/airtable-sdk`. Add the smallest API that gives the provider
   error by reference and by value (an owned extraction that keeps the resume/progress evidence
   available, e.g. `into_parts`/`into_provider_error`), without printing secrets through
   `Debug`/`Display`. Migrate the hand-written matches in Graph, Dropbox and Airtable where it
   removes code; keep every existing assertion unchanged.
2. **Item consumption loses page metadata and final completion evidence.** `Pages::items()` /
   `PageItems` / `ItemStream` yield items only; callers that need a page's metadata or the
   traversal's terminal value read pages instead or keep side channels: Google Drive's final
   `newStartPageToken` (`examples/google-drive-sdk/src/client.rs` changes listing,
   `ChangesPageMeta`), Microsoft Graph's final delta link (`src/paging.rs` `PageMeta::delta_link`),
   Linear's partial GraphQL errors in page metadata (`src/lib.rs` `PageMeta`), Slack's
   `UsersPageMeta`, GitHub app's request log (`examples/github-app/src/requests.rs`). Design the
   smallest opt-in addition that lets an item consumer also observe per-page metadata (without
   cloning metadata once per item) and obtain the last page's metadata / a completion value when
   the traversal ends cleanly — distinct from bounded/stopped/failed endings, which must stay
   explicit. A final delta token must not be presented as a next-page key. Do not attach
   metadata to every item; do not add global mutable state. Plain `items()` must stay unchanged
   for callers that do not need it.
3. Adopt (2) in at least Drive (final start-page token) and Graph (final delta link) where it
   removes their side-channel code; keep every existing assertion unchanged. List what Linear,
   Slack and GitHub app would change.

## Rules

- Do not break existing public APIs or semantics (`Outcome`, budgets, reports, `Resume`,
  `collect_complete`, cancellation-safe owned futures). Additive only.
- Never weaken a test or change an expected value. Add focused tests: provider error by
  reference/value with resume intact; item traversal observing page metadata; clean end yielding
  last-page metadata; bounded/failed endings never reporting a completion value; no per-item
  metadata clone (count clones with a counter type in a test).
- Update `crates/concord-pagination/README.md`, the crate rustdoc and `docs/RECIPES.md`.
- Bounded verification (`CARGO_BUILD_JOBS=4`, one cargo job at a time): the crate's tests
  (default and `--features link`), clippy `-D warnings`, rustdoc `-D warnings`, and the tests of
  every SDK/app you changed.

## Report (final message)

The API you added and why it is the smallest that serves the witnesses, lines removed/added per
migrated SDK, what stayed provider-specific, alternatives you rejected, test counts, and anything
you could not do.
