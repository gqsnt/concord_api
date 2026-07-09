# Advanced Endpoints

The facade-first client is the normal API. Advanced endpoint structs are available under `endpoints::*` for focused tests, reusable endpoint values, and explicit request construction.

```rust
let endpoint = example_api::endpoints::GetUser::new(42);
let user = api.request(endpoint).execute().await?;
```

Root endpoints live directly under `endpoints::*`. Scoped endpoints are nested under their scope module path:

```rust
let endpoint = minimal_api::endpoints::users::GetUser::new(42);
let user = api.request(endpoint).execute().await?;
```

Endpoint setters are available on explicit endpoint values too.

```rust
use concord_core::prelude::PaginationTermination;

let endpoint = example_api::endpoints::ListItems::new()
    .count(50)
    .count_opt(Some(100))
    .clear_count();

let items = api
    .request(endpoint)
    .paginate(PaginationTermination::hard_page_cap(100))
    .collect()
    .await?;
```

The `.paginate(...)` builder is available only for endpoint structs generated from DSL endpoints that declare `paginate ...`, and it requires an explicit `PaginationTermination`.

Use `#[cfg(feature = "dangerous-raw-response")]` with `.execute_raw_response()` when a test or diagnostic needs the classified raw response before endpoint decoding. It still enforces the same bounded response-body limit as decoded execution, so oversized responses fail before raw body material is returned.

Raw execution still applies logical request construction, auth collision validation, rate-limit acquisition, transport materialization, transport send, response classification, hook observation, auth rejection handling, and retry.

```rust
let raw = api
    .request(example_api::endpoints::GetUser::new(42))
    .execute_raw_response()
    .await?;
```

Normal application code should prefer facade methods because they preserve the intended high-level API shape.

## Advanced Endpoint I/O

The generated advanced surfaces are family-specific and keep runtime values free of codec or format parameters.

| Shape | Request | Response | Runtime value / output | Buffered | Map | Pagination |
| --- | ---: | ---: | --- | ---: | ---: | ---: |
| `Json<T>` | yes | yes | `T` | yes | yes | yes, if page-shaped |
| `Text<String>` | yes | yes | `String` | yes | yes | no unless explicitly page-shaped |
| custom buffered codec | yes | yes | decoded codec value | yes | yes | yes, if page-shaped |
| `Stream<M>` | yes | yes | `StreamBody` / `StreamResponse<M>` | no | no | no |
| `Records<T, F>` | yes | yes | `RecordBody<T>` / `RecordStream<T>` | no | no | no |
| `Multipart<T, F>` | yes | yes | `MultipartBody` / `MultipartStream<T>` | no | no | no |
| `Sse<T, C>` | no | yes | `SseStream<T>` | no | no | no |
| `NoContent` | no | yes | `()` | no body | no | no |
| `Bytes` | no | yes | `bytes::Bytes` | yes | yes | no |

- `ContentType` is the shared wire-content trait for buffered codec associated content markers and reserved endpoint I/O media markers.
- Built-in markers include `JsonContentType`, `TextContentType`, `OctetStream`, `NdJson`, `FormData`, `Mixed`, and `EventStream`.
- `Json<T>` is the ordinary buffered JSON codec. `Text<String>` is the ordinary buffered text codec.
- `Stream<M>` uses `StreamBody` for request bodies and `StreamResponse<M>` for responses.
- `Records<T, F>` uses `RecordBody<T>` for requests and `RecordStream<T>` for responses. Supported formats include `NdJson` and `Csv<Cfg>`; the built-in CSV configs are `CsvCommaDelim`, `CsvSemicolonDelim`, and `CsvTabDelim`.
- Record streams can be consumed one record at a time with `next_record()`, or in explicit bounded batches with `next_batch(n)`. The caller chooses `n` at the usage site. Each batch contains up to `n` decoded records, `next_batch(n)` never returns an empty batch, `next_batch(0)` returns `InvalidParam`, batching does not change the endpoint contract, batching does not buffer the full response body, batching does not change transport chunking, and batched record consumption is useful for database bulk inserts, queue publish batches, file/indexing batches, or other sink-specific batching.
- `Multipart<T>` defaults to `Multipart<T, FormData>` and uses `MultipartBody` for requests and `MultipartStream<T>` for responses. Explicit `Multipart<T, F>` remains supported, including `Mixed`.
- `Sse<T>` defaults to `Sse<T, JsonSse>` and uses `SseStream<T>` for responses; SSE is response-only and `JsonSse` decodes event data, not the HTTP wire content type. Explicit `Sse<T, C>` remains supported.
- `Bytes` is response-only, returns `bytes::Bytes`, uses the ordinary bounded buffered response path that materializes payloads in memory, and omits `Accept`; request-side `Bytes` remains invalid. Use `Stream<OctetStream>` for unbounded byte transfer.
- `NoContent` is response-only, returns `()`, and omits `Accept`; request-side `NoContent` remains invalid. The core `NoContent` buffered codec intentionally omits request and response content headers.
- Each family has a dedicated helper on pending requests: `.execute_stream()`, `.execute_records()`, `.execute_multipart()`, or `.execute_sse()`.
- `.execute()` also routes through the family-specific execution path for these endpoint I/O shapes.
- `BodyCodec::try_content_type()` and `ResponseCodec::try_accept()` are the codec-level override points for buffered codecs. `content_type()` and `accept()` are the convenience forms.
- Retry policies remain available for ordinary HTTP endpoints, including buffered responses and supported stream/records/multipart/SSE response endpoints. Stream-like request bodies are not automatically replayed by retry unless a future replayable-body contract is introduced.
- Pagination remains buffered-response-only and is rejected for `Stream`, `Records`, `Multipart`, `Sse`, and `NoContent` endpoint responses. `Bytes` rejects pagination.
- Request-side SSE remains unsupported.
