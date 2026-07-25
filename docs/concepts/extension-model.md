# Extension model

Concord extensions implement public semantic contracts. They do not receive
native request/response objects or privileged access to the runtime.

Extension families are:

- request body specifications and deferred body factories;
- buffered codecs and sealed response specifications;
- bounded record decoders;
- credential providers using isolated provider HTTP;
- pagination controllers that transform typed page values;
- rate limiters and observers using safe plans, keys, and response heads;
- lifecycle hooks using redacted events.

Choose the narrowest body contract that owns the required behavior:

| Direction | Ordinary buffered codec | Full lifecycle specification |
|---|---|---|
| Request | `EncodeBody` | `RequestBodySpec` |
| Response | `DecodeBody` | `ResponseBodySpec` |

Implement `EncodeBody` or `DecodeBody` for a conventional complete buffered
codec. Concord's blanket implementations then supply `RequestBodySpec` or
`ResponseBodySpec`. Implement the full specification directly only when the
extension must provide structural request metadata and managed preparation, or
must select and convert one of the sealed buffered, streaming, or discard
response modes.

There is no public trait named `ResponseBody`. `ResponseMode::Body` is the
core-acquired semantic input passed to `ResponseBodySpec::from_body`; it is not
a native response and cannot introduce another acquisition mode.

Primitive bytes, text, media types, streams, multipart control, response
acquisition, records execution, pagination execution, authentication
placement, limiter contracts, transport, errors, and completion are runtime
kernel concerns. JSON, CSV, NDJSON, built-in pagination policies, the default
process-local governor, its observer, and the OAuth2 client-credentials helper
are first-party downstream implementations.

That split is a proof: the first-party implementations compile entirely
through public contracts available to any downstream crate. The declaration
compiler treats all Rust specification/provider/controller types as opaque. It
does not detect concrete helper names or select implementations.

Applications should import these contracts through `concord::body`,
`concord::response`, `concord::auth`, `concord::pagination`,
`concord::rate_limit`, or another semantic façade module. Ordinary generated
requests do not require an extension trait import.

See the [Generated API reference](../reference/generated-api.md) and
[Features](../reference/features.md).
