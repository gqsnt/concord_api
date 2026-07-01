# Public Errors

Concord runtime failures surface as `ApiClientError`. The enum is `non_exhaustive`, so application code should prefer variant matching for known cases and `ErrorCategory` for broader grouping.

`Display`, `Debug`, and `source()` diagnostics are safe metadata surfaces. They must not contain request body bytes, response body bytes, raw bearer tokens, query auth values, header auth values, Basic usernames or passwords, or secret values. Debug sinks, runtime hooks, rate-limit contexts, and retry contexts follow the same boundary. A custom transport or user-authored map or codec error can still log or return unsafe text if the integration writes it; Concord does not add body or auth material to those diagnostics itself.

## Taxonomy

| Failure class | Public error variant or kind | Happens before or after transport | Body read? | Retry? | Diagnostic safety notes |
| --- | --- | --- | --- | --- | --- |
| URL, host, path, or request parameter validation | `ApiClientError::InvalidParam`, `BuildUrl`, or `InvalidHostLabel`; category `Config` | Before rate-limit or transport when construction fails | No | No | Names the invalid field or label, not body or auth values. |
| Auth query or header collision | `ApiClientError::Auth`; category `AuthRejected` | After auth preparation, before rate-limit or transport | No | No | May name the colliding query or header key; raw auth value stays hidden. |
| Auth rejection (`401` or `403` on protected request) | `ApiClientError::Auth`; category `AuthRejected` | After transport metadata hooks and rate-limit observation | No | Auth refresh may happen under the configured policy and budget | Distinct from ordinary `HttpStatus`; response body is not read. |
| Rate-limit acquire or response-action failure | `ApiClientError::RateLimit`; category `RateLimit` with a structured `RateLimitErrorKind` | Before transport for acquire, after response metadata for response-action observation | No | No | Acquire/response contexts are request metadata only. |
| Transport send failure | `ApiClientError::Transport`; category `Transport` or `Timeout` | Transport attempted but no response metadata was classified | No response body | Retry only if retry policy covers the transport error | No rate-limit response observation for pure transport errors. |
| HTTP status failure | `ApiClientError::HttpStatus`; category `HttpStatus` | After transport response classification and hooks or rate-limit observation | No endpoint body read in the status-error path | Retry if policy covers the status | Headers are redacted in `Debug`. Protected `401` and `403` responses are handled as auth rejection first. |
| Retry exhaustion | No wrapper in v1; the final transport or status error is returned | After bounded retry attempts | Depends on final error class | Budgeted and bounded | Count attempts via policy or transport, not a retry-exhausted variant. |
| Content-Length body limit | `ApiClientError::ResponseTooLarge`; category `Decode` | After transport metadata classification, before body chunks are read | No | No ordinary retry | Reports limit and content length only. |
| Streaming body limit | `ApiClientError::ResponseBodyLimitExceeded`; category `Decode` | During bounded body read | Reads only enough chunks to detect overflow | No ordinary retry | Does not include partial body bytes. |
| Decode failure under limit | `ApiClientError::Decode` or `Codec`; category `Decode` | After bounded body read | Yes, within limit | No ordinary retry | Context may include status and content type, not payload bytes. |
| Map or transform failure | `ApiClientError::Transform`; category `Decode` | After decode | Already read within limit | No ordinary retry | Concord does not add body bytes; integration-authored source errors must avoid unsafe text. |
| Pagination non-progress or cap failure | `ApiClientError::Pagination` or `PaginationLimit`; category `Pagination` | Depends on page stage; non-progress happens after a page completes | Depends on completed page | Page retry or auth refresh keeps page identity | Error is page and control metadata only. |
| Runtime config invalid values | Most v1 runtime config setters are infallible; invalid runtime state uses `RuntimeState` or typed subsystem errors | Where the configured subsystem is used | Depends on subsystem | Depends on subsystem | Diagnostics remain body-free and auth-free. |

## `execute_raw()`

`execute_raw()` skips endpoint decode and mapping. It still performs logical request construction, auth collision validation, rate-limit acquire and observation, transport send, retry, response classification, auth rejection handling, and runtime response-body limits.

Consequences:

- it can return validation, auth, rate-limit, transport, HTTP status, retry final-error, and body-limit errors;
- it does not produce endpoint decode, map or transform, or pagination collection errors;
- diagnostics follow the same body-free and raw-auth-free rules as decoded execution.

## Testing Guidance

Tests should match `ApiClientError` variants or `ErrorCategory` instead of depending on prose. String checks are appropriate for proving that a sentinel is absent from `Display`, `Debug`, `source()` chains, debug events, hook events, rate-limit events, and retry events.
