# concord-example-native

Downstream example (not published): native-integration recipes written as ordinary application
code around one Concord declaration (`src/api.rs`, the `Inventory` client). Every recipe is a
documented `pub async fn` in `src/` and is exercised against a loopback server in `tests/`.
Concord itself is unchanged: everything below uses the public surface only
(`Prepared::{send, send_raw, into_parts}`, `Decoder::{decode, finish}`, `Error::into_response`,
`Encode`/`Decode`, generated `set_*`/`maybe_*`).

[`docs/RECIPES.md`](../../docs/RECIPES.md) indexes these recipes by task (when to use each one and
what it assumes); [`docs/CALLER_GUIDE.md`](../../docs/CALLER_GUIDE.md) covers the surface they
build on.

| Task | Recipe (`src/`) | What it shows | Tests (`tests/`) |
|---|---|---|---|
| U14 | `errors::rename_item` | non-2xx → `into_response()?` → status + `<Json as Decode<Problem>>::decode_body((), bytes)` (body-only: no response head check); undecodable bodies keep the status and a static label; non-status errors propagate unchanged (a `Metadata` error's 2xx response comes back too) | `errors.rs` |
| U15, R07 | `native_edit::item_with_timeout` | `prepare()?.into_parts()`, per-request timeout and extra header, `decoder.finish(client.execute(req).await)`; a timeout is `ErrorKind::Transport` with `is_timeout()` | `native_edit.rs` |
| U32 | `native_edit::search_filters` | interleaved repeated keys `f=..&op=..&v=..` appended with `url_mut().query_pairs_mut()` after the declared `limit=10` | `native_edit.rs` |
| U16 | `repr::Ndjson`, `bodies::import_stock` | a downstream `Encode<Vec<T>>`/`Decode<Vec<T>>` format, named by path (`format Ndjson;`); decode errors carry `record`, `line` and `column` numbers under a static label, never the line's content (the serde error is the unredacted source) | `bodies.rs` |
| U29, R09 | `repr::Form`, `bodies::send_feedback` | `application/x-www-form-urlencoded` through a custom `Encode` (`url::form_urlencoded`), written `request repr::Form;`; the built-in `form` format (feature `form`) does the same through serde | `bodies.rs` |
| U28, R08 | `streaming::first_event_chunks` | `send_raw()` on a real `Transfer-Encoding: chunked` response, application-owned per-chunk `tokio::time::timeout`, early stop by dropping (the server observes the close) | `streaming.rs` |
| W1 (S02–S04) | `upload::{OneShot, Streamed}`, `upload::upload_attachment` | a non-`Clone`, instrumented one-shot source moved by a custom `Encode<OneShot>` (fixed `application/octet-stream`) into `reqwest::Body::wrap_stream`; building, `Debug`, `prepare()` and `request()` poll it zero times; one `send()` sends exactly its bytes, chunked, and drains it once; the operation and `Prepared` are not `Clone` (doc `compile_fail`), a second upload takes a fresh source; a declared `request Streamed "application/x-ndjson"` or a `content-type` header replaces the static media type | `upload.rs` |
| W2 (M01–M05) | `metadata::{execute, send_with_meta, item_with_meta}` | `into_parts()` → native edit → one `execute` → selected head metadata copied while the body is unread → the endpoint's `Decoder::decode`; a transport failure is converted by `decoder.transport_error(e)` (control: the raw `reqwest::Error` prints the URL secret); `MetaError` keeps the `concord::Error` plus `Option<Meta>` (`None` = no head); `Status`/`Metadata` keep the unread response, body-read `Transport` and `Decode` keep only the head; `Debug` shows no header value, URL or payload | `metadata_recipe.rs` |
| U30, R12 | `policies::collect_items` | finite cursor loop with an application bound; exact query strings asserted; the unused cursor is returned | `policies.rs` |
| U30, R11, R12 | `policies::retry_bounded`, `policies::item_with_retry` | bounded retry around a request **factory** for a read-only GET; status/decode errors and timeouts are not retried; one `send()` is one execute | `policies.rs` |
| U31, R10, R12 | `policies::items_admitted` | build (no I/O) → `Semaphore` permit → `send`; peak in flight ≤ permits (with a wide-semaphore control) | `policies.rs` |
| U31, R12 | `policies::Session` | caller-updated authorization via the generated `set_key(..)`; requests and handles created earlier keep the old key | `policies.rs` |

## Assumptions the recipes state, not Concord

- **Replay safety** is the application's decision. A request's `Clone` only means it can be
  copied. `item_with_retry` retries a read-only GET, and only failures where no response arrived
  (connect/request errors, not timeouts); do not reuse that predicate for writes.
- **Bounds** (pages, attempts, permits, per-chunk deadlines) are chosen by the caller.
- **Credentials** are supplied by the application; Concord stores and sends what it is given.
- After `into_parts()` the request is native: Concord's redacted `Debug` no longer applies, and
  native `Debug`/errors can print URLs (the recipes call `without_url()` on native errors they
  keep).

## Not exercised here

- reqwest's own `form` and `json` features are not enabled in this example. Its `stream` feature
  is enabled in this crate's `Cargo.toml` only, for `Body::wrap_stream` in the upload recipe:
  Concord does not depend on it (`cargo tree --locked -e features -i reqwest -p concord` lists no
  `stream`). The response-streaming recipe still uses `Response::chunk()`, not `bytes_stream()`.
- Multipart uploads live in [`examples/multipart`](../multipart), the only crate that enables
  reqwest's `multipart` feature.
- No HTTP/2, TLS, proxy or redirect behaviour: all fixtures are plain HTTP/1.1 on loopback.

## Run

```sh
CARGO_TARGET_DIR=target cargo test -p concord-example-native --locked
cargo clippy --locked -p concord-example-native --all-targets -- -D warnings
cargo fmt --all -- --check
```
