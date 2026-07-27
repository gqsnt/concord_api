# Request bodies and buffered responses

Choose a request and response specification in the endpoint declaration.
Concord delays request-body work until execution is authorized, bounds wire
bytes, validates accepted response media, buffers under the active limit,
and only then decodes the application value.

## Declare the representation

Mark the request-body parameter explicitly with `#[body]`:

## Complete example

```rust,ignore
{{#include ../../crates/concord-examples/src/request_response/bodies.rs:buffered_endpoints}}
```

The type in the declaration is a specification, while the generated method
accepts its input:

```rust,ignore
{{#include ../../crates/concord-examples/src/request_response/bodies.rs:buffered_requests}}
```

For example, `Json<Vec<u32>>` is the declaration type and the method accepts
`Vec<u32>`. The ordinary await terminal returns the decoded `Vec<u32>`.
Enable the `json` feature for `Json<T>` and provide the required Serde traits.

Core representations need no optional media feature:

- `Bytes` sends or returns owned bytes without a media codec;
- `Text` accepts and returns `String` with its text media contract;
- `NoContent` is a response specification that returns `()` without polling an
  accepted response body.

`NoContent` is not a request-body specification. A bodyless endpoint simply
has no `#[body]` parameter.

The `csv` feature is intentionally a streaming record decoder, not a buffered
body codec. Use [Record streams](records.md) for CSV and NDJSON.

## What happens automatically

Constructing a named request retains the body input but does not encode,
stream, or send it. During execution Concord:

1. prepares and validates the public request and structural body metadata;
2. runs the before-send hook;
3. prepares or reconstructs the managed body;
4. performs current authentication preflight without placing credentials;
5. acquires rate-limit authority;
6. materializes private authentication and enforces the request-body limit
   while the final body crosses transport;
7. runs limiter response observation, the response-head hook, and
   authentication challenge handling;
8. applies the declared status-acceptance policy (2xx by default) and acquires
   an accepted response under the selected response mode;
9. validates media and decodes the complete buffered bytes.

This ordering lets a hook veto body work, keeps limiter admission before
credential placement, and ensures response policy observes every received
head before acceptance or decoding. A limiter can reject after the body has
been prepared, but before private materialization and transport. Known
lengths that already exceed a limit fail before transport. Unknown-length
bodies remain cumulatively bounded as they stream.

A custom `ResponseBodySpec` can accept a non-2xx operation outcome such as
`404`. It then follows ordinary declared acquisition, validation, and
conversion under the response limit. Authentication challenge handling remains
authoritative before this decision.

For a rejected status, Concord does not run the declared response decoder. It
returns the HTTP-status error category with sanitized response metadata and
bounded status-body state when available. Redirects are disabled by default, so
3xx responses are final statuses rather than implicit navigation unless the
response specification explicitly accepts one.

## Get response metadata

Await the named request when only the application value is needed:

```rust,ignore
{{#include ../../crates/concord-examples/src/request_response/acquisition.rs:decoded_response}}
```

Select `.response()` before awaiting when status, version, accepted headers,
the credential-sanitized logical URL, or safe execution metadata is needed:

```rust,ignore
{{#include ../../crates/concord-examples/src/request_response/acquisition.rs:metadata_response}}
```

The returned headers and public URL may still contain application identifiers.
Redaction protects credential-owned data; it is not a promise that every
public business value is suitable for logs.

## Get explicit response bytes

Use `.response_bytes()` when the endpoint's declared status acceptance and
`Accept` header are right but the caller needs the complete bounded bytes
instead of validation and decoding:

```rust,ignore
{{#include ../../crates/concord-examples/src/request_response/acquisition.rs:response_bytes}}
```

This returns `ApiResponse<Bytes>` without a DSL clause or Cargo feature.
Streaming response modes reject this terminal through a sealed Rust trait
bound because their bodies are deliberately caller-driven.

## Accept an optional not-found result

A response specification can opt an operation into an accepted non-2xx
status. This compiled example maps `404` to `None` while retaining normal
buffered acquisition for an accepted UTF-8 body:

```rust,ignore
{{#include ../../crates/concord-examples/src/request_response/acquisition.rs:optional_response_spec}}
```

The endpoint selects that specification in the same compiled module:

```rust,ignore
{{#include ../../crates/concord-examples/src/request_response/acquisition.rs:response_acquisition_contract}}
```

The ordinary generated call then returns the optional output:

```rust,ignore
{{#include ../../crates/concord-examples/src/request_response/acquisition.rs:accepted_not_found}}
```

Accepted `404` uses the declared buffered-response limit and is never a retry
candidate. Limiter observation, the response hook, and authentication
challenge handling still precede acceptance.

## Codec versus body specification

Implement a simple buffered media conversion with `EncodeBody` and
`DecodeBody`. Concord provides blanket body/response specifications that apply
the managed buffered lifecycle around those codecs.

Implement `RequestBodySpec` or `ResponseBodySpec` only when you need to choose
structural metadata, lazy preparation, or a sealed acquisition mode. A body
specification is lifecycle policy; a codec is a transformation of bounded
bytes. The custom example in `concord-examples` uses the same public traits
available to applications.

Wire and staging limits bound bytes retained by Concord, not the heap size,
CPU, or side effects of the decoded application value. Custom codecs must
bound their own work and emit only safe errors.

## Related

- [Streaming responses](streaming.md)
- [Multipart uploads](multipart.md)
- [Record streams](records.md)
- [Limits reference](../reference/limits.md)
- [Error reference](../reference/errors.md)
