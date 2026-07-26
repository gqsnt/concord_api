# Codecs

Implement a codec when a service uses a complete buffered media format that
Concord does not provide. A request codec converts one application value to
bounded bytes; a response codec validates media metadata and converts complete
bounded bytes to one application value.

Use the exact Rustdoc items `concord::codec::EncodeBody` and
`concord::codec::DecodeBody`. Implementing either trait automatically supplies
the matching full body specification.

## Complete example

This compiler-owned example implements both directions for one custom media
type:

```rust,ignore
{{#include ../../crates/concord-examples/src/request_response/specifications.rs:codec_extension}}
```

The same `ExampleCodec` appears in this compiled declaration as both the body
type and response type:

```rust,ignore
{{#include ../../crates/concord-examples/src/request_response/bodies.rs:custom_codec_endpoint}}
```

The example module is compiled by `concord-examples` and exercised through the
same generated and runtime-defined body contracts.

## Request encoding

`EncodeBody::Input` is the type stored by the pending generated request.
`CONTENT_TYPE` is either one static media type or `None`; Concord owns
insertion of that header for the body.

`encode` runs synchronously, at most once for one pending execution, after the
before-send hook approves the attempt. Its `EncodeContext` contains only:

- the stable endpoint ID;
- the HTTP method;
- the active positive body limit when one applies.

It contains no URL, headers, credentials, limiter state, request handle, or
transport client. Check `context.body_limit` before allocating a result when
the format can predict its encoded size. Concord also rejects returned bytes
that exceed the active request-body ceiling.

Return `CodecError::new(CodecErrorKind::Encode)` for ordinary serialization
failure and `CodecErrorKind::Limit` when the codec detects its own finite
ceiling. `CodecError` deliberately retains no application message or source
error.

## Response decoding

`DecodeBody::Output` is the successful endpoint output. `ACCEPT` supplies a
default `Accept` value only when public request shaping has not already set
one.

Concord handles a response in this order:

1. reduce non-success status;
2. preflight a known post-decompression length;
3. call `validate_content_type`;
4. collect the complete body under the configured buffered-response limit;
5. call `decode`.

`DecodeContext` exposes the endpoint ID, method, successful status, and parsed
content type. The `Bytes` argument is already bounded and complete. It may
still contain sensitive application data.

The example requires exact `application/vnd.concord.example` text. A real
codec can parse media types and accept documented parameters, but it should
reject ambiguous or incompatible input before decoding. Use
`CodecErrorKind::Decode` without retaining parser text, response bytes, field
values, or headers.

## What the blanket implementations add

An `EncodeBody` implementation receives the
`concord::body::RequestBodySpec` blanket implementation. Concord computes
buffered structural metadata, calls the codec after hook approval, enforces the
request limit, and produces a managed reusable byte body.

A `DecodeBody` implementation receives the
`concord::response::ResponseBodySpec` blanket implementation with
`concord::response::Buffered` mode. Media validation happens before body
polling; decoder failure is reduced to the stable response-decoding category.

If the response output implements `IntoIterator`, the codec also gets
`concord::response::PageResponseSpec` and can back generated pagination. A
streaming format whose records should arrive incrementally belongs behind
`RecordDecoder`, not a buffered codec.

## Trusted work and bounds

Both callbacks are synchronous. Concord checks the operation deadline after
they return, but it cannot interrupt a long-running serializer or parser.
Bound CPU, recursion, output growth, temporary allocations, application
queues, and any cache owned by the codec. Do not perform network calls, sleep,
spawn detached work, or implement an internal retry loop.

Calls for independent operations may overlap. Stateless marker types are
usually the safest design. If an implementation uses shared state, synchronize
it without holding a lock across unbounded work.

Concord's byte limit bounds retained encoded or received bytes, not the heap
size of the decoded output. A small compressed or compact input can expand
into a large object graph, so the codec must enforce format-specific nesting,
collection, and expansion limits where needed.

## Verification

Test both directions at exact accepted limits and first excess. Include:

- missing, correct, parameterized, and incorrect content types;
- malformed input and invalid text encodings;
- empty values and the largest accepted value;
- inputs containing sentinel secrets that must not appear in `Display` or
  `Debug` errors;
- generated and `RuntimeEndpoint` execution through deterministic local HTTP.

The exact context and error contracts live in Rustdoc at
`concord::codec::EncodeContext`, `concord::codec::DecodeContext`,
`concord::codec::CodecError`, and `concord::codec::CodecErrorKind`.

## Related

- [Request and response body specifications](body-specifications.md)
- [Record decoders](record-decoders.md)
- [Request bodies and buffered responses](../features/bodies-and-buffered-responses.md)
- [Finite behavior](../concepts/finite-behavior.md)
- [Error reference](../reference/errors.md)
- Rustdoc: `concord::codec::EncodeBody` and
  `concord::codec::DecodeBody`
