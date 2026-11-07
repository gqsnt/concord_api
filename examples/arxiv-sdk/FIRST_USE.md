# First use — concord-sdk-arxiv (fresh consumer)

Role: fresh consumer of `examples/arxiv-sdk` (I did not write it). Date: 2026-11-07.
Live provider calls: **NOT_RUN**. Every call below goes to `concord-example-testserver` on
loopback.

## What I read before predicting

- `examples/arxiv-sdk/README.md`.
- Rustdoc I generated with `cargo doc --locked --offline --no-deps` for `concord-sdk-arxiv`,
  and, because the SDK's pages leave Concord types unlinked under `--no-deps`, for
  `concord-example-testserver`, `concord-pagination`, `concord-ratelimit`, `concord` and
  `concord-core` (pages read: `concord::Error`, `ErrorKind`, `ResponseMeta`;
  `concord_pagination::{Pages, PagesError, Resume, Page, PageItems, Incomplete, Progress}`;
  `concord_ratelimit::{Clock, Error}`; all public items of the test server).
- Official documentation, read 2026-11-07 (read-only, through a fetch tool; nothing sent to
  the API itself): <https://info.arxiv.org/help/api/user-manual.html> — §3.1 (GET/POST),
  §3.1.1 / §3.1.1.1 (`search_query`, `id_list`, combination rule), §3.1.1.2 (start /
  max_results, "slices of at most 2000", 30,000 cap, HTTP 400 above it, 3-second delay),
  §3.3.1 (feed elements, OpenSearch namespace `http://a9.com/-/spec/opensearch/1.1/`),
  §3.3.2 (entry elements, arXiv namespace `http://arxiv.org/schemas/atom`, the three link
  kinds), §3.4 (error feeds: one entry, id `http://arxiv.org/api/errors#…`, title `Error`),
  §5.1 (field prefixes, `%22` phrase quoting, `+` for spaces), §5.1.1 (version suffixes).
  The manual does not state the response Content-Type nor the HTTP status of most error
  feeds.
- Not opened: `examples/arxiv-sdk/src`, `tests/`, `fixtures/*` contents, `FRICTION.md`,
  `PROVENANCE.md`, `VALIDATION.md`. I did see *file names* while listing directories
  (`tests/{common,protocol.rs,workflows.rs}`, `fixtures/{empty,error-id,…,search-0,…}.xml`)
  to make sure `tests/first_use.rs` did not already exist. My fixtures are written from the
  manual, not from the SDK's fixtures.

## Predictions

Common setup (both workflows):

```rust
let server = TestServer::start(|r: &Recorded| /* Reply */).await;
let api = Arxiv::with_http(http_client()?).with_base_url(format!("{}/api", server.url()))?;
let papers = api.papers();
```

- `with_base_url` appends the declared path, so the wire path is `/api/query`.
- Headers: no `authorization`, no cookie, no API key — the API is public and there is no
  secret anywhere in the client. I expect `accept: application/atom+xml` (the docs say the
  format's `Decode::ACCEPT` is sent as a fallback; its value is not printed in rustdoc, so
  this is a guess) and no `user-agent` (`http_client()` docs mention none).
- Pacing: the first request is admitted immediately; each later request through the same
  client waits until ≥ 3 s after the previous admission. Separate `Arxiv` clients built with
  `with_http` each get an independent policy.

### Workflow 1 — search by term, inspect an entry

Calls: `papers.search(SearchTerm::new("electron")?).set_size(PageSize::new(10)?).send().await`,
then inspect `feed.entries[0]`; then a bounded traversal
`papers.search(term).set_size(PageSize::new(2)?).pages(Resume::Start, NonZeroUsize::new(3)?)`.

Wire, first call: `GET /api/query?search_query=all%3A%22electron%22&start=0&max_results=10`
— parameter order `search_query, start, max_results` (the order rustdoc lists), form
encoding (`:`→`%3A`, `"`→`%22`, space→`+`), so a two-word term
`quantum criticality` sends `search_query=all%3A%22quantum+criticality%22`. No body.

Returns `Feed { total_results, start, items_per_page, entries, problem: None }`. An entry
is `Paper` with `id` = abstract URI as sent (`http://arxiv.org/abs/2101.00001v2`, not a bare
ID), `title`/`summary` trimmed, `published`/`updated` as provider strings, `authors` with
`affiliations`, `categories`, `links` (alternate + pdf (+ doi)), and the optional
`primary_category`, `comment`, `journal_ref`, `doi`.

Pagination: `next_page()` gives `Page { items: Vec<Paper>, next: Option<u32> }`. With page
size 2 and `totalResults=5`: requests `start=0`, `start=2`, `start=4`; the third page has
one entry and `next == None`, then `next_page()` returns `Ok(None)` (explicit end). With a
budget of 2 pages and records left: `Err(PagesError::Bounded { resume: Resume::At(4), .. })`
— the next offset. A short page whose total says more remain continues; I guess from
`start + entries.len()` (README says it continues but not from where). An empty page while
`start < totalResults` is `Error::Pagination` (non-advancing). A feed whose
`opensearch:startIndex` differs from the requested `start` (or whose `itemsPerPage` differs
from the requested size) is `Error::Pagination`. `items().collect_complete()` returns
`Err(Incomplete { collected: <prefix>, error })` on any of these.

Failures:
- `SearchTerm::new("")`, a term with `"` or `\`, `PageSize::new(0)` / `PageSize::new(101)`
  → `Error::Input`, before any request (server sees nothing).
- `set_start(29_995)` with size 10 → `Error::Input` at `send()`; nothing on the wire.
- HTTP 400 with an Atom error feed → `Error::Http(f)`: `f.meta.status() == 400`,
  `f.document == Ok(feed)` with `feed.problem == Some(Problem { id: ".../errors#…",
  summary })`.
- HTTP 503 with an HTML page → `Error::Http(f)` with `f.document` = `Err(concord::Error)`
  of kind `Metadata` (HTML is not Atom). Status still visible. No retry (one request).
- 200 + Atom error entry → `Error::Api(a)` with `a.problem.summary` holding the message.
- 200 with `content-type: text/html` or `application/xml` → `Error::Concord(e)`,
  `e.kind() == ErrorKind::Metadata`. `application/atom+xml; charset=utf-8` is accepted.
- 200 malformed/truncated XML → `Error::Concord(e)`, kind `Decode`.
- Connection dropped mid-body → `Error::Concord(e)`, kind `Transport`.
- Body > 2 MiB → `Error::Concord(e)`, kind `Limit`.
- In a traversal, a page failure → `PagesError::Provider { error: Error::…, failed:
  Resume::At(offset), .. }`; the error is the same SDK `Error` as `send()`.

### Workflow 2 — fetch known IDs, read metadata

Calls: `papers.fetch(IdList::new(&["2101.00001v2", "hep-ex/0307015", "cs/9901002v1"])?)
.send().await`.

Wire: `GET /api/query?id_list=2101.00001v2%2Chep-ex%2F0307015%2Ccs%2F9901002v1&start=0&max_results=50`
(comma-joined, form-encoded so `,`→`%2C`, `/`→`%2F`; `start=0&max_results=50` fixed — fetch
has no `set_start`/`set_size`). No `search_query` parameter.

Returns `Feed` with entries in provider order (not reordered to my input order). A
well-formed ID the provider does not return is simply absent; I must compare requested IDs
with `Paper.id` myself, stripping `http://arxiv.org/abs/` (I expect no helper for that).

Extension/optional-field handling (the failure this slice must exercise): an entry with no
`arxiv:comment`, `arxiv:journal_ref`, `arxiv:doi`, `arxiv:primary_category`, no
`arxiv:affiliation`, a link without `type`/`title`, or a category without `scheme` still
decodes, with `None`/empty values; unknown extension elements (another namespace, including
one named `entry` or `title`) are ignored; the arXiv namespace bound to an unusual prefix is
still recognised. One entry missing a *required* field (`summary`, say) fails the whole
feed: `Error::Concord`, kind `Decode` (README: core fields are required).

Failures:
- `IdList::new(&[])`, 51 IDs, an ID with `,`, whitespace or a URL
  (`http://arxiv.org/abs/…`) → `Error::Input`; nothing sent.
- A syntactically odd but delimiter-safe ID (`1234.12345`) passes `IdList::new` (the SDK
  leaves grammar to arXiv) and the provider's error feed comes back: `Error::Http` (if 400)
  or `Error::Api` (if 200) with the slug in `problem.id`.
- Same media/decode/transport/limit failures as workflow 1.
- Secrets: none involved. `Debug`/`Display` of the request and of SDK errors should not
  contain the IDs or the provider's error summary; the `Paper` `Debug` does print metadata.

---

Everything below was written after the predictions above, from observed runs.

## First working call

What it took: the README example plus the test-server rustdoc. One `TestServer::start`
returning `Reply::text(200, "application/atom+xml", <manual-shaped feed>)`, then
`Arxiv::with_http(http_client()?).with_base_url(format!("{}/api", server.url()))?`,
`.papers().search(SearchTerm::new("electron")?).set_size(PageSize::new(10)?).send().await`.
It compiled and passed on the first attempt; no source lookup. Imports:
`concord_sdk_arxiv::{Arxiv, PageSize, SearchTerm, http_client}` and the test server's
`{Recorded, Reply, TestServer}`, all needed. `Cargo.toml`/`Cargo.lock` did not change: the
package was already a workspace member with a lock entry, so every build used `--locked`.

Observed wire request:

```
GET /api/query?search_query=all%3A%22electron%22&start=0&max_results=10
accept: application/atom+xml
user-agent: concord-sdk-arxiv/0.1.0
host: 127.0.0.1:<port>
```

## The application (`tests/first_use.rs`)

Ten tests, loopback only, fixtures written from manual §3.3/§3.4 (synthetic titles):

| Test | Workflow | Asserts |
| --- | --- | --- |
| `search_inspect_entry_then_page_to_the_end` | 1 | exact query string and decoded pairs; `accept`, no auth/cookie/body; all `Feed` counts; every field of a full entry (2 affiliations, DOI link, journal ref, comment, primary category, multi-line title); a sparse entry with no optional fields and foreign `entry`/`title`/`media:thumbnail` extensions; 3-page traversal `next = [Some(2), Some(4), None]`, then `Ok(None)` again; request offsets `0,0,2,4`; ≥ 2.9 s between admissions on one client |
| `search_budget_is_resumable_from_the_reported_offset` | 1 | budget 2 → `PagesError::Bounded { resume: At(4) }`, then `Ended`; no third request; resuming from `At(4)` returns the last item |
| `search_failure_mid_traversal_keeps_the_collected_prefix` | 1 | 503 HTML on page 2 → `Incomplete.collected` = the first 2 papers; `failed = At(2)`; `Error::Http` status 503, `Retry-After` readable, `document` = `Err(Metadata)`; exactly one request for the failed page (no retry) |
| `search_paging_metadata_is_checked` | 1 | a short page continues from `start + entries` (offsets `0,1,3`, no gap or duplicate); an empty page while the total says more → `Provider(Error::Pagination)`; echoed `startIndex`/`itemsPerPage` mismatch → `Error::Pagination`; `Resume::Start` honours `set_start(3)` |
| `search_traversal_stops_at_the_30000_window` | 1 | 40,000 matches: the page at 29,990 is read (`next = Some(30000)`); the next is `PagesError::Provider` wrapping `Error::Input`, `resume = At(30000)`; never sent |
| `search_refuses_bad_input_before_dispatch` | 1 | blank, quote, backslash, control, 513-byte terms; page sizes 0/101; a window ending at 30,001 → `Error::Input` with no request; the window ending at exactly 30,000 is sent |
| `search_provider_and_transport_failures` | 1 | 200 + error entry → `Error::Api` (id/summary kept, absent from `Display`/`Debug`); 400 + error feed → `Error::Http` with the decoded `problem`; `text/html`, `application/xml`, `charset=ISO-8859-1` → `Metadata`; `charset=utf-8` accepted; malformed and cut-short XML → `Decode`; dropped connection → `Transport`; > 2 MiB → `Limit`; 429 → `Error::Http`, one request |
| `fetch_known_ids_and_read_metadata` | 2 | request `Debug` hides IDs; exact `id_list=…%2C…%2F…&start=0&max_results=50`, no `search_query`; provider order kept; the caller computes the missing ID; `os:`/`ax:` prefixes recognised; a feed-level foreign `entry` ignored |
| `fetch_refuses_unsafe_lists_before_dispatch` | 2 | empty / 51 IDs / comma / URL / trailing space / blank / `&=` refused; 50 accepted; `1234.12345` accepted (grammar is arXiv's); `Display` is the comma-joined wire spelling |
| `fetch_provider_errors_and_required_fields` | 2 | manual §3.4 malformed-ID feed under 400 → `Error::Http` with `problem.id` slug, `Display`/`Debug` free of the ID; same feed under 200 → `Error::Api`; an entry without `<summary>` fails the whole feed (`Decode`) |

Results (`CARGO_TARGET_DIR=<worktree>/target`, `CARGO_BUILD_JOBS=4`, one command at a time):

- `cargo test --locked --offline -p concord-sdk-arxiv --test first_use`: **10 passed**, 9.0 s
  (the real 3-second pacing; no fake clock).
- `cargo test --locked --offline -p concord-sdk-arxiv` (whole package): unit 0, `first_use`
  10, `protocol` 16, `workflows` 5, doc-tests 1 — all passed.
- `cargo clippy --locked --offline -p concord-sdk-arxiv --all-targets -- -D warnings`: clean.
- `rustfmt --edition 2024 --config-path rustfmt.toml --check tests/first_use.rs`: clean.

Two throw-away probe files (`tests/zz_probe.rs`, `tests/zz_literal_probe.rs`) were used to
observe behaviour the docs leave open; their findings are below and in the tests. Both were
deleted.

## Friction, in the order met

1. **Unlinked Concord types in the SDK rustdoc.** The README's `cargo doc … --no-deps` leaves
   every external type as a bare name. Three different `Error`s render identically:
   `Error::Concord(Box<Error>)` (concord), `Error::Admission(Error)` (presumably
   `concord_ratelimit::Error`; stated nowhere I read), `HttpFailure.document:
   Result<Feed, Error>` (concord; the README says so), and `prepare() -> Result<_, Error>`.
   I had to grep hrefs in the HTML and build rustdoc for five more crates (`concord`,
   `concord-core`, `concord-pagination`, `concord-ratelimit`, the test server).
2. **`Policy::with_clock(clock: impl Clock)`.** `Clock` lives in `concord-ratelimit`, which
   the SDK does not re-export (it re-exports only `concord` and `concord_pagination`). A
   caller who wants deterministic pacing must depend on that crate directly and match its
   version. I stayed on the ordinary path, so my suite pays real 3-second waits.
3. **Generated docs talk in macro terms and mislabel the policy.** Endpoint pages repeat
   "the nearest format statement wins", "the result the client declares on its send
   statement", and `Arxiv::validate_header_names()` shows up as a public client method.
   `Arxiv::set_policy` and `ArxivConfig.policy` say "Sensitive: sent normally, withheld from
   Concord's Debug output", but a pacing policy is never sent. The client's `Debug` then
   prints `policy: <redacted>`. None of this is wrong enough to break a call, but it is noise
   a caller has to read past.
4. **Undocumented `user-agent: concord-sdk-arxiv/0.1.0`.** Neither the README nor
   `http_client()` mentions it (prediction mismatch, harmless).
5. **`Resume::Start` means "the request's `set_start`", not offset 0.** `set_start(3)` plus
   `pages(Resume::Start, …)` starts at 3; `Resume::At(4)` silently overrides `set_start(1)`.
   This is reasonable but undocumented. There are two ways to say where a traversal starts.
6. **The 30,000 boundary during traversal looks like a retryable provider failure.** The
   page at 29,990 reports `next: Some(30000)`; the next call returns
   `PagesError::Provider { error: Error::Input(..), failed: At(30000) }`, though nothing was
   sent and no retry can succeed. A caller that retries `Provider` failures from `failed`
   would loop. I had to match `Error::Input` inside `Provider` to tell "window exhausted"
   from a real failure. The rustdoc says "explicit error" but not which one (unclear
   completion).
7. **Workflow 2 needs caller-side ID matching.** `Paper.id` is the abstract URI; there is no
   bare-ID accessor, so diffing requested against returned IDs ("missing IDs are not
   replaced", README) means stripping `http://arxiv.org/abs/` by hand. Live arXiv also
   versions returned IDs, which would need version handling too (inferred from the manual
   §5.1.1; not run).
8. **Input reasons are one string per type.** `"invalid search term"` covers blank, quote,
   backslash, control and over-length. `"invalid identifier list"` covers empty, > 50,
   comma, URL and whitespace. An application validating user input cannot say which rule
   failed without re-implementing the rules.
9. **Decode diagnostics are coarse.** `"missing required Atom field"` names neither the
   field nor the entry index. A well-formed feed in the wrong namespace reports
   `"malformed or excessive Atom XML"`. Withholding values is intended, but a field name or
   index is not sensitive.
10. **One bad entry fails the whole feed.** An empty `<title/>` or a missing `<summary>` in
    one entry discards every other entry of a 50-ID fetch, with no partial result.
    This is documented ("core … required") and matches the slice rule, which only protects
    *optional* fields. It is a design observation, not a defect.
11. **Titles keep internal line breaks.** `"Quantum criticality in a\n      two-dimensional
    electron gas"` is documented ("preserved except surrounding whitespace"), so every
    display caller normalises whitespace itself.
12. **The search/fetch strictness asymmetry is undocumented.** Search rejects any
    `startIndex`/`itemsPerPage` echo mismatch (`Error::Pagination`). Fetch accepts
    `itemsPerPage` of 1, 2, 10 or 50, and even `totalResults` smaller than the entry count.
    Search's exact `itemsPerPage == max_results` check is unverified against live arXiv
    (NOT_RUN). If the service ever echoed a different value, every search would fail; manual
    §3.3.1.2 ("analogous to start and max_results") supports the check.
13. **`Feed.problem` sits on the success type** but generated `send()` never returns it set.
    It only matters inside `HttpFailure.document`. This is documented, but a caller may
    test it needlessly; I did, in the first draft.

Not friction: the README example was enough for the first call. `PageSize` is `Copy`.
Unsuffixed integer literals work for `set_start`. `concord::ErrorKind`, `PagesError` and
`Resume` are reachable through the SDK's re-exports. `Retry-After` is readable on
`HttpFailure.meta`.

## Predictions versus observations

| Prediction | Observed | Verdict |
| --- | --- | --- |
| Path `/api/query` after `with_base_url(<server>/api)` | same | match |
| Search query `search_query=all%3A%22quantum+criticality%22&start=0&max_results=2`, in that order | exact | match |
| `accept: application/atom+xml` (a guess) | sent | match |
| No `user-agent` | `user-agent: concord-sdk-arxiv/0.1.0` | **mismatch** |
| No auth, cookie or secret anywhere | none on the wire | match |
| First admission immediate, then ≥ 3 s apart per client | server-side gaps ≥ 2.9 s; 4 requests in 9.0 s | match |
| Fetch `id_list=…%2C…%2F…&start=0&max_results=50`, no `search_query` | exact | match |
| `Feed`/`Paper` shapes; `Paper.id` is the abs URI; caller strips the prefix | same | match |
| Traversal offsets `0,2,4`, `next` `Some(2),Some(4),None`, then `Ok(None)` | same, and `Ok(None)` stays | match |
| Budget → `Bounded { resume: At(4) }` | same; later calls `Ended` | match |
| Short page continues from `start + entries.len()` (a guess) | `next = Some(1)` | match |
| Empty page with records left → `Error::Pagination` | inside `PagesError::Provider` | match |
| `startIndex`/`itemsPerPage` mismatch → `Error::Pagination` | search: yes; **fetch: not checked** | partial |
| Traversal starts at offset 0 | starts at the request's `set_start` value | **mismatch** (not predicted) |
| Window beyond 30,000 → `Error::Input` at `send()`, nothing sent | same; during traversal it is `PagesError::Provider(Input)` with `resume At(30000)` | partial (traversal form not predicted) |
| Input refusals before dispatch | same, no request recorded | match |
| 400 + error feed → `Http`, `document` = `Ok(feed)` with `problem` | same (`total 1`, `entries 0`) | match |
| 503 HTML → `Http`, `document` = `Err(Metadata)` | same | match |
| 200 + error entry → `Api` | same | match |
| Wrong media → `Metadata`; `charset=utf-8` accepted | same; `ISO-8859-1` refused | match |
| Malformed / cut-short XML → `Decode`; dropped body → `Transport`; > 2 MiB → `Limit` | same | match |
| Page failure → `Provider { error, failed: At(offset) }` | same; no retry | match |
| Fetch keeps provider order; missing IDs are just absent | same | match |
| Optional fields absent → `None`/empty; extensions ignored (also named `entry`/`title`); renamed prefixes recognised | same | match |
| Missing required field → whole feed `Decode` | same; an empty `<title/>` also counts as missing | match (+ detail) |
| `1234.12345` passes `IdList::new` | same | match |
| Request/error `Debug`/`Display` withhold IDs, term, summary | same; `Paper` `Debug` prints metadata | match |

## Assistance label

**`docs-only`.** I used the README, rustdoc generated locally (the SDK, `concord`,
`concord-core`, `concord-pagination`, `concord-ratelimit`, `concord-example-testserver`), and
the arXiv user manual (read 2026-11-07). I did not open `examples/arxiv-sdk/src`, its
`tests/`, `fixtures/` contents, `FRICTION.md`, `PROVENANCE.md` or `VALIDATION.md`, nor any
rustdoc `src/` page. I saw only file names while listing directories. My fixtures come
from the manual, not from the SDK's fixtures.

Not run: live arXiv (NOT_RUN by rule), native Windows, and any mutation of the SDK (that
would need source reading). Everything above was observed on loopback, Linux, toolchain
1.99.1.
