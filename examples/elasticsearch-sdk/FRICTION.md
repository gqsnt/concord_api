# Friction

Historical author notes. P4.3 now fixes required handle values and declares checked runtime bases; see the current README and `docs/MIGRATION.md`.

What building this slice on the frozen Concord baseline (`c3fd4f1e`) cost, with the evidence,
what was written by hand, whether the cost is provider semantics or generic mechanics, and the
smallest Concord change that would remove it. No shared crate was edited.

## Concord features used

| Feature | For |
|---|---|
| `concord::api!` with a client, one scope and four endpoints | `POST /_bulk`, `POST /<index>/_pit`, `GET /_search`, `DELETE /_pit` |
| `send SendPolicy -> Result<_, Error>;` | every generated `send()` returns the SDK's `Error`; non-2xx answers read with `received.document::<Json, ErrorDocument>(64 KiB)` |
| `format Json;`, `request Ndjson;` (`concord::jsonl`, feature `jsonl`) | JSON everywhere; the bulk body as NDJSON with a line feed after every line (the reference's final-newline rule) and Concord's refusal of a record containing a line feed |
| `limit RESPONSE_LIMIT;` | every successful body bounded at 16 MiB (`ErrorKind::Limit` beyond) |
| `#[sensitive] header(AUTHORIZATION) … HeaderValue` | the API key, redacted in every Concord `Debug`/`Display` |
| `query refresh?: Refresh`, `query keep_alive: KeepAlive`, `scope index(name: IndexName)` | `Display` newtypes as the wire spelling |
| `Error::delivery()`, `Error::head()`, `ErrorKind` | the SDK's `Error::is_uncertain` rule for bulk writes |
| `validate_header_names()` | a unit test |
| `concord-auth::authorization("ApiKey", key)` | the sensitive `Authorization: ApiKey …` value with token-syntax validation |
| `concord-pagination::Pages` (`max_pages`, `items()`, `collect_complete`, `PagesError`) | the bounded `search_after` chain; `Bounded` is never completion |

Not adopted: `concord-retry` (item 4), `concord-ratelimit` (the cited pages document no client
quota; per-item `429`s are handled by explicit resubmission), streamed request bodies
(`StreamRecords`; the slice is a bounded, buffered set and every operation must be kept for
correlation anyway).

## 1. Bulk lines of different kinds need a type-erased line

**Evidence.** `Ndjson` implements `Encode<Vec<T>>` for one `T: Serialize`. A bulk body alternates
action lines (`{"index":{"_index":..,"_id":..}}`) and caller documents of any type, and a
`delete` has no source line. **By hand:** `BulkLine(Arc<RawValue>)` with a forwarding `Serialize`
(`src/bulk.rs`), every line serialized when its operation is added (`Operation::new`,
`object_json`), `serde_json`'s `raw_value` feature, and a second line-break check at add time:
Concord's check runs in `prepare()` and fails the whole request with a `record` number that
counts lines, not operations. **Split:** NDJSON framing is generic and came from Concord; which
line follows which action and "sources are JSON objects" are Elasticsearch's and stay in the SDK.
**Smallest change:** a validated pre-encoded line in `concord::jsonl` (say
`RawLine::new(String) -> Result<RawLine, CodecError>`, `Serialize`, refusing a line feed at
construction), so an SDK shares Concord's line rule when it builds each line.

## 2. Partial success inside a 2xx is not an "error inside a 2xx"

**Evidence.** For a 2xx that carries an error, the recipes (RECIPES.md, "Patterns from other
APIs": `{"ok": false}`) and the caller guide suggest a typed envelope *or a custom `Decode` that
returns a `CodecError`*. For bulk the second route is harmful: a `Decode` error consumes the
body, so the caller would lose which operations were applied, the exact knowledge needed to
avoid replaying them. **By hand:** decode `BulkResponse` as the typed output, then
`bulk::correlate` and `classify` (about 90 lines): count/action/id match, item classification,
`errors`-flag cross-check, `PartialBulk(report)` as an `Err`. **Split:** provider semantics;
Concord correctly stays out. **Smallest change:** documentation only: one line in that recipe
saying that a *partial* result is a typed output checked after decoding, never a decode error.

## 3. "Was the write executed?" is two facts combined by hand

**Evidence.** `Delivery::Answered` covers both a status error and a 2xx whose body then failed;
for bulk only the second means "executed, outcome unknown". **By hand:** `Error::is_uncertain`
(`src/error.rs`, about 20 lines) combines `delivery()` with `head().status()`, relying on the send
policy having turned every non-2xx into `Provider`; plus the SDK's own 4xx/5xx reading.
**Split:** the facts are Concord's and sufficient; the rule is the SDK's. **Smallest change:**
none needed.

## 4. `concord-retry` cannot express a shrinking resubmission

**Evidence.** `Retry::receive` repeats one prepared template; `Retry::run` calls a factory
`|attempt| future`. A bulk retry must send a *different* body on each attempt (only the failed,
retryable operations of the previous answer); repeating the template re-applies the operations
that succeeded, which is exactly the failure this slice must prevent. With `run`, the factory
would have to read a request that the classifier rewrites through shared state. **By hand:**
`BulkReport::resubmit(select)` / `resubmit_rejected()` build the next request from failed
operations only (an applied item is never offered), and the loop is left to the caller.
**Split:** which items may be resubmitted is provider semantics (429 here); "the next attempt
depends on the previous outcome" is generic. **Smallest change:** a `Retry::run` variant whose
factory receives the previous attempt's outcome and may return the next attempt (or none).

## 5. The PIT id is traversal state outside the page key

**Evidence.** A PIT traversal has a cursor (`search_after`, the page key) and a session id that
every response replaces and that the close needs after the traversal ends, also after a failed
page (a refused page with shard failures still returns the newest id). `Pages` keeps the key
only: `checkpoint()` is `None` after exhaustion, `items()` drops page metadata, and a failed
factory future returns only its error. **By hand:** `PointInTime` holds `Arc<Mutex<PitId>>`
shared with the factory (`share()`), and the factory clones the client, the PIT handle and the
search for every page (the factory and its futures must be `Send + 'static`). **Split:** the id
rule is Elasticsearch's; "state every page updates and the caller needs after the end" is
generic. **Smallest change:** document this shared-state pattern in the `concord-pagination`
README (a last-page metadata accessor would cover only the success path).

## 6. A provider without a canonical host still needs a static `base`

**Evidence.** `api!` requires one compile-time `base`. Elasticsearch is self-hosted or
per-deployment. **By hand:** the declared base is the reference curl example's
`http://localhost:9200`, and `Elasticsearch::new` requires the deployment URL (calling
`with_base_url`); a caller who builds `ElasticsearchApi::new` directly gets the plain-HTTP
localhost default. **Split:** generic. **Smallest change:** a declaration form whose base is a
required construction input.

## 7. One response limit for pages whose size the caller chooses

**Evidence.** `limit` is a declaration constant; a search page's size depends on the caller's
`page_size` and documents. A per-call bound would need `decode_limited` inside the send policy,
which only sees the endpoint. **By hand:** nothing; the 16 MiB limit is documented (a larger page
is `ErrorKind::Limit`; lower the page size). **Split:** generic. **Smallest change:** a per-request
limit override on generated requests that the send policy's `decode()` honours.

## 8. Provider data that quotes content needs declared secrecy

P4.1: field-only formatting now derives `concord::Debug` with `#[sensitive]`; computed byte-count and diagnostic projections stay manual. The following observations describe the earlier formatter work.

**Evidence.** Concord redacts values and never renders bodies, but Elasticsearch error `reason`s
can quote document content (`include_source_on_error` defaults to `true`), and hits, sort values
and PIT ids are data. **By hand:** 14 `Debug` impls (`ErrorCause`, `ErrorField`, `ProviderError`,
`Hit`, `SortValues`, `PitId`, `PointInTime`, `PitSearch`, `SearchBody`, `Sort`, `BulkLine`,
`BulkRequest`, `BulkReport`, `ApiKey`); the redaction test covers every one that holds a key,
a document, a query, a reason, a PIT id or sort values. **Split:** provider semantics: only the SDK
knows which fields quote content. **Smallest change:** none.

## Observations without a cost

- `GET search(body request: SearchBody)` (the guide documents `GET /_search` with a body)
  compiled and was sent with `Content-Length` and `Content-Type: application/json`; Concord
  neither refuses nor documents a `GET` body.
- Because the generated client is public (`es.api()`), its body and output types are public too
  (`private_interfaces` under `-D warnings`); a `pub(crate) client` would hide them but also the
  declared operations.
- An empty last page comes back from `Pages::next_page` as a page without a next key, then
  `Ok(None)`; this SDK's first protocol test expected `Ok(None)` at once and was corrected to the
  documented behaviour (agent mistake, not a Concord defect).
- The send policy, `document::<Json, _>(limit)`, `maybe_refresh(Option)` and the `Ndjson` request
  format worked as documented on the first build.

## Examples consulted

The result is example-assisted. Files read, and what was taken:

| File | Taken |
|---|---|
| `examples/openai-sdk/Cargo.toml` | dependency layout (Concord features, `concord-example-testserver` as a dev-dependency) |
| `examples/openai-sdk/src/lib.rs` | the `send SendPolicy -> Result<_, Error>` shape, `received.document::<_, _>(limit)` for error documents, a boxed provider error with `ResponseMeta` |
| `examples/openai-sdk/tests/protocol.rs` (lines 1–80) | test setup: `Script` + `TestServer::start`, a reqwest client with `no_proxy`, no redirects, `retry::never()`; assertions on `Recorded` |
| `examples/openai-sdk/PROVENANCE.md` (lines 1–60) | the fixture provenance table format |
| `examples/frankfurter-sdk/Cargo.toml` | precedent for `serde_json`'s `raw_value` feature in this workspace |
| `examples/clickhouse-sdk/Cargo.toml` | the style of commenting why each Concord feature is enabled |
| `examples/testserver/src/lib.rs` (lines 1–500) | the test server API (mandated by the brief) |

A workspace `grep` for `RawValue` also showed one-line hits in `examples/quickbooks-online-sdk`,
`examples/frankfurter-sdk` and `examples/xero-accounting-sdk` sources; nothing was taken from them.

## Evidence

- `cargo test --locked -p concord-sdk-elasticsearch`: 29 tests (19 protocol, 10 workflows) pass.
- Mutation check (each mutant applied alone to a copy-restored source, full test run): treating
  a partial failure as success, offering applied items to `resubmit`, ignoring the `errors`
  flag, skipping the item-count or id check, classifying the documented `delete`/`not_found` as a
  failure, not tracking the latest PIT id, ignoring shard failures, dropping the close outcome
  after a failed page, and reading 5xx or a failed 2xx body as certain — all 11 were killed.
