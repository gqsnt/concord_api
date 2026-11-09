# MediaWiki Action API SDK39

The declaration owns the provider error document: `error crate::error::ErrorDocument in concord::Json;`. Native prepared/decoded
paths carry that document as `D` in `Prepared<T, D>` and `concord::Error<D>`; SDK status routing
and validation remain in the policy. Concord withholds document contents in formatting.

Package: `concord-sdk-mediawiki-action-api`. A small, example-assisted public-data SDK
at Concord baseline `c3fd4f1ee30c1727e26b285270f2cc8d8b694e77`.
Live provider calls: **NOT_RUN**. Every executed workflow uses `examples/testserver` on loopback.

The declaration follows `MediaWiki` → `query()` → `search`, `content`, `contents`,
`category_members`. It supports modern `formatversion=2` JSON on one configured wiki: search for
page identities, choose a hit and fetch its latest revision's raw main-slot content; fetch the
content of several page IDs in one batch; or traverse category members with a required
page-request bound. A search page can also be continued explicitly.
Authenticated edits, Wikibase, wikitext parsing/rendering, legacy `query-continue`, generators,
and global cross-wiki discovery are excluded.

## Calling the slice

The caller supplies an identifying User-Agent, including its application identity and contact.
`with_base_url` takes the **complete script URL**, for example a local fixture's
`http://127.0.0.1:PORT/w/api.php`. Its path is retained. The declaration's default script is
English Wikipedia; it was never contacted during authoring or testing.

```no_run
use concord_sdk_mediawiki_action_api::{MediaWiki, MediaWikiConfig, concord::reqwest};
use std::{num::NonZeroUsize, time::Duration};

async fn browse_fixture(script_url: &str) -> Result<(), Box<dyn std::error::Error>> {
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .timeout(Duration::from_secs(30))
        .build()?;
    let api = concord::BaseUrl::parse((script_url).as_ref()).map(|origin| MediaWiki::new(http, MediaWikiConfig { origin: origin,
        user_agent: "MyWikiReader/1.0 (contact: maintainer@example.invalid)".into(),
    }))?;
    let query = api.query();

    let found = query.search("Rust programming language").set_srlimit(10).send().await?;
    // The caller chooses a hit. This example chooses the first; tests also choose the second.
    if let Some(chosen) = found.data.search.first() {
        let page = query.fetch_page(chosen.pageid).await?;
        println!("{}: {} bytes at revision {}", page.title, page.content.len(), page.revision);
    }
    // Search's continuation includes the generic field and may include a numeric offset.
    if let Some(next) = found.continuation {
        let _next_search_page = query.search("Rust programming language")
            .set_srlimit(10).set_continuation(next).send().await?;
    }

    let members = query.category_pages(
        "Category:Programming languages", 50, None, NonZeroUsize::new(100).unwrap(),
    ).items().collect_complete().await?;
    println!("{} members", members.len());
    Ok(())
}
```

`collect_complete()` returns all members only on provider exhaustion. On a bound or failure,
its `Incomplete` contains the collected prefix and a typed `PagesError` with the resume/failed
position. Obtain the complete key using `error.resume().and_then(|at| at.key()).cloned()`;
pass it as `from` to `category_pages` with the same wiki, category and page size. Preserve the
prefix according to the application's needs. For incremental handling use `next_page()`;
`Ok(None)` means exhaustion, while subsequent calls after a bound or failure return `Ended`.
There is no automatic deduplication or snapshot guarantee for a changing category.

Every field of the response's `continue` object is retained and emitted once on the next request.
The generic `continue` field is mandatory in a present token. Empty pages still advance when a
token is present. `batchcomplete` does not establish traversal completion. These rules follow
[API:Continue/en](https://www.mediawiki.org/wiki/API:Continue/en) and
[API:Query](https://www.mediawiki.org/wiki/API:Query), read 2026-11-07.

## Fetching several pages at once

`fetch_pages` answers every requested page ID, in the caller's order, with either its
`PageContent` or a typed `Unavailable` reason: `Missing`, `Invalid` or `NoContent` (no revision,
or hidden/absent main-slot content). Build the IDs with `PageIds::new(ids, limit)`: API:Query
allows 50 page IDs per query, 500 for clients the wiki grants higher limits
(`PageIdLimit::HighLimits`; this SDK sends no credentials). More IDs are refused with
`TooManyPageIds` before anything is sent; chunk longer lists by `PageIdLimit::max()`. A repeated
ID is sent once and answered at each position. An empty list answers nothing without a request.

```no_run
# use concord_sdk_mediawiki_action_api::{MediaWikiQuery, PageIdLimit, PageIds, Unavailable};
# async fn read(query: &MediaWikiQuery) -> Result<(), Box<dyn std::error::Error>> {
let ids = PageIds::new([12, 404, 7], PageIdLimit::Standard)?;
for answer in query.fetch_pages(&ids).await? {
    match answer.content {
        Ok(page) => println!("{}: {} bytes", page.title, page.content.len()),
        Err(Unavailable::Missing) => println!("#{} no longer exists", answer.pageid),
        Err(reason) => println!("#{}: {reason}", answer.pageid),
    }
}
# Ok(()) }
```

The batch omits `rvlimit`, which API:Revisions allows only for a single page. A large result is
continued: the next request carries the whole `continue` object, and the next response lists
the same pages with the next properties (API:Continue). A page listed without a revision is
therefore answered only by a later response, or as `NoContent` by the final one. Every continued
response must answer at least one new ID, so a batch takes at most one request per distinct ID.
Any failure of one response fails the whole batch; there are no partial batch results. Like every
query, a response carrying warnings is refused (`Warnings`), also in the middle of a batch. Batch bodies are bounded
by `BATCH_BODY_LIMIT` (16 MiB) instead of the 1 MiB used elsewhere, because one response carries
up to 500 pages' content until the wiki's own result-size limit continues it. Sources:
[API:Query](https://www.mediawiki.org/wiki/API:Query),
[API:Revisions](https://www.mediawiki.org/wiki/API:Revisions) and
[API:Continue/en](https://www.mediawiki.org/wiki/API:Continue/en), read 2026-11-07.

## Failure semantics

Generated `send()` uses `QueryPolicy`, returning `Result<QueryResponse<T>, Error>`:

| Error | Meaning |
|---|---|
| `Concord` | Build failure before dispatch; transport failure before a head or during body reading; strict JSON media refusal; malformed/truncated/wrong-shape JSON; oversized body. Inspect `kind()`, `head()` and `operation()` on the contained Concord error. |
| `Http` | Every non-2xx, including 429 and 5xx. The status/head stay authoritative beside the typed error document or its metadata/decode/limit/transport failure. |
| `Api` | MediaWiki error envelope inside a 2xx; explicit `problem.code` and `problem.info` are available. |
| `Warnings` | A non-null warnings envelope is refused because results might be partial. |
| `InvalidResponse` | Missing query, unexpected content page/revision count or content continuation, content without a title. Batch: a row without a requested page ID, a repeated row or second revision for one ID, a continued response that answers no new ID, or an ID never answered. |
| `ContentUnavailable` | `fetch_page` only: missing/invalid/unexpected page ID (also one listed without a title), no revision ID, or absent/suppressed main-slot content. Empty content is valid. `fetch_pages` reports these per ID as `Unavailable`. |

Success bodies are bounded at 1 MiB (batch content: 16 MiB) and non-2xx document attempts at
64 KiB. Missing, null, empty, non-scalar, non-integer numeric or fixed-input-colliding
continuation data is rejected when a continuation object is present. An absent token is valid exhaustion. Unknown fields in
ordinary response models are ignored to allow additive provider changes. Errors have no source
chain; explicit variants expose the unredacted detail when needed.

The SDK performs one HTTP execution per generated send. Configure the supplied transport with
redirects and native retries disabled, as above, for one wire request per call. It has no auth,
retry loop, automatic backoff or provider quota assumption. On 429, inspect
`HttpFailure.meta.headers()` (including `Retry-After`) and decide when to try again.

User-Agent values are withheld in generated Debug output. API error fields, cookie headers and
body bytes are withheld in error Debug/Display. Query terms, titles and public continuation
pairs are public data; prepared URLs can show them. `Continuation` itself prints only a field
count. Explicit native requests/responses, header/body accessors and application models are
unredacted. Do not put credentials in this public-data client.

`prepare()` remains Concord's native escape: `prepare()?.send()` enforces HTTP status, strict
JSON and the declared body bound, but does **not** apply `QueryPolicy` or project its envelope.
Normal SDK calls and pagination use generated `send()`. There is no SDK production request
mutation; the regression test alone deliberately mutates a native request to drop a field.

## Evidence

`tests/workflows.rs` exercises search → chosen content, numeric search continuation, complete
category traversal through an empty page, bounded resume, page failure, and the required broken
consumer control. In the synthetic control, dropping only generic `continue` repeats page 1;
the correct adapter returns IDs `[1, 2, 3]` in three calls. This fixture does not claim identical
failure behavior for every deployed MediaWiki version.

`tests/batch.rs` checks the batch wire shape (distinct IDs, no `rvlimit`, the whole
continuation), the continued merge in caller order, every fail-closed row/response case, the
page-ID limits and the batch body bound; `tests/batch_consumer.rs` is a consumer application of
the batch. `MAINTENANCE.md` records that change.

`tests/protocol.rs` checks the exact query/header shapes, every SDK error variant, malformed,
truncated, empty, oversized and wrong-media bodies, interrupted HTTP framing, invalid tokens,
suppressed/missing content and secret-safe diagnostics. `PROVENANCE.md` records all sources and
synthetic fixtures; `FRICTION.md` separates provider work from generic costs and records examples.

```sh
CARGO_BUILD_JOBS=4 cargo test --locked -p concord-sdk-mediawiki-action-api
CARGO_BUILD_JOBS=4 cargo clippy --locked -p concord-sdk-mediawiki-action-api --all-targets -- -D warnings
CARGO_BUILD_JOBS=4 RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord-sdk-mediawiki-action-api --no-deps
```
