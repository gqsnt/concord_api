# GitHub SDK maintenance exercise (DX-01)

Status: stage 1 committed; stages 2 and 3 pending.

## Task statement

From `contracts/DATA_AND_VALIDATION.md` §6 (handoff directory) and
`v4/extensions-real-apis/MAINTENANCE_BRIEF.md`: a staged evolution of
`examples/github-sdk` (`concord-example-github-sdk`) by someone who did not write it —
**add an optional, provider-supported filter to a declared list operation and add a related
operation through the SDK's public extension/context pattern**. Evidence to retain: whether old
call sites compile unchanged, source edits, hover/docs discoverability and any duplicated
context/encoding. This is a deliberately staged change, not a claim that GitHub changed.

## Stage 1 — frozen initial consumer

Written before any SDK change, from the SDK's public documentation only.

- `src/before.rs`: a triage tool. `open_bug_digest` builds an `IssueFilter` (state, labels,
  since, sort, direction, per_page, `..IssueFilter::default()`), turns it into the generated
  operation with `RepoContext::issues`, pages it with `RepoContext::issue_pages` and
  `concord_pagination::sequential` under a page budget, skips pull requests
  (`IssueItem::PullRequest`), keeps request ids and the last `x-ratelimit-remaining`.
  `newest_bug` uses the generated setters directly (`repo.handle().issues().set_state(..)
  .set_labels(..)...send()`).
- `tests/before.rs` (5 tests): exact request targets of the first page and of the
  `Link`-provided page (GitHub addresses page 2 by repository id), the four service headers per
  request, two repositories with two tokens, page budget (`Outcome::Bounded`), a rate-limited
  `403` as a typed error whose renderings carry neither the token nor the query, and an
  unusable label refused before anything is sent.
- Fixtures: `schema_derived_synthetic`, built inline from the fields of the pinned `issue`
  schema that the SDK reads (description commit and version in the SDK's `PROVIDER.md`);
  synthetic ids, logins, tokens and request ids.

Documentation read for stage 1: `examples/github-sdk/PROVIDER.md`; rustdoc of `lib.rs`,
`context` (its example shows exactly the `IssueFilter { .., ..IssueFilter::default() }` +
`repo.issues(&filter)` pattern), `filters`, `pages` (example of a page factory), `paging`,
`meta`, `error`, `params`, `models`; `crates/concord-pagination/src/lib.rs` and the
`sequential` example; `examples/testserver` API. Every question was answered by these.

Counter-evidence (what was easy): the consumer compiled and its five tests passed on the first
run, with no compiler error, no type annotation, no cast and no native escape. The page factory
closure `|key| pages.fetch(key)` needed no annotation; `page.meta.response.request_id()` and
`rate_limit()` were found from the `PageInfo`/`ResponseMeta` rustdoc.

Baseline note (not caused by this exercise): at the starting commit `e7bfd29`, `Cargo.lock`
lacked the five `concord-check-maintenance-*` stub packages, so any `cargo ... --locked` in the
workspace failed with "cannot update the lock file". This commit's `Cargo.lock` adds them (as
cargo generates them) together with this crate's dependencies.
