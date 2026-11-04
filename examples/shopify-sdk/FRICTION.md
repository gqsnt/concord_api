# Friction log — Shopify SDK and its two workflows

Written by an SDK author who did not design Concord's pagination or streaming APIs. Each entry:
the provider contract, the public Concord path used, the extra code, and whether it is
intentional flexibility or friction. No library crate was edited. Entries marked **proposal**
were sent to the coordinator.

## F1 — GraphQL variables cannot be Concord values (friction, by design)

**Provider:** one `POST graphql.json`; the request differs only in `query`, `operationName` and
`variables`, where an omitted variable and an explicit `null` mean different things.

**Concord path:** a value can be placed in a path, query, header or template, never inside a
body; a body is one typed value. So each operation's variables are a body type, and the SDK
writes the GraphQL request format itself (`graphql::GraphQl`, an `Encode<O: Operation>`):

```rust
POST products(body variables: ProductsQuery) -> Answer<ProductsQuery>;

#[derive(Serialize)]
pub struct ProductsQuery {
    pub first: PageSize,
    #[serde(skip_serializing_if = "Var::is_absent")]
    pub after: Var<Cursor>,          // Absent | Null | Value(cursor)
    ...
}
```

**Extra code:** `Var<T>` (Concord's own absent/present/`maybe_x` model stops at the body), a
builder per variables type, and the encoder (~40 lines). The generated request has only
`set_variables(..)`, which replaces the whole object, not one setter per variable.

**Assessment:** intentional (bodies are typed values; Concord does not interpret them) and the
open `Encode` trait made it clean. A recipe "GraphQL over one endpoint" (format + `Var`) in
`docs/RECIPES.md` would save the next author the design time; no core change proposed.

## F2 — An endpoint's output repeats its operation (resolved with a type alias)

**Provider:** the type of `data` is fixed by the document.

**Concord path:** the output is written per endpoint. The first version restated it
(`-> GraphqlResponse<ProductsData>`), so a mismatched pair compiled and failed only when decoding.
The macro accepts a projection in the output type, so the SDK derives it from the operation:

```rust
pub type Answer<O> = GraphqlResponse<<O as Operation>::Data>;
POST products(body variables: ProductsQuery) -> Answer<ProductsQuery>;
```

**Assessment:** flexibility that works but is not documented (the guides only show plain paths
as outputs). A mismatched name is still possible, but now visible on one line.

## F3 — Reading a response header before decoding (intentional, documented route)

**Provider:** `X-Shopify-API-Version` says which version served the request; a retired version is
silently served by a newer one.

**Concord path:** `send()` exposes no head, so `ShopContext::send` uses the native recipe:

```rust
let (client, request, decoder) = prepared.map_err(RequestError::Concord)?.into_parts();
let response = match client.execute(request).await {
    Ok(response) => response,
    Err(e) => return decoder.finish(Err(e)).await.map_err(RequestError::Concord),
};
// check x-shopify-api-version on a 2xx, then:
decoder.decode(response).await.map_err(RequestError::Concord)
```

**Extra code:** ~35 lines. Two small awkward points: the transport error must be converted
through `decoder.finish(Err(e))` (there is no `Error::from` for an unlabeled caller), and because
generated requests share no trait, the helper takes the `prepare()` result:
`shop.send(shop.admin().products(query).prepare())`.

**Assessment:** intentional flexibility; the recipe exists and worked first time.

## F4 — The signed result download is a native request (intentional; one sharp edge)

**Provider:** `url` / `partialDataUrl` are opaque signed URLs on another host
(`storage.googleapis.com` in the docs); the query string is the signature.

**Concord path:** `with_base_url` refuses a query, and an opaque absolute URL is not a declared
endpoint, so `ShopContext::result_download` builds `reqwest::Request::new(GET, url)` (no token:
the shop scope's header is never on it) and the workflow opens the response with
`JsonLinesReader::open_for`.

**Sharp edge:** a failure of `client.execute(..)` is a raw `reqwest::Error` whose `Display` prints
the full URL — the signature. The workflow must remember `error.without_url()`
(`bulk_export.rs`, `DownloadFailed`). Body-read failures are already safe (the reader's
`ByteSource` strips the URL). **Proposal:** a reader entry that takes the execute result,
`open_for_result(Result<reqwest::Response, reqwest::Error>, operation)`, returning an error type
that also covers "no response", so the URL cannot leak by omission.

## F5 — One explicit loop per traversal (mild friction)

**Concord path:** `concord_pagination::sequential` with the SDK adapter
`connection_page(shop.send(prepared), cursor.as_ref(), |d| Some(d.products))`. SH-A has six
chains (four top level, two nested kinds), each written as:

```rust
let mut pages = sequential(Resume::Start, request.pages_per_traversal, |cursor: Option<Cursor>| {
    let query = ProductsQuery::new(sizes.products, sizes.variants).filter(filter).after(cursor.clone());
    let prepared = shop.admin().products(query).prepare();
    fetch(shop, prepared, cursor, policy, |d| Some(d.products))
});
let mut failure = None;
while let Some(result) = pages.next().await {
    match result { Ok(mut page) => { /* record meta, consume items */ } Err(e) => { /* keep e */ } }
}
TraversalSummary::new(pages.finish(), failure)
```

A shared "drive" helper would have to name `Sequential<K, T, M, E, F, Fut>` with its bounds and
take an async per-page callback needing `&mut state` while the factory borrows the context; six
~25-line loops were clearer. The crate's choices that made the rest easy: no `'static`/`Send`,
the factory borrows the shop context, `finish()` returns the failed and resume identities, keys
are just my `Cursor` type.

**Assessment:** acceptable; a `try_for_each_page`-style helper could remove the repetition but
is not required.

## F6 — Nested connections have no composite checkpoint (intentional)

A product page is emitted (and the outer checkpoint moves past it) before that product's nested
variant traversal runs. If the nested chain is bounded or fails, resuming the outer chain alone
would skip its remaining variants. SH-A therefore records nested resume points separately
(`ReconcileReport::variant_pages.incomplete`: parent id + `TraversalSummary` with its own
`resume`). The crate is per chain by design; this is workflow state.

## F7 — A JSONL read error does not say whether it ended the stream (friction; caused a defect)

**Concord path:** `RecordErrors::Yield` with `MalformedLines::Skip { max }`: the workflow skips
`Decode`/`Format` errors and continues. Per `DECISIONS.md` JSONL-04 a malformed line is isolated,
but a refused byte-order mark is a **terminal** `Format` error — and `ReadError` carries no flag
for that. My first version counted the BOM error as skipped, the reader then returned `None`
(fused), and the export was reported **`Complete` with zero records**. Reproduction (fails on the
first version, passes now):
`cargo test -p concord-example-shopify-app --test sh_b_bulk_export sh_b_a_skipped_error_right_before_the_end_is_never_reported_as_complete`.

**Workaround (in `bulk_export.rs`):** an error stays "unconfirmed" until a later record proves the
reader went on; an end right after a skipped error is reported as `Interrupted` by that error. A
malformed *final* line is therefore reported as interrupted too (conservative, documented).
**Proposal:** `ReadError::is_terminal()` (or `JsonLinesRecords::ended_cleanly()`), so a caller
can tell a clean end from a fused one.

## F8 — Heterogeneous JSONL lines (intentional flexibility)

Product and variant lines share one file. `BulkRecord` deserializes each line into the union of
the selection's fields, then `TryFrom` picks the object type from the global id and checks that
type's required fields; violations are `serde::de::Error::custom` → `ReadError` of kind `Decode`
(isolable with `Yield`). ~120 lines, no `serde_json::Value`. The reader needed nothing more.

## F9 — Small notes (no friction)

- `#[sensitive]` on a scope value placed under a vendor header worked as documented
  (`x-shopify-access-token` is not on the well-known list).
- `subdomain "{shop}"` plus `ClientBuilder::resolve` and a base URL with the port is the documented
  way to test real host names; it also makes the two-shop leak test meaningful (distinct `Host`).
- `MediaPolicy::any()` for the result download is a deliberate provider choice (`PROVIDER.md` §5).
- `Page::new` exists only for `M = ()`; with metadata the struct literal is used. Fine.

## Packaging deviation (for the coordinator)

The brief lists `tokio` as a dev-dependency. The app's two live binaries need a runtime and a
timer, so the app depends on `tokio` (`rt-multi-thread`, `macros`, `time`) as an ordinary
dependency; the library code itself does not use it (polling waits are a caller-supplied future).
