# Public Errors

Concord runtime failures surface as `ApiClientError`. The enum is
`non_exhaustive`, so application code should prefer variant matching for known
cases and `ErrorCategory` for broader grouping.

`Display`, `Debug`, and `source()` diagnostics are safe metadata surfaces. They
must not contain request body bytes, response body bytes, raw bearer tokens,
query auth values, header auth values, Basic usernames/passwords, or secret
values. Debug sinks, runtime hooks, rate-limit contexts, retry contexts, and
cache metadata follow the same boundary. A custom transport or user-authored
map/codec error can still log or return unsafe text if the integration writes
it; Concord does not add body or auth material to those diagnostics itself.

## Taxonomy

| Failure class | Public error variant/kind | Happens before/after transport | Body read? | Retry? | Stale fallback? | Cache admission? | Diagnostic safety notes |
|---|---|---|---|---|---|---|---|
| URL, host, path, or request parameter validation | `ApiClientError::InvalidParam`, `BuildUrl`, or `InvalidHostLabel`; category `Config` | Before cache/rate-limit/transport when construction fails | No | No | No | No | Names the invalid field or label, not body/auth values. |
| Auth query/header collision | `ApiClientError::Auth`; category `AuthRejected` | After auth preparation, before cache/rate-limit/transport | No | No | No | No | May name the colliding query/header key; raw auth value stays hidden. |
| Auth rejection (`401`/`403` on protected request) | `ApiClientError::Auth`; category `AuthRejected` | After transport metadata hooks and rate-limit observation | No | Auth refresh may happen only under the PR70 policy and budget | No by default | No | Distinct from ordinary `HttpStatus`; response body is not read. |
| Rate-limit acquire failure | Current advanced API returns the error from `RateLimiter::acquire`, commonly `RuntimeState` or integration-provided `ApiClientError` | After cache lookup, before transport | No | No | No | No | Acquire contexts are request metadata only. |
| Transport send failure | `ApiClientError::Transport`; category `Transport` or `Timeout` | Transport attempted but no response metadata was classified | No response body | Retry only if retry policy covers the transport error | Stale fallback only after retry declines/exhausts when cache policy allows | No success admission | No rate-limit response observation for pure transport errors. |
| HTTP status failure | `ApiClientError::HttpStatus`; category `HttpStatus` | After transport response classification and hooks/rate-limit observation | No endpoint body read in the status-error path | Retry if policy covers the status | Stale fallback only after retry declines/exhausts when cache policy allows | No success admission | Headers are redacted in `Debug`. Auth `401`/`403` on protected requests are handled as auth rejection first. |
| Retry exhaustion | No wrapper in v1; the final transport/status error is returned | After bounded retry attempts | Depends on final error class | Budgeted and bounded | Considered after retry declines/exhausts for ordinary failures | No failed-response admission | Count attempts via policy/transport, not a retry-exhausted variant. |
| Content-Length body limit | `ApiClientError::ResponseTooLarge`; category `Decode` | After transport metadata classification, before body chunks are read | No | No ordinary retry | No stale fallback after body read stage | No | Reports limit and content length only. |
| Streaming/chunked body limit | `ApiClientError::ResponseBodyLimitExceeded`; category `Decode` | During bounded body read | Reads only enough chunks to detect overflow | No ordinary retry | No stale fallback after body read stage | No | Does not include partial body bytes. |
| Decode failure under limit | `ApiClientError::Decode` or `Codec`; category `Decode` | After bounded body read | Yes, within limit | No ordinary retry | No stale fallback | No | Context may include status and content type, not payload bytes. |
| Map/transform failure | `ApiClientError::Transform`; category `Decode` | After decode | Already read within limit | No ordinary retry | No stale fallback | No | Concord does not add body bytes; integration-authored source errors must avoid unsafe text. |
| Pagination non-progress/loop/cap failure | `ApiClientError::Pagination` or `PaginationLimit`; category `Pagination` | Depends on page stage; non-progress happens after a page completes | Depends on completed page | Page retry/auth refresh keeps page identity | Stale fallback follows normal per-page order before page decode | No failed-page admission | Error is page/control metadata only. |
| Cache lookup/admission behavior | Cache store hooks are not fallible in the v1 public trait signatures | Lookup runs after auth collision validation, before rate-limit | No | Not applicable | `after_error` may return a stale response for ordinary failures | Admission only after endpoint success | Implementations must not panic; current trait methods return `CacheBefore`, `CacheAfter`, or `Option<BuiltResponse>`, not `Result`. |
| Runtime config invalid values | Most v1 runtime config setters are infallible; semantic invalid runtime state uses `RuntimeState` or typed subsystem errors | Where the configured subsystem is used | Depends on subsystem | Depends on subsystem | Depends on subsystem | No unless endpoint success completes | Diagnostics must remain body/auth-free. |

## `execute_raw()`

`execute_raw()` bypasses endpoint cache lookup/store and endpoint decode/map.
It still performs logical request construction, auth collision validation,
rate-limit acquire/observation, transport send, retry, response classification,
auth rejection handling, and runtime response-body limits.

Consequences:

- it can return validation, auth, rate-limit, transport, HTTP status, retry
  final-error, and body-limit errors;
- it does not produce endpoint decode, map/transform, pagination collection, or
  cache admission errors;
- diagnostics follow the same body-free and raw-auth-free rules as decoded
  execution.

## Testing Guidance

Tests should match `ApiClientError` variants or `ErrorCategory` instead of
depending on prose. String checks are appropriate for proving that a sentinel is
absent from `Display`, `Debug`, `source()` chains, debug events, hook events,
rate-limit events, retry events, and cache events.
