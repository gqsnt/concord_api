# GitHub SDK maintenance exercise (DX-01)

## Task statement

From `contracts/DATA_AND_VALIDATION.md` §6 (handoff directory) and
`v4/extensions-real-apis/MAINTENANCE_BRIEF.md`: a staged evolution of
`examples/github-sdk` (`concord-example-github-sdk`) by someone who did not write it —
**add an optional, provider-supported filter to a declared list operation and add a related
operation through the SDK's public extension/context pattern**. Evidence to retain: whether old
call sites compile unchanged, source edits, hover/docs discoverability and any duplicated
context/encoding. This is a deliberately staged change, not a claim that GitHub changed.

**Chosen change.**

- Filter: `type` on `issues/list-for-repo` (`GET /repos/{owner}/{repo}/issues`). GitHub's
  reference for REST API `2026-03-10` (retrieved 2026-11-05) documents it as "the name of an
  issue type", `*` for any type, `none` for issues without a type. The SDK listed it as
  deliberately unsupported (`PROVIDER.md`); every other parameter of that operation was already
  declared, and the other list operations' documented filters were too (only `page`, excluded on
  purpose, and the newer `issue_field_values` remained).
- Related operation: `issues/list-events` (`GET /repos/{owner}/{repo}/issues/{issue_number}/events`,
  `per_page`/`page`, `200` array of issue events, `410`), not declared by the SDK ("timeline and
  events" unsupported). It was added **from outside the SDK** in this crate
  (`src/issue_events.rs`), because that is what the SDK documents for "an operation this SDK does
  not declare" (`context` module: "another crate can add an operation ... without editing this
  SDK"; `PROVIDER.md` points at `RepoContext::request`). Concord's own recipe adds: "If you own
  the SDK, declare the endpoint" — that route was measured too, as an uncommitted probe (below).

## Commits

| Stage | Commit | Subject |
|---|---|---|
| 1 | `c21e1300f443cbae5cc0258ef84a44b3db8a070b` | maintenance github: frozen initial consumer |
| 2 | `7afc379faafa8a3989e8284ab6f01f49bff11131` | maintenance github: SDK change — issues type filter |
| 3 | `9c42c1a0829f2c6304f12515876933439e146f2e` | maintenance github: updated consumer |

The commit adding this file records the findings only (no code). `src/before.rs` and
`tests/before.rs` are byte-identical from stage 1 to the last commit
(`git diff c21e1300 HEAD -- checks/maintenance-github/src/before.rs checks/maintenance-github/tests/before.rs`
is empty).

## Results (Rust 1.99.1, `CARGO_BUILD_JOBS=6`, Linux x86_64, loopback only)

| Command | Stage 1 | After stage 2 | Final |
|---|---|---|---|
| `cargo test --locked -p concord-check-maintenance-github` | before: 5 passed | before: 5 passed (unchanged source) | before: 5 passed, after: 6 passed |
| `cargo test --locked -p concord-example-github-sdk` | 41 passed + 2 doctests (baseline) | 42 passed + 2 doctests | same as stage 2 |
| `cargo test --locked -p concord-example-github-app` | 21 passed (baseline) | 21 passed, **no app edit** | same |
| `cargo clippy --locked -p <each> --all-targets -- -D warnings`, `cargo fmt` | clean | clean | clean |

No live GitHub request was made (no credentials; the exercise is offline by design).

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

Documentation read: `examples/github-sdk/PROVIDER.md`; rustdoc of `lib.rs`, `context` (its
example shows exactly the `IssueFilter { .., ..IssueFilter::default() }` + `repo.issues(&filter)`
pattern), `filters`, `pages`, `paging`, `meta`, `error`, `params`, `models`;
`crates/concord-pagination/src/lib.rs` and the `sequential` example; `examples/testserver`.
Every question was answered by these.

Counter-evidence: the consumer compiled and its five tests passed on the first run, with no
compiler error, no type annotation, no cast and no native escape. The page-factory closure
`|key| pages.fetch(key)` needed no annotation.

Baseline note (not caused by this exercise): at the starting commit `e7bfd29`, `Cargo.lock`
lacked the five `concord-check-maintenance-*` stub packages, so every `cargo ... --locked` in the
workspace failed with "cannot update the lock file". Stage 1's `Cargo.lock` adds them as cargo
generates them (other maintenance branches will add the same five entries).

## Stage 2 — the SDK change

`git diff --stat c21e1300 7afc379f`:

```text
 examples/github-sdk/PROVIDER.md         | 10 +++++---
 examples/github-sdk/src/api.rs          |  3 +++
 examples/github-sdk/src/context.rs      |  2 ++
 examples/github-sdk/src/filters.rs      |  2 ++
 examples/github-sdk/tests/operations.rs | 44 ++++++++++++++++++++++++++++++++-
 5 files changed, 56 insertions(+), 5 deletions(-)
```

Source lines (7 added, 0 removed), in the three places the SDK repeats each optional filter:

```rust
// api.rs — declaration (1 line + 2 doc lines), after `assignee` as GitHub documents it
/// `type`: an issue type's name (`Bug`), `*` (issues with any type) or `none`
/// (issues without a type).
query("type") issue_type?: String,
// filters.rs — the record field (1 + 1 doc)
/// `type`: an issue type's name, `*` (any type) or `none` (no type).
pub issue_type: Option<String>,
// context.rs — the typed wrapper (2 lines): destructuring and mapping
issue_type,
.maybe_issue_type(issue_type)
```

`type` is a Rust keyword: `query("type") issue_type?: String` (AUTHOR_GUIDE §4.1, "Placing a
value") answered it at the first lookup. The value is a `String` like the existing `assignee` and
`milestone` filters, which use the same `name | * | none` convention; a typed
`enum { Named(String), Any, None }` would be clearer but inconsistent with the SDK.

Tests (1 line + 1 expected-target line adapted, 1 new test of 40 lines):
`every_operation_on_the_wire` now sets `issue_type` and expects
`...&assignee=*&type=Bug&creator=octocat...`; `issue_type_filter_on_the_wire` checks absence,
`Bug`, `Feature request` (`type=Feature+request`), `*`, `none` and the generated setter.

### Did old call sites compile unchanged?

| Call site | Result |
|---|---|
| Declaration edit alone (`api.rs`) | SDK, app and frozen consumer compiled; the record simply did not offer the filter yet (a declared parameter without a record field compiles silently). |
| Frozen consumer (`IssueFilter { .., ..IssueFilter::default() }` and generated setters) | **unchanged, compiled, 5/5 passed**, identical wire bytes. |
| `examples/github-app` (`..IssueFilter::default()` in its tests and `gh_a` binary) | **unchanged, 21/21 passed**. GH-A's `AuditInput.filter` can now carry `issue_type` with no app edit. |
| The SDK's own wrapper `RepoContext::issues` (exhaustive destructuring, deliberate) | broke, pointing at the right place (below). |
| The SDK's own test `tests/operations.rs` (exhaustive `IssueFilter { .. }` literal without `..`) | **broke**; any downstream crate written in that style breaks the same way. |

Compiler errors, verbatim except the elided bodies of E0027's three `help:` suggestions (both
pointed at the right cause):

```text
error[E0027]: pattern does not mention field `issue_type`
   --> examples/github-sdk/src/context.rs:327:13
    |
327 |           let IssueFilter {
    |  _____________^
...
338 | |         } = filter.clone();
    | |_________^ missing field `issue_type`
help: include the missing field in the pattern
help: if you don't care about this missing field, you can explicitly ignore it
help: or always ignore missing fields here
```

```text
error[E0063]: missing field `issue_type` in initializer of `IssueFilter`
  --> examples/github-sdk/tests/operations.rs:82:18
   |
82 |     let filter = IssueFilter {
   |                  ^^^^^^^^^^^ missing `issue_type`
```

The E0027 guard does its job only if the maintainer takes the first suggestion: the second and
third (`issue_type: _`, `..`) compile and silently drop the new filter from the record.

So adding a field to a public, non-`#[non_exhaustive]` record with public fields is a **source
break for exhaustive literals and patterns** (Rust semver: a minor-version break), although
AUTHOR_GUIDE §11 is right that the *generated* operation is unaffected ("Add an optional ...
value: calling code unchanged"). `#[non_exhaustive]` would not help: it forbids struct-literal
construction outside the crate even with `..Default::default()`, breaking every current caller.

### Wire bytes (omitted-field compatibility)

Frozen consumer, first page, identical before and after the change:
`/repos/octo-org/hello-world/issues?state=open&labels=bug&sort=updated&direction=desc&since=2026-10-01T00%3A00%3A00Z&per_page=2`.
Updated consumer with the filter:
`/repos/octo-org/hello-world/issues?state=open&type=Bug&sort=updated&direction=desc&since=2026-10-01T00%3A00%3A00Z&per_page=2`
— `type` takes its declared position (after `state`/`assignee`), not the end of the query.

### Hover / docs discoverability

The generated setters carry the declaration's doc comment plus Concord's placement note, so the
wire name is visible on hover although the Rust name differs (text extracted from the generated
rustdoc of `GitHubRepoIssues`, `cargo doc -p concord-example-github-sdk --no-deps`):

```text
pub fn set_issue_type(self, value: impl IntoValue<String>) -> Self
  `type`: an issue type's name (`Bug`), `*` (issues with any type) or `none` (issues without a type).
  Sets `issue_type` (present). Initially absent. Sent as: query parameter `type`.
pub fn maybe_issue_type(self, value: Option<String>) -> Self
  ... Sets whether `issue_type` is present: `Some(v)` is `set_issue_type(v)`; `None` makes it absent. ...
```

The record field's doc is handwritten and must be kept in step by hand. The SDK has no
maintainer note listing what to touch for a new filter (declaration, record, wrapper,
`PROVIDER.md`, wire test); the exhaustive destructuring found the wrapper, the compiler found the
test, `PROVIDER.md` had to be found by reading.

Documentation read in stage 2: AUTHOR_GUIDE §4.1 (key renaming), §4.7 (query order), §9
(generated names `set_x`/`maybe_x`), §11 (changing a declaration); GitHub's issues reference.

## Stage 3 — the updated consumer and the related operation

- `src/after.rs`: `typed_bug_trails` — the same digest, bugs selected by `issue_type: Some("Bug")`
  in the record, each with its events (bounded per bug); `newest_of_type` uses the generated
  `.set_issue_type(..)`. It reuses the frozen consumer's `BugSummary`.
- `src/issue_events.rs`: `issues/list-events` through `RepoContext::request`, with a page factory
  (`IssueEventPages::fetch(Option<EventsLink>)`) for `concord_pagination::sequential`.
- `tests/after.rs` (6 tests): filter and extension end to end with exact targets
  (`/repos/.../issues/21/events?per_page=100`, then GitHub's `/repositories/1300192/issues/21/events?per_page=100&page=2`),
  the extension's requests carrying exactly the declared operations' four service headers; per-bug
  event budget; `410` keeping status and request id, renderings without token or host; a foreign
  `rel="next"` refused and not requested; a continuation bound to its issue (mismatch sends
  nothing); two contexts (generated setter with `type=*` and extension, each with its own token
  and repository).

### What the context pattern gave, and what had to be repeated

Given by the SDK, reused unchanged: path prefix and segment encoding, the four headers (token
flagged sensitive), `ResponseMeta::capture`/`links()`, `GitHubContext::is_api_url`, `Binding`,
`GitHubError`, the `User` and `Timestamp` models. **No credential, version, header or path
encoding code was duplicated** — the context mirror (FRICTION.md F6) worked exactly as
documented.

Repeated in the extension (`src/issue_events.rs`, 231 lines, 164 non-comment):

| Part | Code lines | Duplicates |
|---|---|---|
| Event model (`IssueEventId`, `IssueEvent`, `EventLabel`) | 18 | nothing (needed on either route) |
| Continuation key `EventsLink` + redacting `Debug` | 18 | the SDK's `NextLink` (fields private, no public constructor) |
| Page factory, request building | 62 | `per_page` query encoding (6 lines, `query_pairs_mut`); binding check, origin check and URL replacement (13 lines) of `paging::fetch_page` |
| `send`: execute, transport error, metadata, decode | 25 | `meta::send_edited` (it takes a `Prepared`; a native request cannot become one) |
| `send`: `Link` → next key, origin check | 26 | `paging::fetch_page` |

About 80 lines repeat SDK logic that exists but is reachable only from a generated operation's
`Prepared` (`send_edited`, `LinkPages::new(.., prepare: fn(Op) -> Result<Prepared<R>, _>)`). The
`let Err(error) = decoder.finish(Err(e)).await else { unreachable!() }` idiom (FRICTION.md F2)
appears once more.

An honest misstep: the first draft (kept outside the repository) also had its own 91-line
`EventsError` mirroring `GitHubError`, because `GitHubError::api` is `pub(crate)` and I assumed
`#[non_exhaustive]` on the enum forbade building its variants elsewhere. A probe showed
`GitHubError::Api { error, meta: Some(Box::new(meta)) }` compiles outside the SDK (only
variant-level `#[non_exhaustive]` forbids that), so the extension now returns `GitHubError`.
Nothing in `error.rs` says extensions may construct the variants; the crate-private helper
suggests the opposite.

### Metadata the extension could not preserve

The extension's errors lose the endpoint label: `Decoder::new::<Json>()` has no name, so
`endpoint()` is `None`. Probe output (same `410`, same SDK error type):

```text
extension:        HTTP status 410 Gone
                  Api { error: Error { kind: Status, endpoint: None, status: 410, ... }, meta: Some(..) }
declared (probe): GET GitHub.repo.issue_events: HTTP status 410 Gone
```

Concord's recipe says to "name the operation in your own error type" for this reason; reusing
`GitHubError` trades that for one error type, and `after::TrailError::Events { number, .. }` adds
"events of #21" back. Also not preserved: the variant-specific fields of the ~26 event variants
(`event` stays text, only `label` is modelled), and the issue's returned `type` object (the SDK
does not model it, so `newest_of_type(repo, "*")` cannot say which type it found).

### The declared route (uncommitted probe, measured for comparison)

Declaring the same operation in the SDK, as its owner would, was applied, compiled and run, then
reverted (`git diff --stat` of the probe):

```text
 examples/github-sdk/src/api.rs    | 11 ++++++++++-   declaration (7 lines) + import
 examples/github-sdk/src/ids.rs    |  5 +++++         IssueEventId
 examples/github-sdk/src/models.rs | 31 +++++++++++++++++++++++++++++--   same model as above
 examples/github-sdk/src/pages.rs  | 25 ++++++++++++++++++++-----   issue_event_pages factory (15) + imports
 examples/github-sdk/src/paging.rs |  2 ++           operation::ISSUE_EVENTS
```

```rust
GET issue_events(number: IssueNumber, query per_page?: PerPage) -> Vec<IssueEvent> {
    path "issues/{number}/events";
}
// caller, identical in shape to every other child listing:
let pages = repo.issue_event_pages(IssueNumber(21), |op| op.set_per_page(PerPage::MAX));
let mut chain = sequential(Resume::Start, budget, |key| pages.fetch(key));
```

It passed the same two-page wire scenario and named the endpoint in its error. Besides the
model, the outside route costs about 145 non-comment lines (imports included) against about 25
for the declared route; the difference is the page factory and send/continuation logic above,
not context or encoding.

### Concepts the caller had to learn

Stage 3 consumer: none beyond stage 1 for the filter (`issue_type` in the record or
`set_issue_type`). The extension: native `reqwest::Request` from `RepoContext::request`;
`query_pairs_mut` (same form encoding as Concord, RECIPES); `Decoder::new::<Json>()` with
`decode`/`finish`; `ResponseMeta::capture` before decoding; `Link` continuation rules (follow as
sent, origin check, binding); `concord_pagination::Page { items, next, meta }`.

Casts / annotations / adapters: none needed (one `expect` on `PerPage::new(10)`, since `PerPage`
has only a `MAX` constant). Native escapes: one, the documented one (`RepoContext::request` +
`execute`). Hidden collection: none; `after` collects each bug's events into a `Vec`, bounded by
`event_pages`, visibly. Compiler errors met in stage 3: none in the committed code (one unused
parameter warning in a test helper, my mistake).

Documentation read in stage 3: RECIPES "An operation the SDK does not declare"; CALLER_GUIDE §7
(`Decoder::new`, `decode`, `finish`); SDK `context` rustdoc and `RepoContext::request` (they say
how to build, execute and decode one request); `tests/operations.rs::undeclared_operation_through_the_context`
(the only worked example: one unpaginated page, no metadata). Nothing in the SDK says how to
page an extension operation or which SDK pieces are reusable (`ResponseMeta::capture`,
`Binding`, `is_api_url`, `GitHubError` variants): that took reading `paging.rs`, `meta.rs` and
`error.rs` source.

## Counter-evidence (what was easy)

- The filter was a 7-line source change; the declaration accepted the keyword-named key at the
  first try; requests without it are byte-identical; the frozen consumer and the GitHub app
  needed no edit; hover shows `Sent as: query parameter type`.
- All three consumer/extension files compiled on the first attempt and all 11 tests passed on
  their first run.
- The context gave the extension the same headers as the declared operations with no copied
  credential or version code (asserted on the wire in every after-test).
- `concord_pagination::sequential` and `Page<T, K, M>` accepted the extension's own key and
  metadata types without adapters; bounded traversal and `Outcome::Bounded` came for free.

## Proposals (not implemented; no library code was edited)

1. **SDK, removes ~80 extension lines:** a public native send and continuation in the SDK, e.g.
   `meta::send_native<T>(&GitHubContext, reqwest::Request, Decoder<T>) -> Result<WithMeta<T>, GitHubError>`
   and `paging::NativePages` (or a public `NextLink` constructor that checks `is_api_url`).
   Before (this crate): `send` (51 lines) + `EventsLink` (18) + checks in `build` (13).
   After: `let page = paging::native_page(repo.context(), binding, request, decoder, key).await?;`.
2. **SDK, docs:** say in `error.rs` that extensions may construct `GitHubError` variants; list
   in `PROVIDER.md`/`context` what an extension can reuse; a short "adding a filter" note naming
   declaration, record, wrapper, `PROVIDER.md` and the wire test.
3. **Concord, small:** a named decoder, e.g. `Decoder::<T>::new::<Json>().named("GitHub.repo.issue_events")`,
   so native-route errors keep an endpoint label (today `endpoint()` is `None`). Before: errors
   render `HTTP status 410 Gone`; after: `GitHub.repo.issue_events: HTTP status 410 Gone`.
4. **Concord, larger (supports 1 generically):** the inverse of `into_parts`,
   `Prepared::from_parts(client, request, decoder)`, would let an SDK's `Prepared`-based helpers
   (`send_with_meta`, `LinkPages`) accept extension operations unchanged. It would weaken the
   "a `Prepared` came from a declaration" property, so it needs its own decision.
5. **SDK API choice for future fields:** a builder (`IssueFilter::new().state(..)`) or documenting
   `..IssueFilter::default()` as the only supported construction would make new filter fields
   non-breaking; today's exhaustive-literal break is legal but undocumented.

## Comparison with `examples/github-sdk/FRICTION.md` (read after stage 3)

- **Agree, F11** (filter records repeat the declaration; record field without mapping is a
  compile error, declared parameter without a field compiles silently): reproduced exactly. Not
  in F11: the record change also breaks exhaustive struct literals downstream (the SDK's own test
  did), and E0027's alternative suggestions can silently drop the filter.
- **Agree, F1/F2/F3:** the metadata plumbing, the `unreachable!` transport conversion and the
  "same endpoint at a provider URL" logic are needed again in every extension operation, not only
  once per SDK, because they are bound to `Prepared`. F1's proposed `Prepared::send_with_head`
  would not reach the native route; a `Decoder`-level equivalent would.
- **Agree, F6, with counter-evidence:** the context mirror is the SDK author's cost; for the
  extension author it worked with zero duplicated header or encoding code.
- **Agree, F12:** `sequential` with a borrowing closure and typed `Page` metadata worked without
  friction for the extension too.
- **Not in FRICTION.md:** the unlabeled decoder (proposal 3), and the missing guidance on what an
  extension may reuse (proposal 2).

## Fixtures and limits

Fixtures are `schema_derived_synthetic`, built inline in `tests/before.rs` and `tests/after.rs`:
issue objects with the pinned `issue` schema's fields the SDK reads (description commit and
version in `examples/github-sdk/PROVIDER.md`; after-fixtures add a `type` object, ignored by the
SDK), issue events with the fields every variant has per GitHub's issue-events reference (REST
API `2026-03-10`, retrieved 2026-11-05). Synthetic ids, logins, tokens and request ids.
Limits: offline only (no capture of a real GitHub response); the literal `event` names of the
issue-type events could not be confirmed from GitHub's event-types page, so the consumer relies
only on `labeled` and `reopened`, which it documents.
