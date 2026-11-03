# concord-example-native

Downstream example (not published): native-integration recipes written as ordinary application
code around one Concord declaration (`src/api.rs`, the `Inventory` client). Every recipe is a
documented `pub async fn` in `src/` and is exercised against a loopback server in `tests/`.
Concord itself is unchanged: everything below uses the public surface only
(`Prepared::{send, send_raw, into_parts}`, `Decoder::finish`, `Error::into_response`,
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

- reqwest's own `form`, `json` and `stream` features are not enabled in this example (it adds
  no dependency or feature beyond Concord's), so streaming uses `Response::chunk()` rather than
  `bytes_stream()`.
- Multipart uploads live in [`examples/multipart`](../multipart), the only crate that enables
  reqwest's `multipart` feature.
- No HTTP/2, TLS, proxy or redirect behaviour: all fixtures are plain HTTP/1.1 on loopback.

## Run

```sh
CARGO_TARGET_DIR=target cargo test -p concord-example-native --locked
cargo clippy --locked -p concord-example-native --all-targets -- -D warnings
cargo fmt --all -- --check
```
