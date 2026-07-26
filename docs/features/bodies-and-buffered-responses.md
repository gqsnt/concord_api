# Request bodies and buffered responses

Choose a request and response specification in the endpoint declaration.
Concord delays request-body work until execution is authorized, bounds wire
bytes, validates successful response media, buffers under the active limit,
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
- `NoContent` is a response specification that returns `()` without polling a
  successful response body.

`NoContent` is not a request-body specification. A bodyless endpoint simply
has no `#[body]` parameter.

The `csv` feature is intentionally a streaming record decoder, not a buffered
body codec. Use [Record streams](records.md) for CSV and NDJSON.

## What happens automatically

Constructing a named request retains the body input but does not encode,
stream, or send it. During execution Concord:

1. prepares and validates the public request and structural body metadata;
2. runs the before-send hook and acquires rate-limit authority;
3. asks the request specification to prepare the managed body;
4. enforces the request-body limit while the final stream crosses transport;
5. accepts a successful 2xx response and acquires it under the selected
   response mode;
6. validates media and decodes the complete buffered bytes.

This ordering prevents rejected hooks or limiters from triggering body
factories and keeps protected authentication placement later than safe
observation. Known lengths that already exceed a limit fail before transport.
Unknown-length bodies remain cumulatively bounded as they stream.

For a non-2xx response, Concord does not run the declared success decoder.
It returns the HTTP-status error category with sanitized response metadata and
bounded status-body state when available. Redirects are disabled by default,
so 3xx responses are final statuses rather than implicit navigation.

## Get response metadata

Await the named request when only the application value is needed:

```rust
let values: Vec<u32> = client.json_round_trip(vec![1, 2, 3]).await?;
```

Select `.response()` before awaiting when status, version, successful headers,
the credential-sanitized logical URL, or safe execution metadata is needed:

```rust
let response = client.text_round_trip("hello".to_owned()).response().await?;
println!("{} {}", response.status(), response.value());
```

The returned headers and public URL may still contain application identifiers.
Redaction protects credential-owned data; it is not a promise that every
public business value is suitable for logs.

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
