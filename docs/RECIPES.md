# Recipes

Application code around a Concord client, for tasks the declaration deliberately does not cover.
The recipes live in [`examples/native`](../examples/native) as documented functions over one
declaration ([`src/api.rs`](../examples/native/src/api.rs), the `Inventory` client); each is
exercised against a loopback server in `examples/native/tests/`
(`cargo test -p concord-example-native`). This page says when to use each one and what it
assumes; the code is in the linked files.

They use only the public surface: `Prepared::{send, send_raw, into_parts}`, `Decoder::finish`,
`Error::{response, into_response}`, `Encode`/`Decode` and the generated setters.

## Responses

### Error documents

[`errors::rename_item`](../examples/native/src/errors.rs) — the API documents an error body
(RFC 9457 problem details, a JSON error object).

- **Bounded, with the head:** `prepare()?.receive().await?`; for a non-2xx, read the document with
  `received.document::<Json, Problem>(n).await`: it checks the format's head, reads at most `n`
  bytes and returns metadata beside the result. Choose `IgnoreContentType<Json>` to preserve a
  provider's policy of accepting any media label. A send policy (`send Policy;`) does this once
  for a whole client; the [quickstart](QUICKSTART.md#4-configure-the-client-call-handle-the-result)
  shows it for one call.
- **Legacy native recipe (unbounded):** `errors::rename_item` retains its owned `reqwest::Error`
  and `CodecError` payloads; `document` instead returns a `concord::Error` that only borrows
  those inner errors. It uses `send()`; on error, return any error whose `status()` is `None` unchanged (not an
  error document), then `into_response()?` returns the unread response of the status error;
  decode its bytes with
  `<Json as Decode<Problem>>::decode_body((), bytes)`. That is a body-only decode: it checks no
  response head, so the error document's `Content-Type` is not judged. The status always stays
  with the document or with the reason it could not be read.
- **Assumes:** the error body fits in memory; against an untrusted server, use the bounded route
  above. A native read error is stored with `without_url()`.
  `into_response()` alone also returns the response of a `Metadata` error (a 2xx whose head the
  format refused); the recipe's `status()` guard keeps such a 2xx from being read as an error
  document.
- **When not:** "404 means `None`" needs no document: `match` on `error.status()`
  ([caller guide](CALLER_GUIDE.md#6-errors)).

### A response the format refuses

A server labels its JSON `text/html` or `text/plain`, or does not label it, and `Json` refuses the
response before parsing (`ErrorKind::Metadata`, the response kept unread).

- **How:** fix the server or the test fixture first. Where that is impossible, write
  `response IgnoreContentType<Json>;` on that endpoint (or on a scope of such endpoints). It skips
  only `Json`'s `Content-Type` check: the status, the other head checks, the JSON decoding and the
  `Accept` suggestion are unchanged. To look at what the server sent without decoding it,
  `error.into_response()` returns the unread response.
- **Assumes:** the exception is as narrow as the endpoint. On the client it would also accept a
  proxy's HTML error page up to the point where the JSON parser fails.
- **Evidence:** `r03_content_type_is_checked_unless_deliberately_ignored` in
  [`crates/concord/tests/bodies.rs`](../crates/concord/tests/bodies.rs); the full rules are in
  [Writing a format](CUSTOM_FORMATS.md#7-strict-json-and-the-explicit-adapter).

### A response header that decides how to decode

A service sends a schema version (or a format flag) in a header, and the body is only readable for
the versions you support.

- **How:** a format of your own reads the header in `context`, before the body is read, and hands
  the parsed value to `decode_body` as its typed `Context`. A missing or unsupported value is a
  `CodecError`, so the call fails with `ErrorKind::Metadata` and the body is never decoded.
- **Where:** `Versioned` in [`crates/concord/tests/metadata.rs`](../crates/concord/tests/metadata.rs),
  explained in [Writing a format](CUSTOM_FORMATS.md#5-a-vendor-format-that-checks-a-version-header).

### Response metadata with the typed value

[`metadata::send_with_meta`](../examples/native/src/metadata.rs) (and `item_with_meta`, one
endpoint packaged) — the caller needs a few response headers (an `ETag` for a later conditional
request, a request id, a count) together with the decoded value, possibly after a native edit.

- **Without a native edit, start with `receive`:** `prepare()?.receive().await?` returns a
  `concord::Received` for any status; read the headers you need from `received.head()`, then
  `received.decode().await` (the [author guide](AUTHOR_GUIDE.md#511-reading-response-metadata-in-an-sdk)
  shows the pattern; `received.url()` gives the final URL). The recipe below adds a native edit
  and copies the selected headers into a value of its own.
- **How:** `prepare()?.into_parts()`; edit the native request; one `client.execute(request)`. On
  `Err(e)`, convert with `decoder.transport_error(e)` (Concord's `Transport` error naming the
  endpoint, URL removed); `Decoder::execute(&client, request)` does both and returns a
  `concord::Received`. On a response, copy the status, the final URL and the headers you select by
  borrowing it, then `decoder.decode(response)` as usual. The recipe returns `WithMeta { value, meta }` or a
  `MetaError` holding the `concord::Error` and `Option<Meta>`: `None` when no response head
  arrived, `Some` once one did. `Status` and `Metadata` errors still keep the whole unread response
  (`into_response()`); after a body-read failure (`Transport`) or a `Decode` failure only the
  captured head is left, the body is gone.
- **Assumes:** you choose which headers outlive the response; nothing else is kept. `Debug` of
  `Meta`, `WithMeta` and `MetaError` is written by hand: status, version and kept header
  **names**, never a header value (`Set-Cookie`), the URL or the decoded value, so no `T: Debug`
  bound. Values are read deliberately with `raw_header`, `raw_headers`, `raw_url` and `value`.
  Never return the raw `reqwest::Error` of `client.execute(..).await?`: its `Display` prints the
  full URL, sensitive query values included.
- **Whole head, same output:** read `Received::head` or keep `Received::meta` as a `concord::ResponseMeta` before decoding, without changing the declared output type.
- **When not:** for the value alone, `send()`; for the headers of a status error alone,
  `error.response()`.
- **Evidence:** [`examples/native/tests/metadata_recipe.rs`](../examples/native/tests/metadata_recipe.rs)
  (native edit on the wire, metadata captured while the body is still withheld, a refused
  connection to a URL with a synthetic secret against the raw-error control, each failure stage,
  `Debug` with synthetic cookie, URL and payload values).

### Streaming a large or endless body

[`streaming::first_event_chunks`](../examples/native/src/streaming.rs) — a chunked export or event
feed that must not be buffered.

- **How:** `prepare()?.send_raw()` returns the native response unread; check the status yourself,
  then read `chunk()` by chunk; drop the response to stop early.
- **Assumes:** the per-chunk deadline (`tokio::time::timeout`) and the chunk limit are the
  application's. Dropping a response before its end closes that connection instead of returning it
  to the pool.
- **Note:** a finite typed output (`-> Bytes` with `response Binary;`, `-> T`) buffers the whole
  body for `Decode<T>`. `-> stream T` uses `Incremental<T>` and opens its reader without reading
  the body; `-> stream` returns the unread `Incoming` for a reader chosen at run time (see
  [author guide, 5.9](AUTHOR_GUIDE.md#59-streaming-outputs---stream)). An endpoint without `->`
  drains the body without buffering but discards it. This is the declared handling used by
  `Prepared::send` and generated `send()` without a client send policy.
- **Request bodies** go the other way: a body produced while it is sent, with one fixed media
  type, is a format. [`upload::Streamed`](../examples/native/src/upload.rs) implements
  `Encode<OneShot>` and moves a non-`Clone` one-shot source into `reqwest::Body::wrap_stream`
  (reqwest's `stream` feature, enabled in that example's `Cargo.toml`); `prepare()` does not poll
  it, one `send()` reads it once, chunked. The request cannot be cloned or replayed: another
  upload needs a fresh source. Evidence:
  [`examples/native/tests/upload.rs`](../examples/native/tests/upload.rs).

## Requests

### A per-request timeout or an extra header

[`native_edit::item_with_timeout`](../examples/native/src/native_edit.rs) — one call needs a
transport setting or a header the declaration does not carry (a request id, a tighter timeout).

- **How:** `prepare()?.into_parts()`, edit the native request (`timeout_mut()`, `headers_mut()`),
  `decoder.finish(client.execute(request).await).await` (`finish` is async: it reads the body). A
  timeout becomes `ErrorKind::Transport` with `transport().is_timeout()`.
- **Assumes:** after `into_parts` the request is native — its `Debug` prints the full URL
  (sensitive query values and path segments included) and every header value not flagged
  sensitive. Mark a secret extra header with `HeaderValue::set_sensitive(true)` yourself.

### An operation the SDK does not declare

[`concord-example-extension`](../examples/extension) — another crate needs an endpoint the SDK's
declaration lacks (here a tenant's CSV export, `GET tenants/{tenant}/exports/{export}`), and
cannot or should not edit the SDK.

- **How:** the SDK publishes a [context](../examples/sdk/src/context.rs), configured once
  (client, base URL, credentials), that yields both the generated handles and native requests:
  `tenant.request(Method::GET, &["exports", id])?` keeps the base path prefix, encodes each
  segment like a Concord path value and adds the service headers (the key flagged sensitive).
  Add `Accept`, execute once with the context's client, finish with your own format —
  `Decoder::<Vec<CsvRow>>::new::<Csv>().finish(client.execute(request).await)` — and name the
  operation in your own error type.
- **Assumes:** the SDK keeps the context in step with its declaration (base, header names,
  encoding); the example's SDK tests compare it with the generated endpoints. The decoder is
  unlabeled (`endpoint()` is `None`), and the request is native: its `Debug` and URL show path
  data, so do not log it when that data is secret.
- **When not:** do not prepare a declared endpoint as a seed and rewrite its URL. The native URL
  can be rewritten freely, but the seed's base, headers, `Accept`, decoder and endpoint name still
  belong to the other endpoint, and nothing checks that they fit the new route (an append-only
  seed helper, like the campaign's, also reaches only suffixes of declared routes). If you own the
  SDK, declare the endpoint.

### Interleaved query parameters

An API that wants repeated groups across keys — `f=a&op=eq&v=1&f=b&op=gt&v=2` — cannot be
declared: Concord keeps one position per query key ([author guide](AUTHOR_GUIDE.md#47-order-on-the-wire)).
Declare the endpoint with its fixed parameters and append the sequence to the native URL.
`query_pairs_mut` applies the same `application/x-www-form-urlencoded` encoding as Concord.

Excerpt of [`examples/native/src/native_edit.rs`](../examples/native/src/native_edit.rs) (the
endpoint is `GET search() -> Vec<Item> { path "search"; query { "limit" = "10"; } }` under the
client's `response Json;`):

```rust
pub async fn search_filters(
    api: &Inventory,
    filters: &[Filter<'_>],
) -> Result<Vec<Item>, concord::Error> {
    let (client, mut request, decoder) = api.search().prepare()?.into_parts();
    if !filters.is_empty() {
        let mut pairs = request.url_mut().query_pairs_mut();
        for filter in filters {
            pairs
                .append_pair("f", filter.field)
                .append_pair("op", filter.op)
                .append_pair("v", filter.value);
        }
    }
    decoder.finish(client.execute(request).await).await
}
```

`examples/native/tests/native_edit.rs` checks the exact request target on the wire:
`/v1/search?limit=10&f=name&op=eq&v=Blue+pen&f=qty&op=gt&v=2&f=name&op=ne&v=a%26b%3Dc`.

- **Assumes:** the declared pairs come first; the appended ones follow in the given order.
  Declared sensitive query keys are no longer redacted in any native `Debug` output.

### A resource key spanning path segments

Concord has no multi-segment path value (a typed sequence was prototyped and deferred: its
redaction would need run-time ranges). Declare the final path position as an ordinary value (for example,
`PUT write(key: String) { path "objects/{key}"; }`). A value `a/b` supplied to that
endpoint is **one** segment, sent as `a%2Fb`. If the service instead expects multiple
segments, pass an explicit list to this application-level helper. Excerpt of
[`write_resource_segments`](../examples/compat/src/lib.rs):

```rust
pub async fn write_resource_segments(
    api: &Resources,
    segments: &[&str],
) -> Result<(), Box<dyn std::error::Error>> {
    if segments.is_empty()
        || segments
            .iter()
            .any(|s| s.is_empty() || *s == "." || *s == ".." || s.chars().any(char::is_control))
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "resource key needs non-empty segments other than . and .., without controls",
        )
        .into());
    }
    let (client, mut request, decoder) = api.write("placeholder").prepare()?.into_parts();
    request
        .url_mut()
        .path_segments_mut()
        .map_err(|_| std::io::Error::other("base URL cannot hold path segments"))?
        .pop()
        .extend(segments.iter().copied());
    decoder.finish(client.execute(request).await).await?;
    Ok(())
}
```

This helper assumes the placeholder is the **last** segment, with no declared trailing slash;
`pop()` removes it. It rejects an empty sequence, empty items, `.`/`..` and control characters
before editing: the URL library discards dot segments and can discard newline and tab characters.
It does not split strings: choose the
segment boundaries in application code. Each item is data, not pre-escaped text: `a/b` becomes
`a%2Fb` and `%2F` becomes `%252F`. Backslash and Unicode are encoded by the
native URL library; its encoding policy need not match Concord's for every punctuation character.
If a service requires an empty item or trailing slash, define and test that service's policy
explicitly. A runtime base path is retained. The loopback
[`resource_key` test](../examples/compat/tests/resource_key.rs) checks these cases.

`into_parts()` hands out a native request: its `Debug` and URL access expose path and query
values, including secrets. Do not log that request or URL when they contain secrets. Concord's
`decoder.finish` still supplies the declared response handling and redacts its own error output;
explicit native response and error accessors have their documented disclosure boundaries.

## Body formats

### A custom format: NDJSON

[`repr::Ndjson`](../examples/native/src/repr.rs), used by
[`bodies::import_stock`](../examples/native/src/bodies.rs) — line-delimited JSON in both
directions.

- **How:** a marker type implementing `Encode<Vec<T>>` and `Decode<Vec<T>>`; a statement names it
  by its path, like `Json` (`format Ndjson;` on the endpoint, or on a scope for all
  its endpoints), with the payloads written as types (`body rows: Vec<StockRow>`,
  `-> Vec<StockRow>`).
- **Assumes:** error labels are static text; the position is numbers (`record`, `line`, `column`,
  added with `CodecError::at`), never content. The underlying serde error (which may quote the
  body) is attached with `with_source` and read with `source_unredacted()`. This `Ndjson` does
  not override `check_content_type`, so it accepts any response `Content-Type`.
  [Writing a format](CUSTOM_FORMATS.md#6-ndjson-with-positions) walks through it.

### Form-encoded bodies

With the `form` feature, `Form` encodes any `Serialize` value with flat fields
(`request Form;` on the endpoint or a scope) through `serde_urlencoded`, and decodes form
responses (`response Form;`); see the [author guide](AUTHOR_GUIDE.md#56-forms).

Without the feature, or without serde: [`repr::Form`](../examples/native/src/repr.rs), used by
[`bodies::send_feedback`](../examples/native/src/bodies.rs) —
`application/x-www-form-urlencoded` from ordered `(name, value)` pairs.

- **How:** a custom `Encode<Vec<(String, String)>>` with `url::form_urlencoded`, named by path
  (`request repr::Form;`), a different type from `concord::Form`.
- **When:** the body is fully determined by the value and its content type is constant.

### Multipart uploads

With the `multipart` feature, `concord::multipart` declares a `multipart/form-data` or
`multipart/related` body like any other: `body form: MultipartBody` with `request Multipart;`
([author guide](AUTHOR_GUIDE.md#510-bodies-whose-media-type-is-known-at-run-time)). Used by
`upload_batch_file` in [`examples/openai-sdk`](../examples/openai-sdk/src/lib.rs) (a form-data
file and field) and `upload_multipart` in
[`examples/google-drive-sdk`](../examples/google-drive-sdk/src/upload.rs) (`multipart/related`:
JSON metadata, then the media).

- **How:** `MultipartBody::form_data().text(name, value).file(name, file_name, content_type,
  bytes)`, or `MultipartBody::related_with_type(root)` (RFC 2387's `type` parameter) /
  `related()` (without it, only where the provider documents that form) with
  `.part(Part::new(bytes).header(name, value))`; then the generated `send()`.
- **What it does:** writes the parts in order, chooses the boundary `concord-boundary-<n>` that
  occurs in no part (deterministic: checked, not left to chance), and sends
  `multipart/<subtype>; boundary=...` as the body's runtime media type with `Content-Length`. A
  declared media type or `content-type` header that differs is a `Build` error before I/O.
- **Assumes:** the parts fit in memory. Names, file names and part header values with control
  characters (tab excepted), and `"` or `\` in form-data names and file names, are refused rather
  than escaped; sanitize them first if they come from users.

For large or streamed files, [`concord_example_multipart::upload`](../examples/multipart/src/lib.rs)
attaches reqwest's own `multipart` form (enabled in that example crate only) to a declared endpoint
without a body.

- **How:** `prepare()?.into_parts()`, then
  `reqwest::RequestBuilder::from_parts(client, request).multipart(form).send().await` and
  `decoder.finish(result).await`. The declared path, headers and decoder still apply; reqwest
  writes the boundary, `Content-Type` and (for parts of known length) `Content-Length`.
- **When:** a part is streamed (`Part::stream_with_length`) or too large to buffer, or reqwest's
  `Form` is already the application's currency.
- **Assumes:** field names use `Form::percent_encode_attr_chars()` (reqwest's default leaves `\`
  raw inside `name="..."`); file names with CR/LF are escaped by reqwest in a form a strict
  RFC 9110 recipient may reject, so sanitize them if the server is unknown.

## Application policies

`Prepared::send` and generated `send()` without a client send policy perform one `execute`,
with no Concord retry or extra request. A client `send policy;` delegates generated `send()` to
`SendWith<T>::send`; that policy may sign, make sub-requests or repeat requests. The recipes here
implement retries, pagination, admission and authentication as ordinary application code with
their assumptions stated; none is a recommended default for every API. All are in
[`policies.rs`](../examples/native/src/policies.rs).

| Recipe | When | Assumptions stated by the recipe |
|---|---|---|
| `collect_items` | walk a cursor-paginated listing | the application bounds the pages; the unused cursor is returned, nothing is silently dropped; each page is a fresh request (`maybe_cursor(cursor)`) |
| `retry_bounded`, `item_with_retry` | retry a read-only `GET` after a connection failure | the operation is safe to replay (a read-only `GET`); only failures without a response are retried (connect/request errors, not timeouts, statuses or decode errors); attempts and backoff are bounded. Do not reuse the predicate for writes: whether a write may be replayed depends on the API (idempotency keys, conditional requests) |
| `items_admitted` | at most N requests in flight | requests are built first (no I/O), then each waits for a `Semaphore` permit and holds it until decoded |
| `Session` | the application rotates a credential | the application obtains the new value; `set_key(..)` produces a new handle, and requests created earlier keep the old key |

A request value's `Clone` (available when its values and body are `Clone`) only means it can be
copied. The retry recipe deliberately takes a request **factory** instead.

## SDK and application patterns

[`examples/app`](../examples/app) is an application over a shared SDK
([`examples/sdk`](../examples/sdk): two independent clients, Catalog and Ledger). Each pattern
below is application code with its policy stated in the module; the tests run against the
loopback fake services of [`examples/fixture`](../examples/fixture)
(`cargo test -p concord-example-app`), and the [README](../examples/app/README.md) maps modules
to tests.

### Domain values into a service's representation

[`catalog_repr::activity`](../examples/app/src/catalog_repr.rs) and
[`ledger_repr::entries`](../examples/app/src/ledger_repr.rs) — the application has its own
[`AccountId` and `Moment`](../examples/app/src/domain.rs), and each service writes them
differently (decimal and whole-second RFC 3339; `acct-` hex and milliseconds).

- **How:** one module per service converts the domain values into that service's text and returns
  the **generated operation**: `activity(&Catalog, AccountId, Option<Moment>) ->
  Result<CatalogActivity, CatalogReprError>`. It is synchronous and sends nothing; the caller keeps
  the operation's setters (Ledger's `entries(..)?.set_page_size(2)`), `prepare()` and `send()`.
- **Assumes:** each service has its own precise error enum (a fractional second, a year outside
  0000–9999; sub-millisecond precision, an instant before 1970, `u64` overflow). Nothing is
  rounded, wrapped or truncated, and a refused value never creates a request.
- **When not:** a value the application already holds as the service's text needs no adapter. A
  `Display` on the domain type tuned to one service would put that service's rule in the domain.

### Independent outcomes per service

[`report::account_report`](../examples/app/src/report.rs) — one domain query touches two services,
and one service refusing or failing must not hide the other's answer.

- **How:** `Outcome::attempt(conversion, Operation::send)` per service, kept in one `Outcome` each:
  `Rejected(E)` (nothing sent), `Failed(concord::Error)` or `Succeeded(T)`. No aggregate `?`.
- **Assumes:** this aggregation is application policy, not a Concord rule: Concord returns one
  result per `send()` and has no notion of a report. The services are attempted one after the
  other.
- **When not:** when the result means nothing unless both parts exist, short-circuiting with `?`
  is the honest policy.

### A named workflow

[`workflow::Browser`](../examples/app/src/workflow.rs) — a call with application intent
(`unfiltered_page`: the featured listing without its tag filter) that would otherwise be spelled
`list().set_tags(Vec::new())` at each call site.

- **How:** a struct holding the configured scope handle (a `CatalogTenantListing` with default
  tags) whose methods return the generated `CatalogTenantListingList`, so callers still chain
  setters, `prepare()` and `send()`. The per-request `set_tags` leaves the handle's defaults
  intact for sibling calls.
- **Assumes:** only calls that carry intent get a name; everything else goes through the SDK
  handle (`Browser::listing()`).
- **When not:** wrapping every generated endpoint in a same-named method adds a layer without
  meaning.

### Bounded traversal with a continuation

[`traversal::traverse`](../examples/app/src/traversal.rs) — two services page differently
(`next_cursor`, `paging.next`) and the application walks both.

- **How:** one walk over `fetch(token) -> Page<T>`, with a `NonZeroUsize` page bound; small
  per-service adapters (`catalog_items`, `ledger_entries`) set the token on a clone of a request
  template and map the page. The result is `Complete` or `Bounded` with a `Continuation`, and every
  call takes `from: Option<Continuation>` to resume.
- **Assumes:** a failed page stops the walk with `Interrupted`: the error, the records collected in
  that call and `retry_from`, the position of the failed page. The caller keeps the records and
  resumes, or discards them and restarts; nothing is retried. A continuation belongs to the
  template it came from, and "no duplicates, no gaps" holds only if the listing does not change
  between calls.
- **When not:** a single page needs no walk. Unlike `collect_items` above, which drops collected
  pages on error, this walk hands them back; pick the policy your caller needs.

### Many pages or jobs: `concord-pagination`

When an SDK has several listings to walk, or result manifests to fan out over, the separate
[`concord-pagination`](../crates/concord-pagination/README.md) crate packages this walk: a
dependent chain (`sequential`, with item-level checkpoints), a known plan of independent keys or a
first page that fixes the plan (`independent`, `first_page_then`), and manifest-discovered jobs
that each read a whole streamed body (`manifest_jobs`), all with budgets, a bounded window and
reports that keep failed keys apart from the resume point. It does not depend on Concord: the SDK
supplies a factory from a key to an owned future (a generated operation with the key set, `send()`,
and a projection of the response into a page), so the continuation rules stay in the SDK adapter,
and `Prepared::send` or generated `send()` without a client send policy performs one `execute`.
A client `send policy;` delegates to `SendWith<T>::send`, which may sign, make sub-requests or
repeat requests. The pagination README states the bounds and what the crate cannot check;
the provider SDK examples (`examples/{github,stripe,shopify,salesforce,kubernetes}-sdk`)
show adapters over real provider contracts.

## Patterns from other APIs

[`examples/compat`](../examples/compat) is a downstream crate (edition 2021, a private client) with
one compact case per pattern below, each checked against a loopback server. Its
[README](../examples/compat/README.md) says which outside facts are sourced and which are
synthetic; a fake server proves nothing about the real service.

| Need | How | Case |
|---|---|---|
| Read response headers (`ETag`, `Link`) and keep the typed decoding | `prepare()?.receive().await?`, read `head()` (and `url()`), then `decode()`; with a native edit, `into_parts()` then `decoder.execute(&client, request)`; packaged with failure stages in [response metadata](#response-metadata-with-the-typed-value) | `tests/metadata.rs` |
| Read a status error's headers without consuming it | `error.response()`; `into_response()` afterwards for the body | `tests/metadata.rs` |
| An error inside a 2xx (`{"ok": false}`) | a typed output that models the envelope, or a custom `Decode` returning a `CodecError` (its detail goes to `with_source`, read with `source_unredacted()`) | `tests/application.rs` |
| Nested or bracket form keys (`metadata[order]=7`) | a custom `Encode<T>` with the form media type; `form` itself is flat | `tests/forms.rs` |
| Follow a `Link: rel="next"` or a `next` URL as the same endpoint | validate the link (origin, path) in the SDK, then `prepare()?.follow(&url)?` and send it: headers and handling are kept, another origin is refused | `checks/phase-a/tests/received.rs` |
| A key spanning path segments (`docs/2026/report.md`) | `into_parts()`, then `url_mut().path_segments_mut()`; an ordinary path value is one segment (`/` is sent as `%2F`) | `tests/resource_key.rs` |
| JSON or nothing (`200` with a document, `204` without) | a format of your own implementing `Decode<Option<T>>` that decides on the status: a `204` needs no `Content-Type` and is `None`; any other 2xx must pass `Json`'s head check (`Json` stays strict; an empty `200` is a decode error) | `tests/json_or_empty.rs` |
| JSON under another media type (Kubernetes server-side apply) | `request Json "application/apply-patch+yaml";` on the endpoint | `tests/apply.rs` |
| A fresh value per request (an idempotency key) | an initialized value on the endpoint; a `.clone()` of the request handle repeats it (a deliberate retry); a client initializer runs once, in `new` | `tests/idempotency.rs` |
| A new base URL or credential | `with_base_url` and setters return new handles; scopes created earlier keep the old values | `tests/refresh.rs` |
| Header aliases, `Content-Type` and secrets | native constants, re-exports and shadowed names compose and classify by resolved name | `tests/headers.rs` |

## Running them

```sh
cargo test -p concord-example-native --locked
cargo test -p concord-example-compat --locked
```

All fixtures are plain HTTP/1.1 on loopback: no HTTP/2, TLS, proxy or redirect behaviour is
exercised by these recipes.
