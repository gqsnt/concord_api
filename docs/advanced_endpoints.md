# Advanced endpoints

This file owns runtime-defined endpoints, explicit endpoint values, streaming
selection, records, and multipart composition.

## Explicit generated endpoints

An explicit endpoint value separates endpoint construction from client
binding:

```rust
let endpoint = catalog_api::ById::new("42".to_owned());
let request = client.request(endpoint);
```

The explicit value exposes its safe description and inherited value controls.
Binding it to a client produces the same named request wrapper as the direct
method.

## Runtime-defined endpoints

`RuntimeEndpoint::builder(EndpointId, Method)` builds a typed endpoint at
runtime. The builder selects public host, path, query, headers, body
specification, response specification, authentication, rate-limit policy, and
timeout state through bounded semantic values.

```rust
use concord::endpoint::{EndpointId, RuntimeEndpoint};
use concord::http::Method;

let endpoint = RuntimeEndpoint::builder(
    EndpointId::new("runtime.health")?,
    Method::GET,
)
.path_segment("health")
.build()?;

let response = client.request(endpoint).response().await?;
```

Builder mutations consume finite structural budgets. A latched error prevents
later formatting or iterator consumption.

Runtime-defined endpoints use the same client base, planning, hooks,
credentials, limiter, transport, response modes, errors, and completion as
generated endpoint leaves.

Runtime-defined endpoints do not provide generated pagination.

## Streaming

`Stream` selects media-unconstrained byte streaming.
`Stream<M: MediaType>` selects exact media validation.
`Records<D>` selects typed record streaming.

Opening a stream completes the request operation. Later polling is lazy,
bounded, redacted, and consumer-owned.

## Buffered records

`BufferedRecords<D>`, `Csv<T>`, and `NdJson<T>` select complete record
collection. They are page-capable response specifications.

`Records<Csv<T>>` and `Records<NdJson<T>>` select streaming delivery.

## Multipart

The `multipart` feature exposes a request-only multipart specification. A
multipart body does not imply multipart response parsing.

## Explicit response bytes

`.response_bytes()` uses the ordinary request path and declared status
acceptance. It skips declared response conversion and returns complete bounded
bytes with accepted metadata.

Streaming response specifications do not support this terminal.

## Related

- [Generated client](generated_client.md)
- [Bodies and responses](bodies_and_responses.md)
- [Runtime model](runtime.md)
