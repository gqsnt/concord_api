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
