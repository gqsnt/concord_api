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
`concord::rate_limit`, or `concord::advanced`. Ordinary generated requests do
not require an extension trait import.

See the [Generated API reference](../reference/generated-api.md) and
[Features](../reference/features.md).
