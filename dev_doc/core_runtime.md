# Core Runtime

The runtime has one production executor: the managed Reqwest client. Redirects
are disabled, cookies are unsupported, and raw Reqwest clients/builders are
not exposed.

For each visible execution core performs collision validation, credential
preparation, rate-limit acquisition, sanitized pre-send observation, final
native request materialization, one `reqwest::Client::execute` call, response
observation, bounded body handling, and terminal decode. A `401` or `403` may
trigger one generation-safe authentication recovery when the logical body can
be rebuilt. No other Concord resend loop exists.

With the explicit non-default `dangerous-dev-tools` feature, the final managed
native boundary may instead consume one script from a deterministic executor.
The switch is stored only on the private managed clients; it is not a client,
endpoint, request, or generated-code generic. Application and provider clients
have independent handles. Without the feature, the branch and storage do not
compile and execution always reaches `reqwest::Client::execute`.

Scripted successes are converted from `http::Response<reqwest::Body>` into a
native `reqwest::Response`. No alternate response model exists: response
observation, classification, release ordering, `BoundedResponseStream`,
buffering, streaming, and codec decode are unchanged. The only synthetic
execution errors are the focused timeout, connect, request, and body categories
needed when public Reqwest APIs cannot construct an equivalent failure.

`RetryMode` is fixed at client construction. Reqwest owns hidden protocol or
status resends, so those sends do not rerun hooks, rate limiting, credential
preparation, or body factories. Use `RetryMode::Disabled` when exact
visible-to-wire accounting is required.

Reusable bytes become cloneable native Reqwest bodies. Streams and direct
multipart remain native and are not Reqwest-cloneable. Request and exact-length
limits are enforced during native materialization. Buffered responses use one
bounded collector; streaming responses retain native lazy delivery.

The `request_error` hook receives only sanitized metadata and a stable public
error category. It never receives Reqwest errors or transport implementation
objects.
