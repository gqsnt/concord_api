# Recipes

Application code around a Concord client, for tasks the declaration deliberately does not cover.
The recipes live in [`examples/native`](../examples/native) as documented functions over one
declaration ([`src/api.rs`](../examples/native/src/api.rs), the `Inventory` client); each is
exercised against a loopback server in `examples/native/tests/`
(`cargo test -p concord-example-native`). This page says when to use each one and what it
assumes; the code is in the linked files.

They use only the public surface: `Prepared::{send, send_raw, into_parts}`, `Decoder::finish`,
`Error::into_response`, `Encode`/`Decode` and the generated setters.

## Responses

### Error documents

[`errors::rename_item`](../examples/native/src/errors.rs) — the API documents an error body
(RFC 9457 problem details, a JSON error object).

- **How:** `send()`; on error, `into_response()?` returns the unread response of a status error
  (any other error propagates unchanged); decode its bytes with `<Json as Decode<Problem>>::decode`.
  The status always stays with the document or with the reason it could not be read.
- **Assumes:** the error body fits in memory. Against an untrusted server, read it with
  `response.chunk()` up to your own limit. A native read error is stored with `without_url()`.
- **When not:** "404 means `None`" needs no document: `match` on `error.status()`
  ([caller guide](CALLER_GUIDE.md#6-errors)).

### Streaming a large or endless body

[`streaming::first_event_chunks`](../examples/native/src/streaming.rs) — a chunked export or event
feed that must not be buffered.

- **How:** `prepare()?.send_raw()` returns the native response unread; check the status yourself,
  then read `chunk()` by chunk; drop the response to stop early.
- **Assumes:** the per-chunk deadline (`tokio::time::timeout`) and the chunk limit are the
  application's. Dropping a response before its end closes that connection instead of returning it
  to the pool.
- **Note:** any declared typed output (`-> Bytes` with `response bytes;`, `-> T`) buffers the whole body, as every
  `Decode` does; an endpoint without `->` drains it without buffering but discards it.

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

### Interleaved query parameters

An API that wants repeated groups across keys — `f=a&op=eq&v=1&f=b&op=gt&v=2` — cannot be
declared: Concord keeps one position per query key ([author guide](AUTHOR_GUIDE.md#47-order-on-the-wire)).
Declare the endpoint with its fixed parameters and append the sequence to the native URL.
`query_pairs_mut` applies the same `application/x-www-form-urlencoded` encoding as Concord.

Excerpt of [`examples/native/src/native_edit.rs`](../examples/native/src/native_edit.rs) (the
endpoint is `GET search() -> Vec<Item> { path "search"; query { "limit" = "10"; } }` under the
client's `response json;`):

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

## Body formats

### A custom format: NDJSON

[`repr::Ndjson`](../examples/native/src/repr.rs), used by
[`bodies::import_stock`](../examples/native/src/bodies.rs) — line-delimited JSON in both
directions.

- **How:** a marker type implementing `Encode<Vec<T>>` and `Decode<Vec<T>>`; a statement names it
  by path where `json` would be written (`format Ndjson;` on the endpoint, or on a scope for all
  its endpoints), with the payloads written as types (`body rows: Vec<StockRow>`,
  `-> Vec<StockRow>`).
- **Assumes:** error descriptions name the line and column, never its content; the underlying
  serde error (which may quote the body) is attached with `with_source`.

### Form-encoded bodies

With the `form` feature, the built-in `form` format encodes any `Serialize` value with flat fields
(`request form;` on the endpoint or a scope) through `serde_urlencoded`, and decodes form
responses (`response form;`); see the [author guide](AUTHOR_GUIDE.md#56-forms).

Without the feature, or without serde: [`repr::Form`](../examples/native/src/repr.rs), used by
[`bodies::send_feedback`](../examples/native/src/bodies.rs) —
`application/x-www-form-urlencoded` from ordered `(name, value)` pairs.

- **How:** a custom `Encode<Vec<(String, String)>>` with `url::form_urlencoded`, named by path
  (`request repr::Form;`) so it is not mistaken for the word `form`.
- **When:** the body is fully determined by the value and its content type is constant.

### Multipart uploads

[`concord_example_multipart::upload`](../examples/multipart/src/lib.rs) — `multipart/form-data`
through reqwest's own `multipart` feature, enabled in that example crate only.

- **How:** declare the endpoint without a body; `prepare()?.into_parts()`, then
  `reqwest::RequestBuilder::from_parts(client, request).multipart(form).send().await` and
  `decoder.finish(result).await`. The declared path, headers and decoder still apply; reqwest
  writes the boundary, `Content-Type` and (for parts of known length) `Content-Length`.
- **Why native:** `Encode::CONTENT_TYPE` is a constant, while a multipart boundary is chosen per
  body; Concord has no multipart policy.
- **Assumes:** field names use `Form::percent_encode_attr_chars()` (reqwest's default leaves `\`
  raw inside `name="..."`); file names with CR/LF are escaped by reqwest in a form a strict
  RFC 9110 recipient may reject, so sanitize them if the server is unknown.

## Application policies

Concord performs no retries, pagination, admission or authentication: one `send()` is one
`execute`. These are ordinary application code with their assumptions stated; none is a
recommended default for every API. All are in [`policies.rs`](../examples/native/src/policies.rs).

| Recipe | When | Assumptions stated by the recipe |
|---|---|---|
| `collect_items` | walk a cursor-paginated listing | the application bounds the pages; the unused cursor is returned, nothing is silently dropped; each page is a fresh request (`maybe_cursor(cursor)`) |
| `retry_bounded`, `item_with_retry` | retry a read-only `GET` after a connection failure | the operation is safe to replay (a read-only `GET`); only failures without a response are retried (connect/request errors, not timeouts, statuses or decode errors); attempts and backoff are bounded. Do not reuse the predicate for writes: whether a write may be replayed depends on the API (idempotency keys, conditional requests) |
| `items_admitted` | at most N requests in flight | requests are built first (no I/O), then each waits for a `Semaphore` permit and holds it until decoded |
| `Session` | the application rotates a credential | the application obtains the new value; `set_key(..)` produces a new handle, and requests created earlier keep the old key |

A request value's `Clone` (available when its values and body are `Clone`) only means it can be
copied. The retry recipe deliberately takes a request **factory** instead.

## Running them

```sh
cargo test -p concord-example-native --locked
```

All fixtures are plain HTTP/1.1 on loopback: no HTTP/2, TLS, proxy or redirect behaviour is
exercised by these recipes.
