# concord-sdk-datadog-events

A bounded SDK for the read side of the Datadog Events API (v2), built on
[Concord](../../README.md): list the events of a time range and inspect one, and search events
with a filter following the documented cursor pagination, on any Datadog site.

**Status.** Every request shape, response and failure is tested against a local stand-in
server (`cargo test -p concord-sdk-datadog-events`). The real Datadog API has **never** been
called by this crate or its tests (live calls: NOT_RUN). Sources and the derivation of every
fixture are in [`PROVENANCE.md`](PROVENANCE.md); the costs met while building it, in
[`FRICTION.md`](FRICTION.md).

## What the slice covers

| Operation | Request | Used by |
|---|---|---|
| Get a list of events | `GET /api/v2/events` with `filter[query]`, `filter[from]`, `filter[to]`, `sort`, `page[cursor]`, `page[limit]` | workflow 1 ([`DatadogEvents::list`]) |
| Search events | `POST /api/v2/events/search` with a JSON body (`filter`, `options`, `page`, `sort`) | workflow 2 ([`DatadogEvents::search_pages`]) |

- **Sites.** The host is chosen at run time from the caller's [`Site`] (`US1`, `US3`, `US5`,
  `EU`, `AP1`, `AP2`, `UK1`, `US1-FED`, `US2-FED`): `https://api.<DD_SITE>`, e.g.
  `https://api.datadoghq.eu`. [`Site::from_dd_site`] reads a `DD_SITE` value
  (`datadoghq.eu`).
- **Keys.** Every request carries the API key (`DD-API-KEY`) and the application key
  (`DD-APPLICATION-KEY`): Datadog requires an application key for every read. Both are checked
  when [`Credentials`] are built, flagged sensitive on the wire and never printed.
- **Pages.** One page is a [`EventsPage`]: its events (`data`), `links`, and `meta` (the next
  cursor `meta.page.after`, warnings, request id). Each [`Event`] exposes its id, message, tags
  and timestamp; its event-specific attributes (title, status, `evt`, monitor, ...) are raw
  JSON ([`Event::attribute`]): their schemas are outside this slice.

## Using it

```toml
[dependencies]
concord-sdk-datadog-events = { path = "../concord_api/examples/datadog-events-sdk" }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

### Construction

```rust
use concord_sdk_datadog_events::{Credentials, DatadogEvents, Site};

# fn main() -> Result<(), Box<dyn std::error::Error>> {
let site = Site::from_dd_site("datadoghq.eu")?;
// An empty or unprintable key is refused here, as an authentication error.
let credentials = Credentials::new("<DD_API_KEY>", "<DD_APP_KEY>")?;
// No redirects (the keys are custom headers reqwest would forward), no transport retries.
let http = DatadogEvents::http_client()?;
let events = DatadogEvents::new(http, site, credentials);
assert_eq!(events.site(), Site::Eu);
# Ok(())
# }
```

Bring your own `reqwest::Client` for timeouts, proxies or TLS settings; build it with
`redirect(reqwest::redirect::Policy::none())`, as [`DatadogEvents::http_client`] does, so that
a redirect never carries the keys to another host.

### Workflow 1: a bounded time range, then one event

```rust,no_run
use concord_sdk_datadog_events::{DatadogEvents, PageLimit, Sort, TimeRange};

async fn deploys(events: &DatadogEvents) -> Result<(), Box<dyn std::error::Error>> {
    // Milliseconds since the Unix epoch, `from <= to`.
    let range = TimeRange::from_millis(1_767_225_600_000, 1_767_229_200_000)?;
    let page = events
        .list(range) // the generated request: every setter stays available
        .set_query("service:checkout")
        .set_sort(Sort::Descending)
        .set_limit(PageLimit::new(50)?)
        .send()
        .await?;
    for warning in page.warnings() {
        eprintln!("partial results possible: {:?}", warning.code);
    }
    if let Some(first) = page.data.first().and_then(|e| e.id.clone()) {
        let event = page.event(&first).expect("the id came from this page");
        println!("{:?} {:?} {:?}", event.timestamp(), event.message(), event.tags());
        println!("title: {:?}", event.attribute("title"));
    }
    Ok(())
}
# fn main() {}
```

`list` returns one page. To continue it yourself, pass `page.next_cursor()` to `set_cursor`
with the same inputs.

### Workflow 2: a search, every page

```rust,no_run
use std::num::NonZeroUsize;

use concord_sdk_datadog_events::{DatadogEvents, PageLimit, SearchRequest, Sort};

async fn agent_restarts(events: &DatadogEvents) -> Result<(), Box<dyn std::error::Error>> {
    let request = SearchRequest::new()
        .query("source:datadog-agent")
        .from("now-15m")
        .to("now")
        .timezone("GMT")
        .sort(Sort::Ascending)
        .limit(PageLimit::new(100)?);
    let mut pages = events.search_pages(request).max_pages(NonZeroUsize::new(20).unwrap());
    // `Ok(None)` only after a page without `meta.page.after`.
    while let Some(page) = pages.next_page().await? {
        if !page.meta.warnings.is_empty() {
            eprintln!("page {:?} may be partial", page.meta.request_id);
        }
        for event in &page.items {
            println!("{:?} {:?}", event.timestamp(), event.message());
        }
    }
    Ok(())
}
# fn main() {}
```

Each page is the same body with `page.cursor` set to the previous page's `meta.page.after`,
which is what the documentation prescribes. A cap reached while pages remain is
`PagesError::Bounded`, never a silent end; `PagesError::resume()` (or `checkpoint()`) gives the
cursor to continue with [`DatadogEvents::search_pages_from`]. `items().collect_complete()`
returns every event or an `Incomplete` holding the events collected so far and the error; note
that `items()` drops page metadata, warnings included.

## Failure semantics

Every generated `send()` returns `Result<_, `[`Error`]`>`; `prepare()` and `with_base_url`
return `concord::Error` (re-exported as [`concord`]).

| What happened | Error | Was anything sent? |
|---|---|---|
| API key or application key empty, or not sendable as a header | [`AuthError::MissingApiKey`], [`AuthError::MissingApplicationKey`], [`AuthError::MalformedKey`] (from [`Credentials::new`]; `Error::Authentication` after `?`) | no |
| 401 or 403: wrong or revoked keys, keys of **another site** (wrong region), missing `events_read` permission | `Error::Authentication(AuthError::Rejected)`, naming the API host that answered and its site | yes |
| 400 Bad Request | `Error::BadRequest` | yes |
| 429 Too many requests | `Error::RateLimited` (the head is kept for its rate headers; nothing is retried) | yes |
| any other non-2xx, including a 3xx when the transport does not follow redirects (as [`DatadogEvents::http_client`]) | `Error::Provider` | yes |
| 2xx not labelled JSON, malformed or truncated JSON, a body over 8 MiB, a connection dropped mid-body | `Error::Concord` with kind `Metadata`, `Decode`, `Limit` or `Transport`, and the response head | yes |
| no answer at all | `Error::Concord`, kind `Transport`, no head | unknown |
| a search page whose `meta.page.after` is empty, or a cursor this traversal already requested | `Error::Protocol` (the traversal ends instead of restarting or looping) | yes |

For every non-2xx status the status decides the class, even when the error document
(`{"errors": [...]}`, at most 64 KiB, strict JSON) is absent, malformed, oversized or not JSON;
[`ProviderError::document`] says which. Datadog cannot tell a wrong region from a wrong key:
both are a 403, so the authentication error names the host (and site) that refused and asks
to check the site and both keys.

**Secrets.** No `Display` or `Debug` of this crate (client, credentials, prepared request,
errors) contains either key. Provider error messages are untrusted text and are not printed
either; read them deliberately with [`ProviderError::messages`]. Native reqwest values
(`prepared.request()`, `ProviderError::meta().headers()`) are reqwest's and Concord's
documented boundary: header values Concord flagged sensitive show as `Sensitive` there.

## What this SDK does not do

- No retries, no rate limiting, no backoff: a 429 is returned with its head. The rate-limit
  headers are not documented on the pages this slice cites, so the SDK does not interpret them.
- `links.next` is kept but never requested (the documented example points at
  `app.datadoghq.com`, not the API host); the SDK follows `meta.page.after`.
- Warnings never fail a page; they are returned with it.
- Bearer-token authentication (the `Authorization: Bearer` of the documentation's curl
  examples), OAuth and key management are not supported: this slice uses API and application
  keys.
- **Excluded from the slice:** metric submission, monitor lifecycle and all event schemas (the
  event-specific attributes stay raw JSON).
- **Not covered either:** posting events, the v1 events API, and any other Datadog API.

## Testing

```sh
cargo test --locked -p concord-sdk-datadog-events
```

`tests/protocol.rs` checks wire shapes (hosts per site, query keys, both key headers, the
documented search bodies), every documented status and its error body, unusable bodies and
redaction; `tests/workflows.rs` runs both workflows end to end and the required failure
(wrong-region keys, missing application key). The stand-in is
[`concord-example-testserver`](../testserver), a loopback HTTP/1.1 server; no TLS, HTTP/2 or
real Datadog behaviour is exercised.
