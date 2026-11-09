# Europe PMC bounded SDK

The declaration owns the provider error document: `error concord::bytes::Bytes in concord::Binary;`. Native prepared/decoded
paths carry that document as `D` in `Prepared<T, D>` and `concord::Error<D>`; SDK status routing
and validation remain in the policy. Concord withholds document contents in formatting.

Package `concord-sdk-europe-pmc`, at Concord baseline
`c3fd4f1ee30c1727e26b285270f2cc8d8b694e77`. **Live provider calls: NOT_RUN.**
All executed workflows use `concord-example-testserver` on loopback.
This is **example-assisted**; the exact direct and indirect example reads are in
[PROVENANCE.md](PROVENANCE.md) and [FRICTION.md](FRICTION.md).

This slice supports lite publication searches in JSON and XML, opaque search cursors,
the first citations page for a selected publication, and a core metadata lookup for
one selected citation, plus a page-number traversal of one publication's reference list
in JSON or XML. The hierarchy is generated client → search/publication scope (→ references
scope) → endpoint. The JSON workflow searches, selects a publication, requests one citing
publication, then fetches that citation's core metadata. The XML workflow repeats the
same bounded search and continues using the returned cursor.

The public service supports both formats and defaults to XML. The declaration pins
`format=json` or `format=xml` on each operation, along with its response decoder.
See the [REST documentation](https://dev.europepmc.org/RestfulWebService) and
[official webinar](https://www.ebi.ac.uk/training/online/courses/embl-ebi-programmatically/wp-content/uploads/sites/128/2020/11/Webinar-slides-Europe-PMC-programmatically_2020.pdf)
(pp. 13, 32, 35–44), read **2026-11-07**. The references module and its envelope are recorded
in [MAINTENANCE.md](MAINTENANCE.md).

## Calling it

The snippets compile as doctests. Supply the full local fixture base, such as
`http://127.0.0.1:PORT/rest`; the actual test suite creates that server itself.
The package depends on the local Concord checkout by path and is not published.

```no_run
use concord_sdk_europe_pmc::{
    EuropePmc, Query, PageSize, SearchFormat, http_client,
    concord_pagination::Resume,
};
use std::num::NonZeroUsize;

async fn connected_workflows(local_fixture_base: &str)
    -> Result<(), Box<dyn std::error::Error>>
{
    let api = EuropePmc::new(http_client()?).with_base_url(local_fixture_base)?;
    let search = api.search(Query::new("malaria")?, PageSize::new(2)?);
    let cap = NonZeroUsize::new(2).unwrap();

    let mut json = search.pages(SearchFormat::Json, Resume::Start, cap)?;
    if let Some(page) = json.next_page().await? {
        if let Some(selected) = page.items.first() {
            let key = selected.key();
            let citing = api.publication(key.source, key.id).citations()
                .set_page_size(PageSize::new(1)?).send().await?;
            if let Some(selected_citation) = citing.citation_list.citation.first() {
                let citation = api.fetch_citation(&selected_citation.key()).await?;
                assert_eq!(citation.key(), selected_citation.key());
            }
        }
    }

    let mut xml = search.pages(SearchFormat::Xml, Resume::Start, cap)?;
    while let Some(page) = xml.next_page().await? {
        // Page metadata is the provider's hit count; values stay in page.items.
        let _ = (page.items, page.meta);
    }
    Ok(())
}
```

A publication's reference list (`GET /{source}/{id}/references`) is read page by page at one
page size, in the same two modes. References the provider did not match to a record have no
`key()`; matched ones can be passed to `fetch_citation` or `publication(..)`:

```no_run
use concord_sdk_europe_pmc::{
    EuropePmc, PageSize, RecordKey, SearchFormat, concord_pagination::Resume,
};
use std::num::NonZeroUsize;

async fn reference_list(api: &EuropePmc, key: RecordKey)
    -> Result<(), Box<dyn std::error::Error>>
{
    let references = api.publication(key.source, key.id).references(PageSize::new(25)?);
    let cap = NonZeroUsize::new(4).unwrap();
    match references.pages(SearchFormat::Xml, Resume::Start, cap)?.items().collect_complete().await {
        Ok(all) => {
            for matched in all.iter().filter_map(|r| r.key()) {
                let _record = api.fetch_citation(&matched).await?;
            }
        }
        // A cap or a failed page: completed pages are kept, `resume()` is the page to request.
        Err(incomplete) => {
            let (_kept, _next) = (incomplete.collected, incomplete.error.resume().cloned());
        }
    }
    Ok(())
}
```

For a single page, use `search.json().send().await`,
`search.xml().set_cursor(cursor).send().await` or `references.json().set_page(n).send().await`. `prepare()` builds a request without I/O.
Every generated `send()` uses `SendPolicy`; the SDK performs one execution and never
retries. The provided transport disables redirects/native retries, suppresses Referer,
uses no proxy and times out the entire request after ten seconds. A caller-supplied
transport controls those behaviors itself. No credentials or authentication flow exist
in this slice.

## Bounds and continuation

- Page size is 1..=100 (SDK policy); the default is 25. Every response is checked against
  the requested record count and the SDK cap. A traversal requires a 1..=20 page budget.
- Success documents are at most 256 KiB; HTTP diagnostic bodies are at most 16 KiB.
  XML additionally permits at most 20,000 DOM nodes, uses UTF-8 and resolves no entities
  from external resources. DTD entity declarations are refused.
- `Ok(None)` from `next_page()` means a page without a next cursor was consumed.
  Short/empty pages with fresh cursors continue. An empty cursor is invalid.
- Every repeated cursor is a typed failure, even on an empty page. The offending page
  is not emitted and no further request is admitted. This includes longer cycles.
- A page cap produces `PagesError::Bounded`, never success. `resume()` identifies the
  unrequested page; use it with the same base, query, page size and format. The shared
  paginator cannot enforce that association. Cursor history resets on a new traversal.
- `items().collect_complete()` preserves the successful prefix in `Incomplete` on a bound
  or failure. After a failure/bound, further reads return `PagesError::Ended`.
  The hit count is metadata, not a promise of a stable snapshot or a manufactured cursor.
- A references traversal sends `page` = 1, 2, … with the scope's one `pageSize` and stops at
  the page that reaches the hit count, so the last page needs no extra request. Every earlier
  page must be full and the last must hold exactly the remainder; a short, empty, overfull
  or beyond-the-end page, or a hit count that differs from the traversal's first page, is
  `Error::InvalidPage` (reached via `PagesError::Provider`, like `RepeatedCursor`) and is not
  emitted. A bound or failure resumes at a `PageNumber`; resume with the same publication,
  page size and format (not checked across runs). A missing `referenceList` reads as empty;
  an empty page is accepted only where no records remain (for example a zero hit count).

For both `pages` methods, use `concord_sdk_europe_pmc::concord_pagination::Resume` for the
position. Search `Resume::Start` repeats the first search page with `cursorMark=*`;
`Resume::At(key)` takes the `Cursor` from `nextCursorMark` in a returned page's `next`.
References `Resume::Start` repeats page 1 of the publication's references; `Resume::At(key)`
takes the `PageNumber` in a returned page's `next`.

## Failure semantics

`Error::InvalidInput` refuses local inputs before I/O. `Error::Call` keeps Concord's
`Build`, `Transport`, `Metadata`, `Decode` and `Limit` stages. A default XML response
labelled XML is rejected at the JSON media check before reading its body. XML falsely
labelled JSON fails JSON decoding; it is never reinterpreted as a JSON success.
Malformed, truncated, unsupported-media and oversized responses fail explicitly.
`status()` also retains the status on mid-body and decode failures.

`Error::Http` covers every non-2xx, including redirects, 4xx and 5xx. It retains the
status even when reading its body fails. Its diagnostic bytes have no assumed JSON/XML
error schema. `retry_after(now)` uses Concord's shared strict parser; absence, invalid,
repeated and overflow values remain distinct. There is no automatic sleep or replay and
no invented provider quota. `Error::InvalidPage` rejects contradictory/excess records.
`NotFound` and `UnexpectedRecord` distinguish an absent exact citation from a mismatched
identity or count; no fuzzy result is substituted.

Generated handles, prepared requests and SDK diagnostics withhold search expressions,
cursors, header values and body content. Model `Debug` also withholds publication text.
`Query`/`Cursor`/identity `Display` is deliberately the **wire spelling**, required by
Concord; use `Debug` for logging. Native URL/request access, `as_str()`, HTTP raw body and
header accessors explicitly expose data. Error `source()` does not forward unredacted
errors. No credentials are invented just to test redaction.

## Exclusions and validation

No full-text PDF, full-text XML retrieval, annotation/text-mining APIs, harvesting,
reference-network traversal (following references beyond one publication's list) or
citations paging beyond page 1. Core results expose a
small typed subset (including optional abstract); unknown fields are tolerated.
Tests establish fixture behavior, not current service availability or schema conformance.

```sh
CARGO_BUILD_JOBS=4 cargo test --locked -p concord-sdk-europe-pmc
CARGO_BUILD_JOBS=4 cargo clippy --locked -p concord-sdk-europe-pmc --all-targets -- -D warnings
CARGO_BUILD_JOBS=4 RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord-sdk-europe-pmc --no-deps
```

See [PROVENANCE.md](PROVENANCE.md) for derivations and commands and
[FRICTION.md](FRICTION.md) for the bounded SDK's remaining Concord costs.
