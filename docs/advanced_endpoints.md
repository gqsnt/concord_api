# Advanced Endpoints

The facade-first client is the normal API. Advanced endpoint structs are available under `endpoints::*` for focused tests, reusable endpoint values, and explicit request construction.

See [Security Model](security_model.md) for the boundary between normal, advanced, and dangerous surfaces.

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

Use `#[cfg(feature = "dangerous-raw-response")]` with `concord_core::dangerous::BuiltResponse` and `.execute_raw_response()` when a test or diagnostic needs the classified raw response before endpoint decoding. This dangerous escape hatch lives under `concord_core::dangerous`, and it still enforces the same bounded response-body limit as decoded execution, so oversized responses fail before raw body material is returned.

Raw execution still applies logical request construction, auth collision validation, rate-limit acquisition, transport materialization, visible execution, response classification, hook observation, and bounded auth rejection handling. Reqwest may perform hidden resends according to the client-level mode.

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
| `Multipart<T>` | yes | no | `MultipartBody` | no | no | no |
| `NoContent` | no | yes | `()` | no body | no | no |
| `Bytes` | no | yes | `bytes::Bytes` | yes | yes | no |

- `ContentType` is the shared wire-content trait for buffered codec associated content markers and reserved endpoint I/O media markers.
- Built-in markers include `JsonContentType`, `TextContentType`, `OctetStream`, and `FormData`.
- `Json<T>` is the ordinary buffered JSON codec. `Text<String>` is the ordinary buffered text codec.
- `Stream<M>` uses `StreamBody` for request bodies and `StreamResponse<M>` for responses.
- `Multipart<T>` uses `MultipartBody` as a recipe for request-side `multipart/form-data` construction. It constructs native `reqwest::multipart::Form` and `Part` values only for a visible execution; Reqwest owns the boundary and complete `Content-Type` value.
- Multipart with a one-shot stream part cannot perform authentication recovery unless a complete multipart factory can reconstruct every part. An all-reusable direct multipart recipe supports one bounded authentication recovery, which builds a fresh form and boundary. Materialized multipart is never Reqwest-cloneable and therefore is not resent by status mode.
- `Bytes` is response-only, returns `bytes::Bytes`, uses the ordinary bounded buffered response path that materializes payloads in memory, and omits `Accept`; request-side `Bytes` remains invalid. Use `Stream<OctetStream>` for unbounded byte transfer.
- `NoContent` is response-only, returns `()`, and omits `Accept`; request-side `NoContent` remains invalid. The core `NoContent` buffered codec intentionally omits request and response content headers.
- `Stream<M>` has the dedicated `.execute_stream()` helper; `.execute()` also returns its stream response.
- `StreamResponse<M>` keeps the native response as its normal authority. `next_chunk()` and `write_to_file()` consume data chunks only and skip trailers; EOF, a native body error, or a streaming-limit error permanently terminates data-only consumption. The explicit `into_body()` escape hatch uses one narrow private native-body wrapper that preserves remaining data and trailer frames, frame order, byte accounting, and the native size hint in the returned `DynBody`. Extraction after a terminal data-only result remains terminal, and normal streaming does not pass through `DynBody`.
- `BodyCodec::try_content_type()` and `ResponseCodec::try_accept()` are the codec-level override points for buffered codecs. `content_type()` and `accept()` are the convenience forms.
- General retry is selected at client construction. Reqwest hidden retries use only Reqwest-cloneable materialized bodies. A complete `PreparedBody` factory can make a body rebuildable for Concord authentication recovery, but does not make a stream, advanced body, or multipart cloneable by Reqwest.
- Pagination remains buffered-response-only and is rejected for `Stream` and `NoContent` endpoint responses. `Bytes` rejects pagination.
