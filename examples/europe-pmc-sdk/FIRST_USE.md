# First use — fresh consumer of `concord-sdk-europe-pmc` (SDK49, Europe PMC)

Role: fresh consumer (did not write the SDK). Date: 2026-11-07.
Live provider calls: **NOT_RUN**. Every call below goes to `concord-example-testserver`
on loopback.

## What I read before predicting

In this order, all inside the worktree unless stated:

1. `examples/europe-pmc-sdk/README.md` and `examples/europe-pmc-sdk/Cargo.toml`
   (the manifest, to learn the dev-dependencies a test can use).
2. Generated rustdoc: `cargo doc --locked --offline -p concord-sdk-europe-pmc --no-deps`
   (every page under `target/doc/concord_sdk_europe_pmc/`, converted to text with a small
   script in `target/tmp/consumer/h2t.py`; `[src]` links were skipped).
3. Generated rustdoc of the test server (`concord-example-testserver`), of
   `concord-pagination` (`Pages`, `Resume`, `PagesError`, `Incomplete`, `Page`,
   `PageItems`) and of `concord` (`Error`, `ErrorKind`, `ResponseMeta`, `Operation`).
4. Official documentation, read 2026-11-07:
   - <https://dev.europepmc.org/RestfulWebService> (HTML; copy and text extraction kept in
     `target/tmp/consumer/docs/restful.{html,txt}`). Facts used: base URL
     `https://www.ebi.ac.uk/europepmc/webservices/rest/search`; `format` is XML, JSON or DC
     and "the default value is XML if the parameter is unspecified"; `resultType` is
     `idlist`, `lite` (default) or `core`.
   - EMBL-EBI webinar slides, *Europe PMC programmatically* (2020) PDF (copy in
     `target/tmp/consumer/docs/webinar-2020.pdf`), pp. 32–44: citations route
     `/{source}/{id}/citations` with `page` and `pageSize` (p. 32); source codes MED, PMC,
     PPR, PAT, AGR, CBA, ETH (p. 33); `id`/`source` in a search `<result>` (p. 34);
     citations XML envelope `responseWrapper/hitCount/request/citationList/citation` with
     `id, source, citationType, title, authorString, journalAbbreviation, pubYear, volume,
     issue, citedByCount` (pp. 36–37); XML is the default output, JSON optional (p. 39);
     result types (p. 40); `cursorMark` (`*` or omitted for the first request) and
     `nextCursorMark` in the response (p. 41); `pageSize` default 25, maximum 1000 (p. 42);
     search XML envelope with `request/queryString,resultType,cursorMark,pageSize,sort,synonym`
     and `resultList/result` (p. 44).

Not read before the first working call: `examples/europe-pmc-sdk/src/`, its `tests/`,
`fixtures/` contents, `FRICTION.md`, `PROVENANCE.md`. (An initial `ls` of the package
showed the *names* of the files in `src/`, `tests/` and `fixtures/`, e.g.
`fixtures/repeated.xml`; none were opened.) All response bodies in my tests are written
from the official documentation above, not copied from the SDK's fixtures.

## Predictions

Setup common to both workflows (from the README snippet):

```rust
let api = EuropePmc::new(http_client()?)
    .with_base_url(format!("{}/europepmc/webservices/rest", server.url()))?;
let search = api.search(Query::new("malaria")?, PageSize::new(2)?);
```

`with_base_url` keeps its path, so every request target should start with
`/europepmc/webservices/rest`. Construction performs no I/O.

### Workflow 1 — JSON search, select a publication, fetch one citation

Calls, in order:

1. `search.pages(SearchFormat::Json, Resume::Start, cap)?.next_page().await?`
   (or `search.json().send().await?` for a single page).
   - Wire: `GET /europepmc/webservices/rest/search?query=malaria&pageSize=2&cursorMark=*&format=json&resultType=lite`.
     Parameter order as listed in the rustdoc ("query, pageSize, cursorMark, format,
     resultType"). `cursorMark=*` is sent on the first page because the rustdoc says the
     cursor is "Initially `Cursor::start()`" (I expect `*` to stay unescaped). A query with
     spaces and a colon (`malaria AND OPEN_ACCESS:y`) should be percent/form encoded; I
     expect `+` for spaces and `%3A` for `:`, but I am not sure which space spelling
     Concord uses — the test asserts on the *decoded* value and records the raw target.
   - Headers: `accept: application/json` (the JSON codec's suggested Accept), `host`. No
     `authorization`, `cookie`, `referer`; no `user-agent` (reqwest sets none by default);
     no body.
   - Returns `Page<Publication, Cursor, u64>`: `items` = `resultList.result`, `next` =
     `nextCursorMark`, `meta` = `hitCount`. With `send()` it is a `SearchPage`.
2. Select `page.items[0]`, `key()` → `RecordKey { source: MED, id: 20485428 }`.
3. `api.publication(key.source, key.id).citations().set_page_size(PageSize::new(1)?).send().await?`
   - Wire: `GET /europepmc/webservices/rest/MED/20485428/citations?pageSize=1&format=json&page=1`,
     `accept: application/json`.
   - Returns `CitationsPage { hit_count: 288, citation_list.citation: [one Publication] }`.
     The official citations record has `journalAbbreviation`, not `journalTitle`, so I
     predict `journal_title == None` on a citation unless the SDK aliases it.
4. `api.fetch_citation(&citation.key()).await?`
   - Wire: `GET /europepmc/webservices/rest/search?query=EXT_ID:32403408 AND SRC:MED&pageSize=…&format=json&resultType=core`
     (the exact query spelling, possible quoting and the page size — probably 1 or 2 so that
     ambiguity can be seen — are guesses; the rustdoc only says "via the search module").
   - Returns one `Publication` with `abstract_text` filled from `abstractText`. A real core
     record carries the journal under `journalInfo.journal.title`, so I predict
     `journal_title == None` for core results too.

Failures I expect:

- Local refusals before I/O, as `Error::InvalidInput(&'static str)`: blank `Query`,
  `PageSize::new(0)` / `new(101)`, lowercase or non-letter `Source::new("med")`,
  `PublicationId::new("a/b")`, `Cursor::new("")`. No request reaches the server.
- Non-2xx (400, 404, 429, 500, a 302 redirect): `Error::Http(Box<HttpError>)`;
  `err.status()` is `Some(code)`; `HttpError.body` holds at most 16 KiB of raw bytes;
  `retry_after(now)` on `Retry-After: 120` gives `Ok(Some(120 s))`; absent header gives
  `Ok(None)`. No retry: exactly one request.
- The provider's **XML default** served to the JSON operation (`content-type:
  application/xml`): `Error::Call` with `ErrorKind::Metadata`, refused before the body is
  read; `status()` is `Some(200)`.
- XML falsely labelled `application/json`: `Error::Call` with `ErrorKind::Decode`.
- Truncated / malformed JSON: `ErrorKind::Decode`. Body > 256 KiB: `ErrorKind::Limit`.
- A 200 document that reports a provider error instead of results (no `resultList`): I
  expect a decode failure, because `result_list` is not optional — or `InvalidPage`.
- More records than requested (3 records for `pageSize=2`): `Error::InvalidPage`.
- `fetch_citation`: zero records → `Error::NotFound`; a different id or two records →
  `Error::UnexpectedRecord`.
- Secrets: there are none in this slice (public API, no key). `Debug` of the client,
  scopes, requests and errors should not print the query text or cursor; `Display` of
  `Query` prints the raw wire value.

### Workflow 2 — the same bounded search in XML, continue with the returned cursor

1. `let mut xml = search.pages(SearchFormat::Xml, Resume::Start, NonZeroUsize::new(3)?)?;`
2. `xml.next_page().await?` → `GET …/search?query=malaria&pageSize=2&cursorMark=*&format=xml&resultType=lite`
   with an XML Accept (I predict `accept: application/xml`). Response
   `content-type: application/xml;charset=UTF-8`, root `<responseWrapper>` with
   `<hitCount>`, `<nextCursorMark>AoIIQKsQtyg0MjQ5NDc4NA==</nextCursorMark>`, `<request>`
   and `<resultList><result>…`. Returns `Some(page)` with 2 items, `next` = that cursor,
   `meta` = hit count.
3. Second `next_page()` → same target with `cursorMark=AoIIQKsQtyg0MjQ5NDc4NA%3D%3D`
   (`=` percent-encoded), decoded value equal to the cursor text. A last page without
   `<nextCursorMark>` returns `Some(page)`, and the **following** call returns `Ok(None)`
   without another request.
4. Repeated cursor: if page 2 returns the cursor that was already requested (or page 3
   returns page 1's cursor), `next_page()` returns
   `Err(PagesError::Provider { error: Error::RepeatedCursor, .. })`, that page's items
   are not emitted, and every later call is `Err(PagesError::Ended { .. })` with no further
   request. With `items().collect_complete()` I expect `Incomplete { collected: <items of
   the pages before>, error: Provider(RepeatedCursor) }`. `failed` should be
   `Resume::At(<cursor sent for the offending page>)`.
5. Cap: `max = 1` and a page with a next cursor → `Err(PagesError::Bounded { resume:
   Resume::At(next cursor) })`, never `Ok(None)`. Resuming with
   `search.pages(Xml, Resume::At(cursor), cap)` sends that cursor first.
6. An empty or whitespace-only page with a fresh cursor continues. An empty
   `<nextCursorMark/>` is invalid (I expect `InvalidPage` or a decode error; the README only
   says "invalid").
7. Malformed / truncated XML → `ErrorKind::Decode`; an XML document with a DTD entity →
   refused (`Decode`); JSON labelled `application/xml` → `Decode`; non-2xx → `Error::Http`.

Open question (cannot be checked: live calls are NOT_RUN): Europe PMC's search is
cursor-based in the Solr style. If the real service signals the end by echoing the cursor
it was sent (Solr's convention) rather than omitting `nextCursorMark`, this SDK would
report `RepeatedCursor` at the true end of a listing. The official pages I read do not say
how the last page is marked.

## First working call

Test `first_call` (since grown into
`workflow_1_through_the_json_traversal_has_page_metadata`): compiled and passed on the first
`cargo test`, before I had opened any SDK source. It took the README snippet almost unchanged:
`EuropePmc::new(http_client()?)`, `.with_base_url(format!("{}/europepmc/webservices/rest", server.url()))?`,
`api.search(Query::new("malaria")?, PageSize::new(2)?)`,
`.pages(SearchFormat::Json, Resume::Start, NonZeroUsize::new(1)?)?.next_page().await?`.
Imports: `EuropePmc, PageSize, Query, SearchFormat, http_client` from the SDK,
`concord_pagination::Resume` through the SDK re-export, `std::num::NonZeroUsize`, and the test
server's `Recorded, Reply, TestServer`. The server saw exactly

```text
GET /europepmc/webservices/rest/search?query=malaria&pageSize=2&cursorMark=*&format=json&resultType=lite
accept: application/json
host: 127.0.0.1:<port>
```

## The application (`tests/first_use.rs`, 17 tests)

All bodies are written from the official pages listed above (lite/core/citations envelopes,
`responseWrapper` XML), with realistic records (MED/20485428 → citing MED/32403408).

Workflow 1, JSON (`workflow_1_*`): ordinary `search.json().send()`, selection with
`Publication::key()`, `api.publication(source, id).citations().set_page_size(1).send()`,
`api.fetch_citation(&key)`; three wire requests asserted (path, decoded query pairs and order,
`accept`, no `authorization`/`cookie`/`referer`/`content-type`, empty body). Failures: XML
default refused (`Metadata`) on `send()` and inside a traversal (then `Ended`); XML labelled
JSON, truncated JSON, a 200 without `resultList`, an empty JSON cursor (`Decode`); HTML 200
(`Metadata`); 300 KiB (`Limit`); 429 with `Retry-After: 120` (status, raw body, `retry_after`,
operation name, one request); 302 not followed (`Error::Http`); zero hits = one empty final page
then `Ok(None)`; three records for `pageSize=2` (`InvalidPage`); `fetch_citation` NotFound /
UnexpectedRecord (other id, other source, `hitCount` 2 with one record) / InvalidPage (two
records); local `InvalidInput` refusals with no request; redaction of query, cursor, error body
and title in `Display`/`Debug` of the error, client, scope, request and model. Partial result:
the selected publication survives a failed citations step.

Workflow 2, XML (`workflow_2_*`): `pages(SearchFormat::Xml, Resume::Start, 5)` over three pages
(`*` → `CURSOR_1` → `CURSOR_2` → no cursor), same query/page size/result type as workflow 1,
`format=xml`, `accept: application/xml`, `=` padding sent as `%3D%3D`, hit count as `page.meta`,
`Ok(None)` sticky without a request; single-page `xml().set_cursor(next)`; repeated cursor
(page 2 echoes the cursor it was sent) → `Provider { error: RepeatedCursor }` with
`failed = At(CURSOR_1)`, then `Ended`, exactly 2 requests, and `items().collect_complete()`
keeping only page 1's items; a two-page cycle (`CURSOR_2` → `CURSOR_1`) also stops; cap 1 →
`Bounded { resume: At(CURSOR_1) }` and a resumed traversal that finishes the listing; an empty
page with a fresh cursor continues; truncated XML, a DTD entity, JSON labelled XML, an empty
`<nextCursorMark>` (`Decode`); JSON labelled JSON at the XML operation (`Metadata`); 503 on
page 2 (`Error::Http` inside `Provider`, resume at the failed cursor, then `Ended`); 300 KiB
XML (`Limit`).

Commands (worktree, `CARGO_TARGET_DIR` = the worktree's absolute `target/`, `CARGO_BUILD_JOBS=4`):

```text
cargo test   --locked --offline -p concord-sdk-europe-pmc                 # 0 + 17 + 13 + 6 tests + 1 doctest, all ok
cargo test   --locked --offline -p concord-sdk-europe-pmc --test first_use # 17 passed, run 3 times
cargo clippy --locked --offline -p concord-sdk-europe-pmc --all-targets -- -D warnings  # clean
rustfmt --edition 2024 --check examples/europe-pmc-sdk/tests/first_use.rs # clean
```

`Cargo.lock` and the root `Cargo.toml` were not changed (the package was already a workspace
member). No new crate was added.

## Friction incidents, in order

1. **Rustdoc of the client opens with `validate_header_names()`.** The first method on
   `EuropePmc` is a generated header-name check with a long paragraph about native
   `HeaderName` constants, before `new`/`with_base_url`/`search`. Irrelevant to a caller of
   this SDK (and it says itself that it always returns `Ok(())` here).
2. **Fixed wire values are invisible in the endpoint rustdoc.** "Query (when present): query,
   pageSize, cursorMark, format, resultType" lists names only. That `format=json|xml`,
   `resultType=lite|core` and citations `page=1` are pinned had to come from the README
   ("pins `format=json` or `format=xml`") and from guessing; I confirmed them on the wire.
   The XML decoder's `ACCEPT` value is also hidden (`const ACCEPT: Option<&'static str>`
   without a value); observed `application/xml`.
3. **Long repeated generated boilerplate.** Each endpoint page repeats a ~70-word paragraph
   on `send()` vs `prepare()?.send()` and `Decode::ACCEPT`; the useful line (method + path)
   is buried in it.
4. **Two types named `Error` in signatures.** `Error::Call(Error)` and `HttpError.body:
   Result<Bytes, Error>` print the same bare name for the SDK error and `concord::Error`.
   To read a stage I needed `use concord_sdk_europe_pmc::concord::ErrorKind` and
   `err.call_error().map(|e| e.kind())`; a one-step `Error::kind()` would avoid the second
   crate path.
5. **`fetch_citation` wire shape undocumented.** The rustdoc says only "via the search
   module". Observed: `query=EXT_ID:32403408 AND SRC:MED`, `pageSize=1`, `format=json`,
   `resultType=core`, no `cursorMark`. Matched my guess; still a guess.
6. **Asymmetric identity arguments.** `fetch_citation(&RecordKey)` takes the key by
   reference, but `publication(source, id)` takes two values, so keeping a selected key
   meant `key.source.clone(), key.id.clone()` (or moving the key apart).
7. **Two shapes for one response.** `send()` gives `SearchPage { hit_count,
   next_cursor_mark, result_list.result }`; `pages()` gives `Page { meta, next, items }`.
   Not wrong, but the caller learns both vocabularies for the same document.
8. **Cap type vs SDK newtypes.** The page cap is a `std::num::NonZeroUsize`
   (extra import, extra `unwrap`), yet `pages()` still returns `Result` because 1..=20 is
   checked at run time (`21` → `InvalidInput`). The SDK otherwise uses validated newtypes
   (`PageSize`).
9. **SDK-detected repeat is reported as `PagesError::Provider`.** The repeated-cursor
   failure is reached through `err.provider_error()` → `Error::RepeatedCursor`; the variant
   name suggests the provider failed. The README's wording ("typed failure") does not say
   which `PagesError` variant carries it.
10. **`journal_title` is empty for citations and core records.** The field doc says "Journal
    title, if present", but a citation carries `journalAbbreviation` (webinar p. 36) and a
    core record carries `journalInfo.journal.title`; both decode to `None` while the
    provider did send a journal. Only lite search fills it. A consumer displaying the
    selected citation shows no journal without any error.
11. **`fetch_citation` ambiguity naming.** The rustdoc says "ambiguity or an identity
    mismatch is `UnexpectedRecord`". Two records come back as `InvalidPage("record count
    exceeds requested size, SDK cap or hit count")` because the lookup asks for one record;
    the ambiguous case that does give `UnexpectedRecord` is `hitCount: 2` with one record.
12. **Empty cursor lands in `Decode`.** README: "An empty cursor is invalid". Observed
    `Error::Call(Decode)` ("invalid XML cursor mark"; JSON likewise `Decode`), not
    `InvalidPage`, which is where other envelope contradictions go.
13. **Rustdoc redirect stubs (minor, reading-method specific).** `model/`, `error/`,
    `input/`, `pagination/`, `xml/` contain redirect stubs to the root items. `all.html`
    lists each type once, so a browser user is not affected; it only cost me empty pages
    because I read the HTML files from disk.

No source lookups: nothing under `src/`, `tests/`, `fixtures/`, `FRICTION.md` or
`PROVENANCE.md` was opened.

## Predictions vs observed

| Prediction | Observed | Verdict |
|---|---|---|
| JSON lite search target, parameter order `query, pageSize, cursorMark=*, format=json, resultType=lite` | exactly that | match |
| spaces `+`, `:` → `%3A`, `*` raw, `=` → `%3D` | `query=malaria+AND+OPEN_ACCESS%3Ay`, `cursorMark=*`, `cursorMark=AoIIQKsQtyg0MjQ5NDc4NA%3D%3D` | match |
| headers: `accept: application/json` / XML Accept, `host`, no UA/auth/cookie/referer | `accept`, `host` only; XML accept `application/xml` | match |
| citations `…/MED/20485428/citations?pageSize=1&format=json&page=1`; default pageSize 25 | same; default 25 | match |
| `fetch_citation` = core search for `EXT_ID … AND SRC …`, pageSize 1 or 2 | `EXT_ID:32403408 AND SRC:MED`, pageSize 1, no cursor | match |
| `journal_title == None` for citation and core | `None` for both | match (model gap, friction 10) |
| XML default at JSON op → `Metadata`, status 200 | `Metadata`, `Some(200)`; in a traversal `Provider` at `Start`, then `Ended` | match |
| XML labelled JSON / malformed JSON → `Decode`; > 256 KiB → `Limit` | same (JSON and XML) | match |
| 200 without `resultList` → `Decode` or `InvalidPage` | `Decode` | match |
| non-2xx → `Error::Http`, status, ≤16 KiB raw body, `retry_after` 120 s, no retry | same; 302 not followed | match |
| more records than requested → `InvalidPage` | `InvalidPage` | match |
| `fetch_citation`: 0 → `NotFound`; other id / two records → `UnexpectedRecord` | 0 → `NotFound`; other id/source and `hitCount` 2 → `UnexpectedRecord`; **two records → `InvalidPage`** | **mismatch** (friction 11) |
| empty `nextCursorMark` → `InvalidPage` or `Decode` | `Decode` | partial (friction 12) |
| local refusals → `InvalidInput`, no I/O (incl. cap 21) | same | match |
| XML chain `*` → c1 → c2 → end, `Ok(None)` sticky without request | same; `checkpoint()` `None` | match |
| repeated cursor → `Provider{RepeatedCursor}`, `failed = At(sent cursor)`, page not emitted, then `Ended`, no request | same (2 requests); two-page cycle too | match |
| `collect_complete` keeps the prefix in `Incomplete` | page 1's two items | match |
| cap → `Bounded{At(next)}`; resume sends that cursor first | same | match |
| empty page with fresh cursor continues | continues | match |
| DTD entity refused | `Decode` | match |
| JSON labelled JSON at XML op | `Metadata` (not predicted explicitly) | new observation |
| `Debug`/`Display` withhold query, cursor, body, title | no leak in error, client, scope, request, model | match |

Inferred risks, **not verified** (live calls NOT_RUN; the official pages I read show the
citations record only as XML):

- A temporary probe (removed afterwards; a copy of the test file before it is in
  `target/tmp/consumer/first_use.rs.bak`) fed a citations JSON with `"pubYear":2020` as a
  JSON **number**: the whole citations call failed with `Decode` ("the JSON does not match
  the expected type"). If the live JSON uses a number there, workflow 1 would fail at step 2.
  I did not find the JSON type in the documents I was allowed to read.
- The Solr-style end-of-listing question from the predictions is still open: if the service
  echoes the sent cursor on its last page, the SDK reports `RepeatedCursor` instead of
  completion. The prefix is preserved either way.

## What I would change (not done: the SDK's `src/` and shared crates are out of scope)

- SDK docs: state the pinned values (`format`, `resultType`, `page=1`) and the
  `fetch_citation` query on the endpoint/method docs; say which `Publication` fields each
  result type fills (or alias `journalAbbreviation` / `journalInfo.journal.title`); say that
  `RepeatedCursor` arrives as `PagesError::Provider` and an empty cursor as `Decode`; align
  the "ambiguity" sentence with the `InvalidPage` behaviour.
- SDK API: `Error::kind()` forwarding the Concord stage; `publication_key(&RecordKey)` or
  `From<RecordKey>` for the scope arguments.
- Concord generator (shared crate, recorded only): show constant query values and a decoder's
  `ACCEPT` value in generated endpoint docs; move `validate_header_names()` after the
  constructors (or hide it when it trivially returns `Ok(())`); shorten the repeated
  send/prepare paragraph.

## Assistance label

**docs-only** — README, the package manifest, generated rustdoc (this SDK, `concord`,
`concord-pagination`, `concord-example-testserver`) and the two official provider documents.
No SDK source, tests, fixtures, FRICTION.md or PROVENANCE.md were read.
