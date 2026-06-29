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

Use `.execute_raw()` when a test or diagnostic needs the classified raw response before endpoint decoding. It still enforces the same bounded response-body limit as decoded execution, so oversized responses fail before raw body material is returned.

Raw execution still applies logical request construction, auth collision validation, rate-limit acquisition, transport materialization, transport send, response classification, hook observation, auth rejection handling, and retry.

```rust
let raw = api
    .request(example_api::endpoints::GetUser::new(42))
    .execute_raw()
    .await?;
```

Normal application code should prefer facade methods because they preserve the intended high-level API shape.

## Advanced Endpoint I/O

The generated advanced surfaces are family-specific and keep runtime values free of codec or format parameters.

- `ContentType` is the shared wire-content marker trait. Built-in markers include `JsonContentType`, `TextContentType`, `OctetStream`, `NdJson`, `FormData`, `Mixed`, and `EventStream`.

- `Stream<M>` uses `StreamBody` for request bodies and `StreamResponse<M>` for responses.
- `Records<T, F>` uses `RecordBody<T>` for requests and `RecordStream<T>` for responses.
- `Multipart<T>` defaults to `Multipart<T, FormData>` and uses `MultipartBody` for requests and `MultipartStream<T>` for responses.
- `Sse<T>` defaults to `Sse<T, JsonSse>` and uses `SseStream<T>` for responses; SSE is response-only.
- `WebSocket<Out, In>` defaults to `WebSocket<Out, In, JsonWebSocket>` and uses `WebSocketClient<Out, In>`; WebSocket is modeled as `WS` endpoint mode, not a buffered response body.
- Each family has a dedicated helper on pending requests: `.execute_stream()`, `.execute_records()`, `.execute_multipart()`, `.execute_sse()`, or `.execute_websocket()`.
- `.execute()` also routes through the family-specific execution path for these endpoint I/O shapes.
- Retry policies remain available for ordinary HTTP endpoints, including buffered responses and supported stream/records/multipart/SSE response endpoints. Stream-like request bodies are not automatically replayed by retry unless a future replayable-body contract is introduced. `WS` endpoints reject retry policies in v1.
- Map and pagination remain limited to buffered decoded responses and are rejected for the reserved stream-like families.
- Request-side SSE and WebSocket remain unsupported.
