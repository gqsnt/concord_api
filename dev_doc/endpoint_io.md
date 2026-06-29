# Endpoint I/O Design Contract

## Purpose

This document records the endpoint I/O contract and the current implementation surface. It defines the model and the constraints later work must preserve, but it does not itself introduce new runtime behavior, DSL support, code generation, or public API changes.

## Current Baseline

The current implementation distinguishes buffered codecs from the reserved endpoint I/O families that have dedicated runtime support and generated endpoint support.

- Macro raw AST and semantic IR carry explicit endpoint I/O shapes.
- Resolved semantic IR carries HTTP request and response body I/O only.
- Non-reserved families keep the buffered-codec shape used by `BodyCodec` and `ResponseCodec`.
- `ContentType` is the shared wire-content marker trait for buffered codecs and reserved family markers.
- Buffered codecs continue to use buffered request bodies and buffered response decode.
- Dedicated runtime paths exist for `Stream`, `Records`, `Multipart`, and `Sse` families so they do not have to buffer the whole body.
- Macro/codegen support exists for `Stream<M>`, `Records<T, F>`, `Multipart<T, F>`, and `Sse<T, C>`.
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

For large or unbounded byte transfer, use `Stream<OctetStream>` rather than trying to stretch `Bytes`.

### NoContent

`NoContent` is a reserved no-response-body spelling.

- It is not a stream.
- It may reuse existing no-content codec behavior internally if appropriate.
- It should allow better sema diagnostics than treating it as a custom codec.
- The core `NoContent` codec exists for ordinary buffered endpoints. The reserved endpoint I/O spelling `NoContent` remains unsupported in the DSL unless and until it is explicitly implemented.

### RawStream

`Stream<M>` is the reserved raw HTTP byte stream family.

- `M` is a marker type implementing `ContentType`.
- Request value direction: `StreamBody`.
- Response value direction: `StreamResponse<M>`.
- The DSL owns media type; runtime values own only source, sink, and consumption state.
- It handles file upload and download, and can be reused for proxying paths when needed.
- It must not be implemented through `BodyCodec` or `ResponseCodec`.
- It does not imply replayable request bodies.

### Records

`Records<T, F>` is the reserved typed record-stream family.

- `T` is the item type.
- `F` is the record format.
- `Records<T>` has no default format in the initial contract; callers must spell the format explicitly, such as `Records<LogEntry, NdJson>`.
- Runtime values are `RecordBody<T>` and `RecordStream<T>`; they are not format-generic.
- Custom formats implement `ContentType + RecordFormat<T>`.
- The first intended built-in format is `NdJson`.
- CSV is planned as a future `Records<T, Csv<Cfg>>` format but is not implemented yet. The design contract lives in [csv_records.md](csv_records.md).
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
- Macro/codegen support exists for `Multipart<T>` and `Multipart<T, F>`:
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
- Core runtime support exists for SSE responses.
- Macro/codegen support exists for `Sse<T>` and `Sse<T, C>`.
- Generated response endpoints return `SseStream<T>`.
- Generated endpoints expose `.execute_sse()`, and `.execute()` also routes through SSE execution.
- Runtime response value: `SseStream<T>`.
- Runtime codec trait: `SseCodec<T>`.
- Built-in codec: `JsonSse`.
- SSE responses parse `text/event-stream` incrementally and expose decoded events through `SseStream<T>`.
- SSE responses are stream-like and do not support map or pagination.
- SSE reconnect, `Last-Event-ID` resume, and browser/EventSource compatibility remain future work.
- It must not be implemented through `ResponseCodec`.

## Reserved Endpoint I/O Names

The reserved top-level endpoint I/O names are:

- `Bytes`
- `NoContent`
- `Stream`
- `Records`
- `Multipart`
- `Sse`

Only these families are sema-special.

| Family | Valid forms | Defaulting |
| --- | --- | --- |
| `Bytes` | `Bytes` | none |
| `NoContent` | `NoContent` | none |
| `Stream` | `Stream<M>` | none |
| `Records` | `Records<T, F>` | none initially |
| `Multipart` | `Multipart<T>`, `Multipart<T, F>` | `Multipart<T>` defaults to `FormData` |
| `Sse` | `Sse<T>`, `Sse<T, C>` | `Sse<T>` defaults to `JsonSse` |

- Reserved-name detection is a macro and sema concern only.
- Core must not know these names as DSL syntax.
- Reserved names are special only in endpoint I/O position.
- Reserved names should produce good diagnostics for invalid arity or unsupported endpoint methods.

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

## Resolved Semantic Model

The current resolved semantic model stores resolved HTTP endpoint I/O directly on each endpoint.

```rust
struct ResolvedEndpoint {
    io: ResolvedHttpEndpointIo,
}

struct ResolvedHttpEndpointIo {
    request: RequestBodyShape,
    response: ResponseBodyShape,
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
RecordBody::<LogEntry>::from_iter(...)

MultipartBody

SseStream<Event>
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
- `map` is rejected for `Stream` responses, `Records` responses, `Multipart` responses, and `Sse` responses.

### Pagination

- Pagination is allowed only for buffered decoded responses.
- Pagination controllers operate on decoded page values.
- Pagination may mutate subsequent logical requests.
- `Records<T, F>` and `Sse<T, C>` are not pages.
- Pagination is rejected for `Stream` responses, `Records` responses, `Multipart` responses, and `Sse` responses.

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
- They must not see live request body bytes, response body bytes, multipart part bytes, record values, SSE payloads, or raw auth material.
- Raw auth material remains confined to the transport send boundary.

## Transport Direction

TransportRequestBody already models request body kind explicitly.

```rust
pub enum TransportRequestBody {
    Empty,
    Bytes(Bytes),
    Stream(TransportByteStream),
}
```

- The current transport request body enum is the request/transport boundary and should be preserved.
- Existing response transport is already chunk-capable and should be preserved or reused.
- Do not create unnecessary special transport paths for `Bytes` or `NoContent` unless current code requires it.
- `Bytes` and `NoContent` may reuse buffered internals.

## Advanced Execution Surfaces

- `.execute_raw()` remains bounded-buffered.
- Do not silently change `.execute_raw()` into a streaming API.
- Advanced execution is already split across family-specific helpers:
  - `.execute_stream()` for `Stream<M>`
  - `.execute_records()` for `Records<T, F>`
  - `.execute_multipart()` for `Multipart<T, F>`
  - `.execute_sse()` for `Sse<T, C>`
- `.execute_raw()` remains bounded-buffered and intentionally separate.
- Normal facade methods remain the preferred surface.

## Runtime Configuration Direction

- Do not add DSL knobs for chunk size, record byte limits, idle timeout, or multipart limits unless there is no clean Rust-trait, request-builder, or runtime-config alternative.
- Detailed behavior belongs in runtime config, request builders, and explicit Rust traits.
- Buffered body limits and stream-specific limits remain separate.

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
- Is `.execute_raw()` still bounded-buffered?
- Is body and auth redaction preserved?
- Is body-free observability preserved?
- Is retry and auth replay safe for non-replayable bodies?
- Does rate-limit acquisition happen before stream consumption?
- Are reserved family arities and defaults preserved exactly?
- Are docs and examples free of cache as an active feature?
