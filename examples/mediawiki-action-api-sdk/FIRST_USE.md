# FIRST_USE — fresh consumer of `concord-sdk-mediawiki-action-api` (SDK39)

Historical author notes. P4.3 now fixes required handle values and declares checked runtime bases; see the current README and `docs/MIGRATION.md`.

Role: fresh consumer (Claude, Agent-tool subagent). Requested/observed model and effort are
recorded by the coordinator from harness metadata, not here. Date: 2026-11-07.
Live provider calls: **NOT_RUN** — every call in `tests/first_use.rs` goes to
`examples/testserver` on loopback.

## Material read before writing any code (in order)

1. `examples/mediawiki-action-api-sdk/README.md` and `Cargo.toml` (the package manifest: to
   see which dependencies a test can use).
2. Rustdoc generated with
   `cargo doc --locked --offline -p concord-sdk-mediawiki-action-api -p concord-example-testserver --no-deps`,
   converted to text with a throwaway HTML stripper (`target/tmp/tools/h2t.py`): crate index,
   `MediaWiki`, `MediaWikiConfig`, `MediaWikiQuery`, `MediaWikiQuerySearch`,
   `MediaWikiQueryContent`, `MediaWikiQueryCategoryMembers`, `Continuation`, `CategoryPages`,
   `Error`, `HttpFailure`, `ApiProblem`, `ErrorDocument`, `QueryResponse`, `Envelope`,
   `SearchResults`, `CategoryResults`, `ContentResults`, `PageContent`, `PageIdentity`,
   `QueryPolicy`, `BODY_LIMIT`; testserver `TestServer`, `Recorded`, `Reply`, `Body`, `Script`.
3. Because `CategoryPages` is only a type alias, rustdoc of `concord-pagination`
   (`Pages`, `PagesError`, `Resume`, `Incomplete`, `Page`, `PageItems`, `Progress`) and of
   `concord` (`ResponseMeta`, `Error`, `ErrorKind`, `Operation`, `Prepared`), generated with
   `cargo doc --locked --offline -p concord-pagination -p concord -p concord-core --no-deps`.
4. Official documentation, fetched once each with `curl` on 2026-11-07 (06:25 UTC) and saved
   under `target/tmp/docs/`:
   - <https://www.mediawiki.org/wiki/API:Continue/en> — revision `oldid=8588859`
     ("last edited on 3 September 2026").
   - <https://www.mediawiki.org/wiki/API:Query> — revision `oldid=7933095`
     ("last edited on 12 October 2025").
   Facts taken from them: the `continue` object has a generic `continue` sub-field (value
   like `-||` or `||categories`) plus module fields (`cmcontinue`, `accontinue`, `plcontinue`,
   …); the documented client loop merges the **whole** returned `continue` object into the
   next request and stops only when a result has no `continue`; values are opaque and must
   not be interpreted; `batchcomplete` is about per-batch property data, not traversal end;
   `error` / `warnings` can appear in a 2xx result.

Not read: the SDK's `src/`, `tests/`, `fixtures/`, `FRICTION.md`, `PROVENANCE.md`,
`VALIDATION.md`; no other campaign file.

## Predictions

Written before `tests/first_use.rs` existed. Setup I expect for both workflows:

```rust
let http = reqwest::Client::builder().redirect(Policy::none()).retry(retry::never()).timeout(..).build()?;
let api = concord::BaseUrl::parse((format!("{}/w/api.php", server.url())).as_ref()).map(|origin| MediaWiki::new(http, MediaWikiConfig { origin: origin, user_agent: "...".into() }))?;
let query = api.query();
```

`with_base_url` keeps the `/w/api.php` path; every request is `GET /w/api.php?...` to that
host. Expected headers on every request: `user-agent: <exact configured value>`,
`accept: application/json` (from the Json format's `Decode::ACCEPT`), `host`; no cookie,
no authorization, no `content-type`, no body.

### Workflow 1 — search, choose a hit, fetch its content

Calls:

1. `query.search("General relativity").set_srlimit(3).send().await` →
   `Result<QueryResponse<SearchResults>, Error>`.
   - Wire: `GET /w/api.php?action=query&format=json&formatversion=2&srsearch=General+relativity&srlimit=3&list=search&srprop=`
     — parameters in the rustdoc's declared order (`action, format, formatversion, srsearch,
     srlimit, list, srprop`); I guess `srprop` is sent **empty** (only `ns/title/pageid` are
     modelled). Without `set_srlimit` it sends `srlimit=10`.
   - Return: `data.search: Vec<PageIdentity { pageid, ns, title }>` in provider order;
     `continuation: Some(..)` when the body has `"continue": {"sroffset": 3, "continue": "-||"}`
     (numeric `sroffset` accepted and later sent as the text `3`); `None` when the body has no
     `continue`; `meta.status() == 200`, `meta.operation()` names `MediaWiki.query.search`.
   - Next search page: `.set_continuation(next)` appends `continue=-||&sroffset=3` after the
     declared parameters (I guess `BTreeMap` order: `continue` before `sroffset`).
2. The caller chooses a hit (I will choose the **second**, not the first) and calls
   `query.fetch_page(hit.pageid).await` → `Result<PageContent, Error>`.
   - Wire: `GET /w/api.php?action=query&format=json&formatversion=2&pageids=<id>&prop=revisions&rvprop=ids|content&rvslots=main&rvlimit=1`
     (guessed values for `prop`/`rvprop`/`rvslots`/`rvlimit`; only the names are documented).
   - Return: `PageContent { pageid, title, revision, content }` from
     `query.pages[0].revisions[0].slots.main.content` and `revid`.

Failure predictions (both calls):

| Provider behaviour | Predicted SDK result |
|---|---|
| HTTP 503 / 429 with `{"error":{...}}` JSON | `Error::Http(HttpFailure { meta: status 503/429 + headers incl. `retry-after`, document: Ok(ErrorDocument { error: ApiProblem { code, info } }) })` |
| HTTP 5xx with an HTML body | `Error::Http` with `document: Err(concord error, kind Metadata)` |
| HTTP 200 `{"error":{"code":"badvalue",...}}` | `Error::Api { meta: 200, problem: ApiProblem { code: "badvalue", .. } }` |
| HTTP 200 with a non-null `warnings` | `Error::Warnings(meta)` — even if `query` is present |
| HTTP 200 with no `query` | `Error::InvalidResponse(meta)` |
| HTTP 200 truncated / malformed JSON | `Error::Concord` with `kind() == ErrorKind::Decode` and `head()` present |
| HTTP 200 `text/html` | `Error::Concord` with `kind() == ErrorKind::Metadata` |
| body > 1 MiB | `Error::Concord` with `kind() == ErrorKind::Limit` |
| content: page `missing: true` / no revisions / no main slot / hidden content | `Error::ContentUnavailable` (no meta attached) |
| content: two pages for one ID, or a `continue` in the content response | `Error::InvalidResponse` |
| empty content `""` | `Ok(PageContent { content: "" })` |

Secrets: there are none in this public-data client. The User-Agent is marked sensitive and
should be absent from `Debug` of the client and of errors; `ApiProblem.info` is withheld
from error `Debug`/`Display`; `Continuation` `Debug` prints only a field count.

### Workflow 2 — list category members and follow every continuation field

Call: `query.category_pages("Category:Physics", 2, None, NonZeroUsize::new(10).unwrap())`
→ `CategoryPages` (= `Pages<PageIdentity, Continuation, (), Error>`), then either
`.items().collect_complete().await` → `Result<Vec<PageIdentity>, Incomplete<PageIdentity, Error, Continuation>>`
or `next_page()` in a loop.

- Wire, first page: `GET /w/api.php?action=query&format=json&formatversion=2&cmtitle=Category:Physics&cmlimit=2&list=categorymembers`.
- Wire, each later page: the same parameters, then **every** field of the previous
  response's `continue` object exactly once, e.g. `cmcontinue=page|...|123&continue=-||`
  (alphabetical; the generic `continue` is never dropped). An unknown extra field in the
  `continue` object would also be forwarded verbatim.
- Return: all members in page order, only when a response has no `continue` object. An
  empty `categorymembers` page **with** `continue` advances; `batchcomplete: true` with a
  `continue` does not stop. One HTTP request per page; no read-ahead.
- Bound: with `max_pages = 2` and three provider pages, `collect_complete()` returns
  `Err(Incomplete { collected: <first two pages' members>, error: PagesError::Bounded { resume: Resume::At(<the 2nd response's continuation>) } })`
  after exactly 2 requests. Resuming with
  `category_pages(same title, same size, error.resume().and_then(|r| r.key()).cloned(), ..)`
  sends that continuation on its first request and finishes the listing.
- Page failure (HTTP 503 on page 2): `Incomplete { collected: page 1, error: PagesError::Provider { error: Error::Http(..), failed: Resume::At(<page-1 continuation>) } }`;
  `next_page()` afterwards returns `PagesError::Ended`, never `Ok(None)`.
- A `continue` object lacking the generic `continue` field (or with an empty/null value, or a
  field colliding with `cmtitle`/`action`) is refused rather than followed: I predict
  `PagesError::Provider { error: Error::Concord(kind Decode) }` because the token is decoded
  with the envelope.
- Metadata: `CategoryPages` has `M = ()`, so per-page `ResponseMeta` is **not** available
  through the traversal; for headers of a category page I would have to call
  `category_members(..).send()` directly (`QueryResponse.meta`).

Required failure — dropping the generic `continue`: the ordinary path cannot drop it
(`Continuation` is opaque, has only `fields()`, and refuses a token without `continue`).
To exercise the failure I expect to need Concord's native escape: build page 2 with
`category_members(..).set_continuation(c).prepare()?`, rewrite the URL without `continue`
via `Prepared::follow`, and read the raw JSON with `send_raw()` (because `Envelope` has
private fields). Against a fixture that, like a misbehaving/older continuation manager,
ignores `cmcontinue` when the generic `continue` is missing, that broken consumer gets
page 1 again (repeat); a fixture variant that answers without `continue` makes it stop
early (silent truncation). The SDK path over the same fixture returns every member once.

## First working call

The first version of `tests/first_use.rs` was 30 lines: a fixture replying with one search
hit, the README's transport configuration copied as-is, `concord::BaseUrl::parse((
format!("{}/w/api.php", server.url())).as_ref()).map(|origin| MediaWiki::new(.., MediaWikiConfig { origin: origin }))`, then `api.query().search("General relativity").send()`.
Imports: `MediaWiki`, `MediaWikiConfig`, `concord::reqwest`, plus the testserver's
`Recorded`, `Reply`, `TestServer`. It compiled and passed **on the first attempt**
(`cargo test --locked --offline -p concord-sdk-mediawiki-action-api --test first_use`). The
recorded wire was
`GET /w/api.php?action=query&format=json&formatversion=2&srsearch=General+relativity&srlimit=10&list=search&srprop=`
with headers `user-agent`, `accept: application/json`, `host`, which was exactly the
prediction. No source was needed. The workspace member and `Cargo.lock` entry were already
present from the author commit; `Cargo.toml` and `Cargo.lock` are unchanged.

## The application (`tests/first_use.rs`)

All values are synthetic (invented page IDs, revisions, tokens), modelled on the two cited
pages. Six tests, five run by default:

| Test | Workflow | What it proves |
|---|---|---|
| `search_choose_exact_title_and_fetch_its_content` | 1 | search with `srlimit=3`, pick the exact-title hit (the **second** result), `fetch_page`, follow the numeric `sroffset` continuation to the end; exact query pairs and headers of all 3 requests; `meta` status/operation; secret-free `Debug` |
| `search_and_fetch_failures_are_typed_and_quiet` | 1 | 429 with `retry-after` + typed document, HTML 503, 2xx `error` with `mediawiki-api-error` header, warnings, no `query`, truncated, HTML 200, >1 MiB, hang-up, body cut after a 200 head, token without generic `continue`, and the content cases (missing page, no revision, hidden main slot, other page ID, two pages, empty content). There are 17 requests, one per call |
| `category_traversal_carries_every_continue_field_to_the_end` | 2 | a 5-page listing (with an empty page that has a continuation, `batchcomplete: true` on every page, a changed generic value `-||categorymembers` and an unknown extra field `cmshard`) returns all 7 members, including the ns 14 and ns 6 ones. Each request carries exactly the previous `continue` object once. Incremental `next_page()` gives `[2,2,0,2,1]`, then `Ok(None)` with no further request |
| `category_bound_resume_and_failure_keep_the_prefix` | 2 | a bound of 2 keeps a 4-item prefix and `Bounded` at the page-2 token. The resume uses the README recipe; a 503 on the next page is a `Provider` error with `failed` = that token and `retry-after` visible. Retrying from it completes with no duplicates. `Bounded` is followed by `Ended`. The exact wire sequence of all 7 requests is asserted |
| `dropping_the_generic_continue_repeats_or_truncates` | 2 (required failure) | a hand-written consumer forwards `cmcontinue` without the generic `continue`. Against a fixture that **restarts**, it repeats page 1 and never reaches "Kinematics". Against a fixture that **looks finished**, it stops after 2 members without any signal. The SDK over the same fixtures returns all 7 members. Tokens without the generic field, null, `{}` or a fixed-input collision do not deserialize. A provider token without generic `continue` is a typed `Provider` error at `Resume::Start` and is never followed |
| `missing_page_id_without_title_is_content_unavailable` (`#[ignore]`) | 1 | reproduces finding F1 (below); fails today |

Results on 2026-11-07, in this worktree (`CARGO_BUILD_JOBS=4`, target `…/w4-sdk39-mediawiki-action-api/target`):

- `cargo test --locked --offline -p concord-sdk-mediawiki-action-api --test first_use`:
  5 passed, 1 ignored, repeated 8 times, all green.
- `... --test first_use -- --ignored`: the F1 test **fails**:
  `Err(Concord(Error { kind: Decode, endpoint: Some("MediaWiki.query.content"), message: GET MediaWiki.query.content: response not decoded: the JSON does not match the expected type (line 1, column 69) }))`.
- `cargo test --locked --offline -p concord-sdk-mediawiki-action-api` (whole package): first_use
  5 ok + 1 ignored, protocol 11 ok, workflows 6 ok, doctest 1 ok.
- `cargo clippy --locked --offline -p concord-sdk-mediawiki-action-api --all-targets -- -D warnings`: clean.
- `rustfmt --edition 2024 --check tests/first_use.rs`: clean.

## Friction incidents, in order

1. **Rustdoc of `CategoryPages` is a bare alias** (`Pages<PageIdentity, Continuation, (), Error>`).
   `cargo doc -p <sdk> --no-deps` does not document `concord-pagination`. To learn
   `next_page`, `PagesError`, `Resume` and `Incomplete`, I had to generate the docs for
   `concord-pagination`, `concord` and `concord-core` separately. The README's two sentences
   on `collect_complete`/`next_page` were right but did not cover matching.
2. **Import path for pagination types not in the README.** To match `PagesError::Bounded` or
   `Provider`, or compare `Resume::Start`, I needed `concord_sdk_mediawiki_action_api::pagination_engine::{PagesError, Resume}`.
   That re-export name (not `concord_pagination`) appears only in rustdoc's re-export line.
3. **Search continuation placement surprised me.** The continuation pairs are written at the
   declared position of `continuation`, between `srlimit` and `list`/`srprop`, not appended.
   Wire semantics are unaffected; my exact-order assertion failed once and now encodes the
   observed order.
4. **Probe of the `Continuation` rules** (a throwaway test, since deleted, called
   `serde_json::from_value::<Continuation>` on a list of cases). The README says
   "Missing, null, empty, … continuation data is rejected". Observed: `{}`, missing or null
   generic `continue`, float and array values, and `action`/`cmtitle`/`srsearch` collisions are
   rejected. The **empty strings** `"continue": ""` and `"cmcontinue": ""` are accepted. "empty"
   is therefore ambiguous. The README should say "empty object" or reject empty values.
   Real MediaWiki is not known to return an empty generic value, so this is a wording issue,
   not a demonstrated defect.
5. **F1 — misclassified missing page ID.** `fetch_page(404)` on
   `{"query":{"pages":[{"pageid":404,"missing":true}]}}` returns
   `Error::Concord(kind Decode, "line 1, column 69")`, not `ContentUnavailable` as the README's
   table promises ("Missing/invalid/unexpected page ID"). The same page with `ns`+`title` added
   gives `ContentUnavailable`, so the content model appears to require `title`/`ns`
   (inferred from the observed behaviour, not from source). Label: *inferred* provider shape. I
   believe MediaWiki reports unknown `pageids` as `pageid`+`missing` only, but the two cited
   pages document only the `titles=` form (`ns`+`title`+`missing`). Impact: a deleted or
   unknown page looks like a broken response, and the decode diagnostic names no field.
6. **Content step has no success metadata.** `fetch_page` returns `PageContent` without
   `ResponseMeta`. The generated `content(..).send()` does return `QueryResponse<ContentResults>`
   with `meta`, but `ContentResults` has only private fields. A caller cannot have both the
   checked content and its response headers, and `Error::ContentUnavailable` carries no meta
   either.
7. **No per-page metadata in the traversal.** `CategoryPages` has `M = ()` (predicted). I took
   headers from one direct `category_members(..).send()` call instead.
8. **`Error::Warnings` hides the warning.** It holds only `ResponseMeta`, so a refused response
   cannot say which module warned or why (for example, a caller-set `srlimit` above the
   wiki's maximum). There is no unredacted accessor, unlike `ApiProblem`.
9. **The native escape cannot be read for data.** `prepare()?.send()` yields an
   `Envelope<T>` with private fields. The broken-consumer control therefore needed
   `prepare()` → `request().url()` → `Prepared::follow(url)` → `send_raw()` → its own
   `serde_json` parsing. That was predicted and is acceptable for a deliberate control, but
   the README presents `prepare()?.send()` as an escape although its result has no accessors.
10. **Minor ceremony.** `category_pages(cat, 2, None, NonZeroUsize::new(10).unwrap())` takes
   four positional arguments with an unwrap. A swap of page size and bound is caught by the
   types. `content(pageids: u64)` uses the plural wire name for a single ID; the README steers
   to `fetch_page` anyway.
11. Not friction: transport configuration, User-Agent, base URL, error variants and the
   completion rules for `collect_complete`, `next_page` and search `continuation: None` were
   all clear from the README. No diagnostic misled me, except the field-less decode message
   in F1.

## Predictions vs observed

| Prediction | Observed |
|---|---|
| Setup, `with_base_url` keeps `/w/api.php`, headers `user-agent`/`accept: application/json`/`host` only | **match** |
| Search wire incl. guessed `srprop=` (empty) and default `srlimit=10` | **match** |
| Search continuation `{"continue":"-\|\|","sroffset":"3"}`, numeric kept as text, `None` at end | **match** |
| Continuation pairs appended at the end of the query | **mismatch**: inserted at the declared position (after `srlimit`); harmless |
| Content wire with guessed `prop=revisions&rvprop=ids\|content&rvslots=main&rvlimit=1` | **match** (all four values) |
| `PageContent` from `revid` + main-slot `content`; empty content `Ok("")` | **match** |
| 429/503 JSON → `Http` with typed document and `retry-after`; HTML 5xx → `Http` with document `Err(Metadata)` | **match** |
| 2xx `error` → `Api`; `warnings` → `Warnings`; no `query` → `InvalidResponse` | **match** (plus `mediawiki-api-error` header via `meta`) |
| truncated → `Decode`, HTML 200 → `Metadata`, >1 MiB → `Limit`, head and operation present | **match** |
| missing page / no revision / hidden slot / other ID → `ContentUnavailable`; two pages → `InvalidResponse` | **match** for every shape with `ns`+`title` |
| missing page ID (any shape) → `ContentUnavailable` | **mismatch (F1)** for `pageid`+`missing` only → `Concord(Decode)` |
| UA absent from `Debug`; `ApiProblem.info` absent from error `Debug`/`Display`; `Continuation` prints no values | **match** |
| Category first page wire; every `continue` field once on the next request, unknown field and changed generic value forwarded | **match** (order within the query not asserted) |
| Empty page advances; `batchcomplete` does not stop; exhaustion only without `continue`; no request after | **match** |
| Bound → `Incomplete` + `Bounded{At(token)}` after exactly 2 requests; resume recipe works | **match** |
| 503 mid-listing → `Provider{Http, failed: At(token)}`, prefix kept; `Ended` after an error | **match** |
| Token without generic `continue` → `Provider{Concord(Decode)}`, not followed | **match** |
| Empty-valued token rejected (my reading of "empty") | **mismatch**: `""` values accepted; `{}` rejected (friction 4) |
| `CategoryPages` exposes no per-page meta | **match** |
| Drop-generic control needs `prepare`/`follow`/`send_raw` + own parsing; repeats or truncates on the fixture; SDK complete | **match** |
| Transport failures (hang-up, body cut after a 200 head) | not predicted; observed `Concord(Transport)`, head `None` / `Some(200)` |

What would change in the SDK (not edited, `src/` is out of scope): accept the `pageid`+`missing`
(and `invalid`) page shapes as `ContentUnavailable`, and say "empty object" in the README or
reject empty values. Consider exposing the warnings document beside `Error::Warnings`, and
document `pagination_engine::{PagesError, Resume}` for matching. Nothing in the shared crates
needed changing for this slice.

## Evidence label

Live provider calls: **NOT_RUN**. All observations come from loopback fixtures.

Assistance label: **docs-only**: the README, the generated rustdoc of the SDK, `concord`,
`concord-core`, `concord-pagination` and `concord-example-testserver`, and the two official
pages cited above. No SDK `src/`, `tests/`, `fixtures/`, `FRICTION.md`, `PROVENANCE.md` or
`VALIDATION.md` was opened, before or after the first working call. Disclosure: workspace
metadata was seen while orienting: `git log --oneline -3` and `git show --stat HEAD`, i.e. the
author commit message (which mentions that its regression drops the generic field) and the
file list with line counts. I also read the root `Cargo.toml` member line, `rustfmt.toml` and
the testserver `Cargo.toml`. The full package test run executed the author's tests without
reading them.
