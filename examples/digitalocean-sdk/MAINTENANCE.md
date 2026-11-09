# Maintenance — tag-filtered droplets, droplet snapshots, resumable traversals

Current API note (P4.2): this evidence records the earlier API. Paging `_from` twins now use the base factory followed by `.starting_at(Resume::At(key))`; Datadog/Notion traversal cycles use `PagesError::Repeated`. See [migration](../../docs/MIGRATION.md#runtime-operations-and-paging-p42).

Role: independent maintainer, after the first consumer (`tests/first_use.rs`). The SDK
author and the consumer were other roles. Date: 2026-11-07. Baseline:
`c3fd4f1ee30c1727e26b285270f2cc8d8b694e77` plus the author's and consumer's commits
(`82e601e3`, `42f5d8d0`). Live provider calls: **NOT_RUN**. Every new behavior runs
against `concord-example-testserver` on loopback. No shared crate, check, script or other
example was edited, and nothing was committed.

## Prior exposure

Before starting, I read the role brief and the harness instructions (global and project
`CLAUDE.md`). The memory index listed in the session context was shown but not opened.
Then, inside the worktree and before writing code: this package's `README.md`,
`PROVENANCE.md`, `Cargo.toml`, every `src/` file, every test file (including the consumer's
`first_use.rs`) and the existing fixtures. I also read the concord-pagination README,
`pages.rs` and `types.rs` (`Pages::new`, `PagesError`, `Resume`), Concord's query assembly
(`__private.rs`, `append_pair`), `Prepared::follow`, `IntoValue`, and parts of
`docs/CHEATSHEET.md` and `docs/AUTHOR_GUIDE.md` (§2.1, §4.1, §4.7, §6, §11).
`FRICTION.md` and `FIRST_USE.md` were read **after** the change, its tests and the mutation
checks were done. Their findings are reconciled below. The only external pages were the two
permitted DigitalOcean pages, read 2026-11-07 (exact facts in PROVENANCE.md's addendum).
I also have general prior knowledge of DigitalOcean's API from training. Where that knowledge
goes beyond the permitted pages (tag-name character rules), it was **not** used for an
enforced rule.

## Provider grounding

From https://docs.digitalocean.com/products/droplets/reference/api/droplets/ (read 2026-11-07):

- "Filtering Results by Tag": `GET /v2/droplets?tag_name=$TAG_NAME`. `tag_name` is an
  optional string (example `env:prod`). It cannot be combined with `name` or `type`, and it
  requires the `tag:read` scope.
- "List Snapshots for a Droplet": `GET /v2/droplets/{droplet_id}/snapshots`, scope
  `droplet:read`, with the same `page`/`per_page` parameters. The response has `snapshots`,
  optional `links` (with the same `pages` forward/backward links) and `meta`. A snapshot has
  `id`, `created_at`, `min_disk_size`, `name`, `regions`, `size_gigabytes` (float) and `type`
  (`snapshot` or `backup`).

From https://docs.digitalocean.com/reference/api/reference/public-apis/ (read 2026-11-07):
`next` is "the URI of the next sequential page of results". Neither page says whether a
filtered listing's links repeat `tag_name`. That is the main open provider fact; see
"Decisions".

## What changed for callers

| New | Use |
| --- | --- |
| `TagName::new(&str) -> Result<TagName, Error>` | Validated before any request: empty, whitespace or control characters → `Error::TagName` |
| `api.droplets().list().set_tag_name(tag)` | Generated optional query on the existing list endpoint; `GET /v2/droplets?tag_name=env%3Aprod` |
| `api.droplet(id).snapshots()` | New generated endpoint `DigitalOcean.droplet.snapshots`, with `set_page`/`set_per_page` |
| `Snapshot`, `Snapshots` | Models; `kind` is the JSON `type`, kept as an open string |
| `tagged_droplet_pages(&tag, size, cap)` | Traversal; the tag must be on every followed or resumed link |
| `snapshot_pages(id, size, cap)` | Traversal of one droplet's snapshots and backups |
| `droplet_pages_from`, `tagged_droplet_pages_from`, `action_pages_from`, `snapshot_pages_from` | The same traversal started at a `Resume<NextLink>`, normally `PagesError::resume()` |

Existing calls keep their signatures, wire and outcomes. `droplet_pages`/`action_pages` now
delegate to their `_from` variant with `Resume::Start`, which is the same code path.
`first_use.rs` is byte-identical to its committed version, and its 19 tests pass. No
existing assertion was edited. The existing test files changed only in their `use` lines;
the new tests were appended.

### Two compatibility facts the owner should know

1. **Tightened link rule for the unfiltered listings.** A next link (or resumed key) must
   carry exactly the traversal's `tag_name` values. For `droplet_pages`, `action_pages` and
   `snapshot_pages` that means *none*. A provider link that *adds* a `tag_name` to an
   unfiltered listing was followed before. It is now `UnsafeNext`. Without this,
   a tagged key passed to `droplet_pages_from` would silently turn "all droplets" into "tagged
   droplets". Nothing documented or tested relied on the old acceptance. To revert, make the
   comparison one-way (only keys the template carries). Mutation M9 below shows the three tests
   that pin the current rule.
2. **`Error` gained a variant (`TagName`).** `Error` is not `#[non_exhaustive]`, so a caller
   with an exhaustive `match` on it stops compiling (`first_use.rs` uses wildcards). This
   follows the existing per-input error pattern (`PageNumber`, `PageSize`). The alternative,
   a separate error type for `TagName::new`, would break that pattern.

## Decisions

- **Tag on the existing endpoint, not a new one.** The provider documents `tag_name` as an
  optional filter of `GET /v2/droplets`, so it is `query tag_name?: TagName` on
  `droplets.list`. That is additive (AUTHOR_GUIDE §11), and `TagName` is `Clone`, so the request stays
  `Clone` for the pagination template. The operation name stays `DigitalOcean.droplets.list`.
- **Fail closed when a link loses the tag.** The SDK has no URL rewriting, and the
  public pages do not promise that links repeat filters. So a link that drops, changes,
  repeats or renames the tag is refused (`UnsafeNext`, nothing sent) rather than "repaired".
  Following it would widen a tagged listing to every droplet in the account. Values are
  compared after query decoding, so `env%3Aprod` and `env:prod` are equal, while `team+ops`
  (a space after decoding) is not equal to `team%2Bops`. **Unverified:** if the real provider
  drops `tag_name` from its links, multi-page tagged listings will fail with `UnsafeNext` on
  page 1. That needs a live check, which this capsule forbids.
- **Minimal tag validation.** Empty is refused because its provider meaning is undocumented
  and could list everything. Whitespace and control characters are refused as an SDK choice:
  they mark untrimmed or corrupted input. No character set or length limit is enforced,
  because those rules are not on the permitted pages. Everything else is percent-encoded by
  Concord (`append_pair`); a test shows `team+ops&tag_name=x#y` stays one value.
- **Resume through `*_from` methods taking `Resume<NextLink>`.** concord-pagination documents
  resuming as "pass it as `from` to a new traversal over the same listing". The SDK had no
  such entry, so the ticket's "resumed one" needed one. To keep the "same resume rules",
  all four traversals get one. The key is revalidated before any request: origin, exact
  collection path (so another droplet's key is refused) and the tag rule. `size` applies
  only to `Resume::Start`; a resumed link keeps its own `per_page`.
- **Snapshot model tolerance matches the other listings.** `meta` is documented as required
  but modeled `Option`, like `Droplets`/`Actions`. The traversal never reads totals.
  Snapshot item fields are required, as documented.

## Every location edited

| File | Edit |
| --- | --- |
| `src/lib.rs` | Declaration: `query tag_name?: TagName` on `droplets.list`; `GET snapshots(..Paging) -> Snapshots { path "snapshots"; }` in `scope droplet(id)`; scope doc. New `TagName` (validation, `as_str`, `Display` = wire spelling). |
| `src/error.rs` | `Error::TagName` and its `Display` arm; `UnsafeNext` doc names the tag filter. |
| `src/models.rs` | `Snapshot`, `Snapshots`. |
| `src/pagination.rs` | `FILTERS = ["tag_name"]`; `validate` compares decoded `tag_name` values both ways (`values` helper); the `pages` driver takes `from: Resume<NextLink>`; `Listing for Snapshots`; six new public methods. |
| `tests/support/mod.rs` | Four fixture routes and `prod()`. |
| `tests/workflows.rs` | Seven regression tests (connected tagged→snapshots workflow; tag validation; link tag-variants; unfiltered listings refuse an added tag; resumed tag kept and foreign keys refused unsent; failed page retried from its position; snapshot completion/cap/resume/trust). |
| `tests/protocol.rs` | Three tests: new request shapes and production `prepare()` URL; every documented HTTP class on the new endpoints with no retries; snapshot body-failure stages. |
| `tests/consumer_tags_snapshots.rs` | New consumer application (7 tests): a nightly backup audit over tagged droplets with budgeted, resumed runs, plus its failure cases. |
| `fixtures/` | `tagged-first.json`, `tagged-last.json`, `snapshots-first.json`, `snapshots-last.json`. |
| `README.md` | Route and traversal tables, tag rules and scope, link and resume rules, second compiled example, scope list. |
| `PROVENANCE.md` | Maintenance addendum (pages, fixtures, reading, lock). |

Not edited: `Cargo.toml`, the root `Cargo.toml` and `Cargo.lock` (already a member, no new
dependency), `first_use.rs`, `FRICTION.md`, `FIRST_USE.md`.

### Facts that had to be written in several places

- **The filter key `tag_name`:** in the declaration (`query tag_name?`) and again in
  `FILTERS` (pagination.rs), then in README/tests. Nothing ties them together. If the
  declared key were renamed, `FILTERS` would keep checking a key that is never sent, and the
  tag guarantee would silently lapse. Only the wire-target tests would notice.
- **One listing = four pieces:** the endpoint (declaration), the envelope model (`Snapshots`
  with `links`/`meta`, a third copy of that shape), the `Listing` impl (which field holds the
  records), and two public traversal methods (start and `_from`). The `Listing` impl is
  checked by the compiler; the envelope key (`snapshots`) only by decoding tests.
- **Every traversal's argument list twice:** once on `X_pages`, once on `X_pages_from`.
- **The collection path:** in the declaration only. The validator derives it from the
  prepared template (`prepared.request().url()`), so no duplication. Page parameters remain
  in one `Paging` group.

## Did the compiler or the tests catch omitted edits?

One omission happened for real: the `Display` arm for `Error::TagName` was left out at first.
The compiler caught it (`E0004 non-exhaustive patterns: &Error::TagName not covered`). The
rest were staged afterwards on copies (originals under `target/tmp/maint-mutation/backup`,
restored and compared with `cmp` after each run). Each row is one
`cargo test --locked --offline -p concord-sdk-digitalocean --no-fail-fast`:

| Mutation | Compiler | Failing tests (pre-existing suites: none in every row) |
| --- | --- | --- |
| M1 remove the `FILTERS` comparison | — | 5 (2 consumer, 3 regression) |
| M2b forget `.set_tag_name(tag)` | warning `unused variable: tag` only (fatal under clippy `-D warnings`) | 9 |
| M4 forget `impl Listing for Snapshots` | `E0277 Snapshots: Listing` | build fails |
| M5b hard-code `Resume::Start` in the driver | warning `unused variable: from` only | 6 |
| M6 drop revalidation of a resumed key | — | 3 (same-origin foreign keys would be **sent**; `follow` still refuses other origins) |
| M7 accept an empty tag | — | 2 |
| M8 compare raw instead of decoded query values | — | 1 (`next_link_must_keep_exactly_the_traversal_tag`, "unencoded" case) |
| M9 check only keys the template carries | — | 3 |
| M10 `TagName` without `Clone` | `E0277 &TagName is not accepted…` and `DigitalOceanDropletsList: Clone` | build fails |

The 19 consumer tests and the author's 17 original tests caught none of the behavior
mutations. They do not exercise tags, snapshots or resume, and before this change there was
no resume entry for M6 to break. M8 is guarded by a single case.

## Remaining generic duplication

- Per-listing boilerplate (envelope struct, `Listing` impl, two methods); 4 listings → 8
  public methods of one to three lines.
- The `_from` twin of every traversal exists only because a `Pages` cannot be told where to
  start after construction.
- The follow-and-revalidate driver and URL checks (FRICTION F1/F4). The plan-identity
  comparison is generic: compare selected decoded query values of a followed URL with the
  template's. It is hand-written here.

## Provider-specific responsibilities (stay in this SDK)

Which query keys define the listing (`tag_name`); collection-path trust; the envelope keys
(`droplets`, `actions`, `snapshots`) and `links.pages.next`; the fail-closed choice when a
link loses the tag; the safety-minimum tag validation; the documented `tag:read` scope note;
`kind` as an open string; the tolerant `meta`.

## Reconciliation with FRICTION.md and FIRST_USE.md (read afterwards)

- FIRST_USE M1 (the page carrying an unsafe link fails whole, and its retry position is that
  same page) applies unchanged to the new traversals, as "same untrusted-link rules"
  requires. `first_use.rs` pins it (`collected.is_empty()`, `failed: Resume::Start`). Resume
  makes it more visible: resuming after `UnsafeNext` refetches a page that fails again.
  The README now says so, and a regression asserts `failed: Resume::Start` for refused tag
  links. Not changed: changing it would alter pinned existing outcomes.
- FIRST_USE incidents 4–6 (traversals on the client rather than the scope, a required
  `PageSize`, naming `Incomplete<T, Error, NextLink>`) also apply to the additions, which follow
  the existing shape. The consumer test had to name `Incomplete<Snapshot, Error, NextLink>`,
  and `NextLink` is now caller-relevant, because it is the resume key.
- FRICTION F1/F4 (hand-written follow driver and link checks) grew by the filter rule.

## Candidate Concord improvements (recorded, not made)

1. **`Pages` start position before the first read** (concord-pagination): for example
   `Pages::starting_at(self, Resume<K>)`, valid like `max_pages` until the first read. SDKs
   could then expose one method per traversal (`api.tagged_droplet_pages(..).starting_at(from)`),
   removing the four `_from` twins here.
2. **Plan-preserving follow** (Concord core or pagination adapter): declaration-level
   marking of query values that define a listing (for example a `params` group attribute),
   with a generated check that a followed URL repeats them exactly. This removes the
   `tag_name` duplication between the declaration and `FILTERS`.
3. **Page with a refused continuation** (concord-pagination): let a page be emitted with a
   continuation error, so the prefix keeps its records and `failed` names the refused link
   rather than a page that fails again (FIRST_USE M1). A behavior change for existing callers.
   Owner decision.
4. **Author guide note on SDK error enums**: §11 covers input records. Adding a variant
   to a public SDK `Error` without `#[non_exhaustive]` is the same kind of break.

SDK-local follow-ups, not done (scope): a persistable resume key (the URL is revalidated on
resume anyway); name/GPU filters; `Clone`/`PartialEq` on models (FIRST_USE 9).

## Validation (2026-11-07, Rust 1.99.1, `CARGO_TARGET_DIR=<worktree>/target`, `CARGO_BUILD_JOBS=4`)

| Command | Result |
| --- | --- |
| baseline `cargo test --locked --offline -p concord-sdk-digitalocean` (before edits) | 19 first_use + 11 protocol + 6 workflows + 1 doctest passed |
| `cargo test --locked --offline -p concord-sdk-digitalocean` (after) | 7 consumer + 19 first_use + 14 protocol + 13 workflows + 2 doctests passed |
| `cargo clippy --locked --offline -p concord-sdk-digitalocean --all-targets -- -D warnings` | clean |
| `RUSTDOCFLAGS="-D warnings" cargo doc --locked --offline -p concord-sdk-digitalocean --no-deps` | clean |
| `cargo fmt -p concord-sdk-digitalocean -- --check`; `git diff --check` | clean |
| `git diff --quiet HEAD -- Cargo.lock Cargo.toml`; `git diff --quiet HEAD -- examples/digitalocean-sdk/tests/first_use.rs` | both unchanged |

All of this is fixture evidence of the SDK's contract. It is not evidence of the provider's
current behavior: in particular, whether real tagged next links repeat `tag_name` is unknown.
