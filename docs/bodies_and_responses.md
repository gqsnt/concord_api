# Bodies and responses

The [DSL specification](spec.md#request-and-response-specifications) owns body
and response declaration syntax. This file defines delivery contracts.

## Request body specifications

`RequestBodySpec` maps a public endpoint input into bounded preparation
metadata and a managed request body.

`EncodeBody` supports complete encoded bodies. `RequestBody` supports managed
streaming bodies. Body factories are invoked only after the request reaches
body preparation.

Known lengths are checked before native transport. Unknown-length bodies
withhold the first chunk that would exceed the request limit.

`PageRequestBodySpec` is the trusted marker for paginated body stability. Each
cloned input independently prepares the same body for every page.

## Buffered responses

A buffered response is fully acquired under the operation timeout and finite
response limit before conversion.

`Bytes` returns complete bytes. `Text` validates and returns text. `Json<T>`
decodes a complete JSON body when the `json` feature is enabled. `NoContent`
uses discard mode and returns no body value.

Known response lengths fail before body collection when they exceed the limit.
Unknown lengths withhold and discard the first excess chunk.

`.response().await` returns `ApiResponse<T>` with accepted metadata and the
declared output. `.response_bytes().await` returns `ApiResponse<Bytes>` without
declared decoding for buffered and discard response modes.

## Status acceptance

Each `ResponseBodySpec` defines accepted status codes and an `Accept` header.
The built-in specifications accept 2xx statuses.

An unaccepted status produces `HttpResponseFailure`. The failure may own a
complete bounded `StatusBody`. Its formatting exposes only structural state
and length; explicit accessors expose retained bytes.

## Byte streaming

`Stream` returns a bounded `ByteStream` without media validation.
`Stream<M>` requires `M: MediaType` and validates exact media semantics.

Stream opening occurs inside the request operation. Later polling enforces
cumulative bytes and non-progress limits. Stream errors are redacted.

## Records

`BufferedRecords<D>` collects a `RecordDecoder` into `Vec<D::Record>` inside
the request operation.

`Records<D>` returns a lazy `RecordStream<D>`. Source bytes, decoder staging,
and decoded records are absent from automatic stream formatting.

`Csv<T>` and `NdJson<T>` are complete buffered response specifications.
`Records<Csv<T>>` and `Records<NdJson<T>>` stream the same record framing.
CSV and NDJSON do not encode request bodies.

Record staging, record bytes, item counts, and decoder non-progress are
independently bounded. Buffered collection is all-or-error. Streaming may
yield records before a later error.

## Multipart

`Multipart` is a request-only body specification behind the `multipart`
feature. Multipart response parsing is not provided.

Multipart fields and streams remain bounded by request-body limits. File
content is materialized only when body preparation begins.

## Extension boundary

Custom `RequestBodySpec`, `ResponseBodySpec`, `EncodeBody`, and
`RecordDecoder` implementations receive application data. They own safe error
messages, finite progress, and data handling within their trait contracts.

Native Reqwest request or response objects do not cross these traits.

## Related

- [Runtime model](runtime.md)
- [Advanced endpoints](advanced_endpoints.md)
- [Customization](customization.md)
