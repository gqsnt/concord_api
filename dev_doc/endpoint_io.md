# Endpoint I/O Design Contract

## Purpose

This document is a design contract for future endpoint I/O implementation PRs. It defines the intended model and the constraints later work must preserve, but it does not itself introduce runtime behavior, DSL support, code generation, or public API changes.

The target is an endpoint I/O model expansion, not a single-feature "streaming" change.

## Scope Of This Contract

This contract covers:

- endpoint I/O families
- reserved endpoint I/O names
- semantic classification rules
- runtime ordering constraints
- retry and replay safety
- auth refresh behavior
- policy compatibility
- body-free observability
- future public API direction

It does not implement any of those behaviors.

## Current Baseline

The current implementation distinguishes buffered codecs from the reserved endpoint I/O families that now have dedicated runtime support.

- Macro raw AST and semantic IR carry explicit endpoint I/O shapes.
- `CodecSpec` remains the buffered-codec shape for non-reserved families.
- Current codegen still emits buffered codec calls through the existing `BodyCodec` and `ResponseCodec` paths for non-reserved families.
- Current core request bodies are buffered for non-reserved families.
- Current response transport is chunk-capable, and dedicated stream/record/multipart/SSE runtime paths avoid buffering where appropriate.
- `.execute_raw()` remains bounded-buffered.
- Custom buffered codecs are already open-ended and must stay that way.

## Architectural Boundaries

- `concord_macros` owns DSL syntax, raw AST, semantic classification, diagnostics, and generated Rust facade shape.
- `concord_core` owns runtime execution and must remain syntax-neutral.
- Core must not know macro AST types, DSL spellings, reserved-name detection, or parser syntax.
- Codegen consumes resolved semantic I/O shapes, not raw parser syntax.
- Raw AST may preserve invalid forms long enough to produce good diagnostics.
- Resolved IR must not contain impossible endpoint I/O states.
- Behavior profiles lower before runtime and do not change endpoint I/O runtime semantics directly.

## Endpoint I/O Families

### BufferedCodec

BufferedCodec is the default family. Everything non-reserved is a buffered codec.

- Examples: `Json<T>`, `Text<String>`, `Cbor<T>`, `Bitcode<T>`, `Compact<T>`, and project-specific codec markers.
- Uses the existing full-body buffered traits: `BodyCodec` and `ResponseCodec`.
- Encodes the full request into bytes.
- Reads the full response under buffered body limits before decode.
- Supports the current buffered-only behavior for decode, map, pagination, and retry when otherwise safe.
- `Json<T>` is not sema-special.
- Custom codec markers must continue to work through the ordinary buffered codec extension path.

### BufferedBytes

`Bytes` is a reserved endpoint I/O spelling for full buffered bytes.

- It is subject to buffered request and response body limits.
- It is not a stream.
- It may reuse buffered internals.
- It is reserved for diagnostics and semantic clarity.

For large or unbounded byte transfer, future PRs should use `Stream<OctetStream>` rather than trying to stretch `Bytes`.

### NoContent

`NoContent` is a reserved no-response-body spelling.

- It is not a stream.
- It may reuse existing no-content codec behavior internally if appropriate.
- It should allow better sema diagnostics than treating it as a custom codec.

### RawStream

`Stream<M>` is the reserved raw HTTP byte stream family.

- `M` is a media marker implementing a future `MediaType` trait.
- Request value direction: `StreamBody`.
- Response value direction: `StreamResponse<M>`.
- The DSL owns media type; runtime values own only source, sink, and consumption state.
- It handles file upload, download, and proxying in future PRs.
- It must not be implemented through `BodyCodec` or `ResponseCodec`.
- It does not imply replayable request bodies.

### Records

`Records<T, F>` is the reserved typed record-stream family.

- `T` is the item type.
- `F` is the record format.
- `Records<T>` has no default format in the initial contract; callers must spell the format explicitly, such as `Records<LogEntry, NdJson>`.
- Runtime values are `RecordBody<T>` and `RecordStream<T>`; they are not format-generic.
- Custom formats implement `MediaType + RecordFormat<T>`.
- The first intended built-in format is `NdJson`.
- CSV is not part of the initial design.
- Request bodies are stream-like and non-replayable.
- Response bodies are incremental and do not support map or pagination.
- It must not be implemented through `BodyCodec` or `ResponseCodec`.

### Multipart

`Multipart<T>` and `Multipart<T, F>` are reserved multipart families.

- `Multipart<T>` defaults to `Multipart<T, FormData>`.
- Core request-side runtime values are `MultipartBody` and `RawPart`.
- `MultipartBody` is not format-generic; the multipart format is selected by the plan/format generic `F`.
- Core request-side traits are `MultipartFormat`, with built-in formats `FormData` and `Mixed`.
- `MultipartBody` lowers to stream-backed transport bodies with generated boundaries and CRLF framing.
- Multipart request bodies are stream-like and non-replayable.
- Multipart request bodies use the existing stream byte limits.
- Macro/codegen support now exists for `Multipart<T>` and `Multipart<T, F>`:
  - generated request endpoints accept `MultipartBody`;
  - generated response endpoints return `MultipartStream<T>`;
  - `Multipart<T>` defaults to `Multipart<T, FormData>`;
  - multipart responses reject map and pagination;
  - runtime values remain non-format-generic.
- Multipart response parsing continues to use `MultipartStream<T>` and `RawResponsePart` at runtime.
- `Related` and `ByteRanges` are later possibilities.
- Nested multipart, derive macros, automatic filename inference, and byteranges semantics are out of scope initially.
- It must not be implemented through `BodyCodec` or `ResponseCodec`.

### Sse

`Sse<T>` and `Sse<T, C>` are reserved server-sent event families.

- `Sse<T>` defaults to `Sse<T, JsonSse>`.
- Core runtime support now exists for SSE responses.
- Macro/codegen support for `Sse<T>` and `Sse<T, C>` remains future work.
- Runtime response value: `SseStream<T>`.
- Runtime codec trait: `SseCodec<T>`.
- Built-in codec: `JsonSse`.
- SSE responses parse `text/event-stream` incrementally and expose decoded events through `SseStream<T>`.
- SSE responses are stream-like and do not support map or pagination.
- SSE reconnect, `Last-Event-ID` resume, and browser/EventSource compatibility remain future work.
- It must not be implemented through `ResponseCodec`.

### WebSocketEndpoint

`WebSocket<Out, In>` and `WebSocket<Out, In, C>` are reserved WebSocket endpoint families.

- Endpoint method mode: `WS`.
- `WebSocket<Out, In>` defaults to `WebSocket<Out, In, JsonWebSocket>`.
- WebSocket is endpoint mode, not an HTTP response body shape.
- Future value: `WebSocketClient<Out, In>`.
- Future trait: `WebSocketCodec<Out, In>`.
- Rate-limit may apply to the handshake.
- Auth, header, query, and path construction apply before upgrade.
- Reconnect, replay, pooling, multiplexing, and server-side WebSocket are out of scope initially.
- HTTP response body planning must not be contaminated with WebSocket semantics.

## Reserved Endpoint I/O Names

The reserved top-level endpoint I/O names are:

- `Bytes`
- `NoContent`
- `Stream`
- `Records`
- `Multipart`
- `Sse`
- `WebSocket`

Only these families are sema-special.

| Family | Valid forms | Defaulting |
| --- | --- | --- |
| `Bytes` | `Bytes` | none |
| `NoContent` | `NoContent` | none |
| `Stream` | `Stream<M>` | none |
| `Records` | `Records<T, F>` | none initially |
| `Multipart` | `Multipart<T>`, `Multipart<T, F>` | `Multipart<T>` defaults to `FormData` |
| `Sse` | `Sse<T>`, `Sse<T, C>` | `Sse<T>` defaults to `JsonSse` |
| `WebSocket` | `WebSocket<Out, In>`, `WebSocket<Out, In, C>` | two-arg form defaults to `JsonWebSocket` |

- Reserved-name detection is a macro and sema concern only.
- Core must not know these names as DSL syntax.
- Reserved names are special only in endpoint I/O position.
- Reserved names should produce good diagnostics for invalid arity or invalid endpoint mode.

## Non-Reserved Types Are Buffered Codecs

`Json<T>` is not reserved.

Everything non-reserved is classified as `BufferedCodec`.

Examples:

```rust
Json<T>
Text<String>
Cbor<T>
Bitcode<T>
Compact<T>
MyCodec<T>
```

All of these must continue to work as ordinary buffered codec markers.

Custom buffered codec extensibility is part of the public contract and must not regress.

## Target Semantic Model

This is the intended future semantic direction for later PRs, not a required PR87 implementation.

```rust
enum EndpointModeIr {
    Http(HttpEndpointIr),
    WebSocket(WebSocketEndpointIr),
}

struct HttpEndpointIr {
    request_body: RequestBodyShape,
    response_body: ResponseBodyShape,
    // existing route, params, auth, retry, rate-limit, paginate, map, etc.
}

enum RequestBodyShape {
    None,

    BufferedCodec {
        codec_ty: TypeRef,
        value_ty: TypeRef,
    },

    BufferedBytes,

    RawStream {
        media_ty: TypeRef,
    },

    Records {
        item_ty: TypeRef,
        format_ty: TypeRef,
    },

    Multipart {
        value_ty: TypeRef,
        format_ty: TypeRef,
    },
}

enum ResponseBodyShape {
    NoContent,

    BufferedCodec {
        codec_ty: TypeRef,
        value_ty: TypeRef,
    },

    BufferedBytes,

    RawStream {
        media_ty: TypeRef,
    },

    Records {
        item_ty: TypeRef,
        format_ty: TypeRef,
    },

    Multipart {
        part_ty: TypeRef,
        format_ty: TypeRef,
    },

    Sse {
        event_ty: TypeRef,
        codec_ty: TypeRef,
    },
}

struct WebSocketEndpointIr {
    outbound_ty: TypeRef,
    inbound_ty: TypeRef,
    codec_ty: TypeRef,
    // route, params, auth, rate-limit, retry/handshake policy if allowed
}
```

## Runtime Value Rule

The DSL owns media, format, and protocol. Runtime values own only data, source, sink, and consumption state.

Correct examples:

```rust
let body = StreamBody::from_file("song.mp3").await?;
api.upload_song(body).await?;
```

```rust
let body = RecordBody::<LogEntry>::from_iter(entries);
api.import_logs(body).await?;
```

Incorrect examples:

Do not make `StreamBody` generic over `Mp3` or any other media marker.

```rust
RecordBody::<LogEntry, NdJson>::from_iter(...)
```

Generated facade methods should stay concrete and usage-first:

```rust
upload(body: StreamBody)
import(body: RecordBody<LogEntry>)
download() -> StreamResponse<Mp3>
logs() -> RecordStream<LogEntry>
```

Avoid broad endpoint parameters such as `upload<B: Into<StreamBody>>(body: B)` unless later evidence shows the tradeoff is worth it.

## Policy Compatibility

### Map

- `map` is allowed only when the response is buffered and decoded.
- A streaming request with a buffered response may still allow `map`.
- `map` is rejected for `Stream` responses, `Records` responses, `Multipart` responses, `Sse` responses, and `WebSocket` endpoints.

### Pagination

- Pagination is allowed only for buffered decoded responses.
- Pagination controllers operate on decoded page values.
- Pagination may mutate subsequent logical requests.
- `Records<T, F>` and `Sse<T, C>` are not pages.
- Pagination is rejected for `Stream` responses, `Records` responses, `Multipart` responses, `Sse` responses, and `WebSocket` endpoints.

### Retry And Replay

- Ordinary retry is unsafe for single-use request bodies.
- Runtime must not automatically replay stream-like request bodies unless a future replayable-body design exists.
- Future stream-like request bodies include raw streams, record streams, and multipart bodies containing streams or otherwise single-use sources.
- Buffered codecs retain current retry behavior when otherwise safe.
- Future explicit replayable-body contracts are out of scope for this endpoint I/O contract.

### Auth Refresh

- Auth rejection handling may invalidate rejected credential generations.
- For consumed or potentially consumed stream-like request bodies, auth refresh must not automatically replay the protected request.
- Endpoint-backed credentials remain manual from the protected request's perspective.
- Runtime must not automatically call endpoint-backed auth endpoints for protected request replay.

### Rate-Limit Ordering

- Rate-limit acquisition must happen before request body stream consumption.
- Auth collision validation must happen before request body stream consumption.
- Removing cache does not remove rate-limit ordering constraints.

### Observability And Redaction

- Hooks, debug sinks, retry metadata, rate-limit metadata, errors, and diagnostics must remain body-free and auth-safe.
- They must not see live request body bytes, response body bytes, multipart part bytes, record values, SSE payloads, WebSocket messages, or raw auth material.
- Raw auth material remains confined to the transport send boundary.

## Transport Direction

Future endpoint I/O work will need a request body enum instead of a single buffered payload.

```rust
pub enum TransportRequestBody {
    Empty,
    Bytes(Bytes),
    Stream(TransportByteStream),
}
```

- The current `Option<Bytes>` request body representation is not sufficient for endpoint I/O expansion.
- Future PRs should introduce a request body enum.
- Existing response transport is already chunk-capable and should be preserved or reused.
- Do not create unnecessary special transport paths for `Bytes` or `NoContent` unless current code requires it.
- `Bytes` and `NoContent` may reuse buffered internals.

## Advanced Execution Surfaces

- `.execute_raw()` remains bounded-buffered.
- Do not silently change `.execute_raw()` into a streaming API.
- Future advanced streaming execution should use a separate method, such as `execute_stream()` or `execute_raw_stream()`.
- Normal facade methods remain the preferred surface.

## WebSocket Mode Separation

- WebSocket is an endpoint mode.
- WebSocket is not a response body shape.
- HTTP `ResponseBodyShape` must not contain WebSocket.
- `WS` endpoints must return `WebSocket<...>`.
- HTTP endpoints must not return `WebSocket<...>`.
- WebSocket implementation can be later and optional behind a backend feature if required.

## Runtime Configuration Direction

- Do not add DSL knobs for chunk size, reconnect behavior, record byte limits, idle timeout, multipart limits, or WebSocket subprotocols unless there is no clean Rust-trait, request-builder, or runtime-config alternative.
- Detailed behavior belongs in runtime config, request builders, and explicit Rust traits.
- Buffered body limits and stream-specific limits should remain separate in future PRs.

## Explicit Non-Goals

- No runtime implementation in PR87.
- No DSL support in PR87.
- No macro parser, sema, or codegen change in PR87.
- No public API change in PR87.
- No public docs expansion in PR87 unless a docs index requires a link.
- No stream retry or replay design in PR87.
- No automatic SSE reconnect.
- No CSV implementation.
- No nested multipart.
- No multipart derive macros.
- No WebSocket reconnect.
- No WebSocket pooling.
- No WebSocket multiplexing.
- No cache reintroduction.

## Cache Is Removed

Cache has been removed from Concord.

- Endpoint I/O expansion must not reintroduce cache directly or indirectly.
- Do not design around cache admission, stale fallback, cache identity, cache keys, cache body limits, cache compatibility, or cache-like behavior under another name.
- Any remaining stale cache mention in the repository is cleanup debt, not an active design constraint.

## Review Checklist For Future PRs

- Does the PR respect crate boundaries?
- Does core remain syntax-neutral?
- Does codegen consume resolved semantic data, not raw AST?
- Are only reserved names sema-special?
- Is `Json<T>` still an ordinary buffered codec?
- Do custom buffered codecs still work?
- Are streaming families kept out of `BodyCodec` and `ResponseCodec`?
- Are runtime values format-free?
- Is WebSocket modeled as endpoint mode, not response body?
- Is `.execute_raw()` still bounded-buffered?
- Is body and auth redaction preserved?
- Is body-free observability preserved?
- Is retry and auth replay safe for non-replayable bodies?
- Does rate-limit acquisition happen before stream consumption?
- Are reserved family arities and defaults preserved exactly?
- Are docs and examples free of cache as an active feature?
