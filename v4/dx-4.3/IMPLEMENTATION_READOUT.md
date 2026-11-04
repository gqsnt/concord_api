# V4.3 candidate — developer readout

What changes in the code a Concord user writes. Every excerpt below is from a file in this branch
that compiles and is tested; paths are given with each one.

**Supported API changes: none.** No public item of `concord`, `concord-core`, `concord-extras` or
`concord-macros` was added, removed or changed; package versions stay 4.2.0. What is new is
maintained guidance and executable examples, plus one evaluated experiment that is not part of
the supported surface (generated input records, decision G1).

## 1. Streaming a request body with a format (corrected guidance)

Before (V4.2 `docs/CUSTOM_FORMATS.md` §1):

> Not a format: a media type that changes with every body (a multipart boundary), a streamed
> request body, a streamed response. Use the native exits (`into_parts`, `send_raw`).

After: a fixed-media request stream **is** a format. `examples/native/src/upload.rs`:

```rust
impl Encode<OneShot> for Streamed {
    const CONTENT_TYPE: Option<&'static str> = Some("application/octet-stream");

    fn encode(source: OneShot) -> Result<reqwest::Body, CodecError> {
        // Wraps, does not poll: the first poll happens when the request is executed.
        Ok(reqwest::Body::wrap_stream(source))
    }
}
```

and the declaration (`examples/native/src/api.rs`):

```rust
PUT upload(id: u64, body source: OneShot) -> UploadReceipt {
    path "items/{id}/upload";
    request upload::Streamed;
}
```

`OneShot` is not `Clone`; building the request, its `Debug`, `prepare()` and inspection poll it
zero times; one `send()` streams it once (`examples/native/tests/upload.rs`: exactly one chunked `PUT` whose body
is the source's 16 bytes, `alpha-beta-gamma`); a second upload needs a new source, and
`.clone()` on the request does not compile (doc test). The media type is static: a per-body
media type still takes the native multipart route, or a `content-type` header the SDK sets
deliberately. reqwest's `stream` feature is enabled by the example, never by Concord.

## 2. Domain values → service operations (new SDK/application pattern)

`examples/app/src/catalog_repr.rs` — a synchronous, fallible conversion that returns the
**generated operation**, so callers keep setters, `prepare` and `send`:

```rust
pub fn activity(
    api: &Catalog,
    id: AccountId,
    since: Option<Moment>,
) -> Result<CatalogActivity, CatalogReprError> {
    let since = since.map(self::since).transpose()?;
    Ok(api.activity(account(id)).maybe_since(since))
}
```

The same `Moment` becomes whole-second RFC 3339 for Catalog and exact milliseconds for Ledger.
`2026-01-15T10:00:00.125Z` is rejected by Catalog before any request (`FractionalSeconds`) while
Ledger still sends `from_ms=1768471200125` and answers; `…00.125000001Z` is rejected by both and
nothing is sent. The report attempts both services independently (`examples/app/src/report.rs`):

```rust
let catalog = Outcome::attempt(
    catalog_repr::activity(catalog, account, since),
    CatalogActivity::send,
)
.await;
let ledger = Outcome::attempt(
    ledger_repr::entries(ledger, account, since),
    LedgerEntries::send,
)
.await;
```

A named workflow where it carries an application intent (`examples/app/src/workflow.rs`), still
returning the ordinary operation:

```rust
/// The same listing without any tag filter: replaces the tags of **this request only** with
/// none, so no `tag` is sent. The shared handle keeps its defaults, and sibling requests made
/// from it still send the featured tags.
pub fn unfiltered_page(&self) -> CatalogTenantListingList {
    self.listing.list().set_tags(Vec::new())
}
```

One bounded traversal for both services (`examples/app/src/traversal.rs`): it returns
`Traversal::Complete` or `Traversal::Bounded { next, .. }`, resumes from `Some(next)`, and on a
failed page returns `Interrupted { error, records, pages, retry_from }` — the records collected
so far and the continuation that refetches the failed page.

## 3. Response metadata with the typed value (corrected recipe)

Before — the campaign's helper (`Pending::receive`, archived campaign file arms/amber/sdk/src/policy.rs) executed
internally, with no edit point, and returned reqwest's error:

```rust
pub async fn receive(prepared: Prepared<T>) -> Result<Self, reqwest::Error> {
    let (client, request, decoder) = prepared.into_parts();
    let response = client.execute(request).await?;
    Ok(Self { response, decoder })
}
```

(`reqwest::Error`'s `Display` prints the full request URL, sensitive query values included.)

After — `examples/native/src/metadata.rs`: the caller edits the native request, it is executed
once, selected head metadata is copied before the body is read, the endpoint's decoder does the
rest, and a transport failure goes through Concord's conversion:

```rust
let got = send_with_meta(api.item(7).prepare()?, &[ETAG], |request| {
    let trace = HeaderValue::from_static("trace-1");
    request.headers_mut().insert("x-trace", trace);
})
.await?;
let etag = got.meta.raw_header(ETAG); // deliberate, unredacted access
```

On a refused connection to `…/items/7?token=SYNTHETIC-SECRET`, the archived route's error prints
`error sending request for url (http://127.0.0.1:…/v1/items/7?token=SYNTHETIC-SECRET)`; the
recipe's prints `GET Inventory.item: transport error (connect)` and has no `source()` chain.
`Meta`/`WithMeta`/`MetaError` `Debug` show status, version and kept header names only. A
`Status`/`Metadata` failure still holds the unread response; after a body-read or decode failure
only the captured head metadata remains.

## 4. An operation the SDK does not declare, from another crate

Before — the campaign's suffix helper (archived campaign file arms/amber/consumer/src/external.rs) borrowed a
declared endpoint and edited it:

```rust
let (http, mut request, _detail_decoder) = items.detail(item_id).prepare()?.into_parts();
request.url_mut().path_segments_mut()
    .map_err(|_| std::io::Error::other("base URL cannot hold path segments"))?
    .push("archive");
// The seed detail decoder and its JSON Accept belong to another representation.
// This endpoint explicitly chooses Binary; the fixture does not negotiate it.
request.headers_mut().remove(concord::reqwest::header::ACCEPT);
let response = http.execute(request).await?;
```

It only reaches routes that extend a declared one, and its transport error is reqwest's own.

After — the SDK publishes a context (`examples/sdk/src/context.rs`), and an independent crate
(`examples/extension/src/lib.rs`) builds a route that is not a suffix of anything declared:

```rust
pub fn export_parts(
    tenant: &TenantContext,
    export: &str,
) -> Result<(reqwest::Client, reqwest::Request, Decoder<Vec<CsvRow>>), ExtensionError> {
    let mut request = tenant
        .request(Method::GET, &["exports", export])
        .map_err(|e| ExtensionError::invalid(EXPORT_OPERATION, e))?;
    request
        .headers_mut()
        .insert(ACCEPT, HeaderValue::from_static(TEXT_CSV));
    let decoder = Decoder::<Vec<CsvRow>>::new::<Csv>();
    Ok((tenant.http().clone(), request, decoder))
}
```

The tenant `acme/east?x=1%20+ü #` under base `…/edge/catalog/v1` goes out as
`/edge/catalog/v1/tenants/acme%2Feast%3Fx%3D1%2520%2B%C3%BC%20%23/exports/items-2026` — the same
segment bytes the generated `tenant.item(..)` sends. Errors name the extension's operation and
have `endpoint() == None` (no borrowed identity). Concord's core is unchanged.

## 5. Generated input records — evaluated experiment

See `experiments/generated-inputs/EVALUATION.md` and decision G1.
