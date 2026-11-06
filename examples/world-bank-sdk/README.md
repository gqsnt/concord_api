# World Bank Indicators SDK

An unauthenticated, small World Bank Indicators API **v2** client for a country's indicator in a single year or an inclusive range of years. It uses the JSON metadata/data array, preserves nullable observation values and traverses numbered pages. This is not a general World Bank SDK: SDMX, XML, metadata catalog queries, other date frequencies, retries, and economic interpretation are out of scope. Fixtures are synthetic offline contract examples, not vendor conformance tests; see [PROVENANCE.md](PROVENANCE.md).

Construct `Client` with a caller-configured `concord::reqwest::Client`; no credentials or configuration record are required. The transport controls timeouts, proxies, TLS, redirects and default headers. `with_base_url` can replace the default `https://api.worldbank.org/v2`, including its path, for offline tests. IDs are each encoded as one path segment; this slice expects one country and one indicator, and leaves their validity to the provider.

## One year

```no_run
use concord::reqwest;
use concord_sdk_world_bank::Client;
# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let http = reqwest::Client::builder().build()?;
let client = Client::new(http);
let page = client.year("br", "SP.POP.TOTL", 2006).await?;
for observation in &page.observations {
    println!("{}: {:?}", observation.date, observation.value);
}
// page.metadata and page.response preserve provider and HTTP metadata.
# Ok(()) }
```

`None` means JSON null, and `Some(0.0)` means a real numeric zero. A missing `value` field is rejected. A `null` data array is represented by an empty observations vector. `year` returns the first page, including its metadata; if it indicates further pages, use the range traversal (a range can have the same start and end year).

## Inclusive range and completion

```no_run
use concord::reqwest;
use concord_sdk_world_bank::{Client, concord_pagination::Resume};
use std::num::{NonZeroU32, NonZeroUsize};
# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let client = Client::new(reqwest::Client::new());
let pages = client.range("br", "SP.POP.TOTL", 2000, 2006,
    NonZeroU32::new(25).unwrap(), Resume::Start)?
    .max_pages(NonZeroUsize::new(100).unwrap());
match pages.collect_complete().await {
    Ok(pages) => { for page in pages { println!("{} items", page.items.len()); } }
    Err(incomplete) => {
        // incomplete.collected contains the successfully decoded prefix;
        // incomplete.error.resume() identifies the failed or unread page.
        eprintln!("{} pages read: {}", incomplete.collected.len(), incomplete.error);
    }
}
# Ok(()) }
```

`next_page()` is lazy and returns `Ok(None)` only after the last indicated page; a page whose values are all null, or whose data array is null, is not an end. Observations keep the `None`/`Some(0.0)` distinction on every page. Each `Page` contains observations, the next page number and `TraversalMeta` with the provider metadata and HTTP response head. `items()` drops page metadata and exposes `next`, `collect_complete`, and `into_stream`; the stream emits an error before ending. `collect_complete` returns an `Incomplete` with the prefix and `PagesError` on a cap or failure; a subsequent call after such an error remains an error. A cap that lands on the final page is complete. Resume with `Resume::At(n)` and exactly the same query and page size. Dropping a traversal cancels pending local work; no retry or request is spawned in the background. Each page is buffered and decoded up to 8 MiB; item streaming is across pages, not incremental JSON parsing. The source may change between requests: pagination does not promise a snapshot, compare totals or deduplicate records.

The lower-level `indicator(country, indicator, date)` generated operation also allows setting `page` and `per_page`, and exposes `prepare()` for inspecting the native request. It accepts an unvalidated date string; prefer `year` and `range` for the workflows above. `range` rejects reversed ranges and takes a nonzero page size. Numeric values use `f64`, so exact decimal arithmetic is not guaranteed.

## Errors

`Error::Provider` carries typed provider messages and the actual response metadata, including for a provider error envelope sent with HTTP 200. `Error::Http` retains the HTTP head and a diagnostic if a non-success response has an unavailable, oversized, non-JSON, malformed or unexpected body. `Error::Concord` keeps build, transport, content-type, size and decode errors plus a head if received. `Error::InvalidPage` rejects invalid or mismatched page metadata; `Error::InvalidRange` is local. The response head includes status, headers, HTTP version and Concord operation. `prepare()` and `with_base_url()` return Concord errors directly. Neither error nor successful response promises a retry or a partial JSON page.
