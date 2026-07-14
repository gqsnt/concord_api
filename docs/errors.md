# Errors

`Display`, `Debug`, and `source()` diagnostics are safe metadata surfaces. They
must not contain request/response bodies, credentials, proxy targets,
multipart values, or secret values. Debug sinks, runtime hooks, and rate-limit
contexts follow the same boundary. User-authored codec errors remain
responsible for their own text.

| Failure | Public shape | Body read | Reqwest/recovery handling |
| --- | --- | ---: | --- |
| Configuration/build | typed configuration or client-build error | no | none |
| HTTPS without managed TLS capability | `ApiClientError::TlsCapabilityUnavailable` (`Config`) | no | rejected before provider, limiter, hooks, body, or execution |
| Auth preparation | `ApiClientError::Auth` | no endpoint body | no visible execution yet |
| Rate-limit acquire/action | `ApiClientError::RateLimit` | no | no Concord resend |
| Timeout | `ApiClientError::Timeout` | no response body | final visible Reqwest result |
| Connect failure | `ApiClientError::Connect` | no response body | final visible Reqwest result |
| Request execution | `ApiClientError::RequestExecution` | no response body | final visible Reqwest result |
| Request body production | `ApiClientError::RequestBody` | no response body | terminal; preserves a structured `BodyErrorKind` |
| Request body limit | `ApiClientError::RequestBodyLimitExceeded { limit, actual }` | no response body | terminal request-body failure; the request-error hook observes `RequestBody` |
| HTTP status | `ApiClientError::HttpStatus` | no endpoint body in status path | final result after Reqwest-internal retry; `401`/`403` may cause one auth recovery |
| Response limit | `ResponseTooLarge` or `ResponseBodyLimitExceeded` | bounded | terminal |
| Decode/codec | `Decode` or `Codec` | bounded | terminal |
| Pagination | typed pagination error/limit | page-dependent | page state does not advance on failure |

Reqwest returns the final status or request result for one visible execution;
hidden Reqwest resends are not exposed as additional Concord response objects.
Concord then performs terminal processing or at most one authentication
recovery.

The TLS-capability failure is URL-free and has no source payload. It reports
only that HTTPS is unsupported by the managed client. Because it occurs before
the hook phase, `pre_send` and `request_error` are not invoked for this
preflight failure. Credential-provider HTTP applies the same private check to
its separately managed client and reports
`AuthErrorKind::TlsCapabilityUnavailable`.

A final `429` can include a sanitized `Retry-After` header and rate-limit
action. It remains the returned status error; any valid capped delay affects
future calls only. A configured status-mode `503` retry is hidden inside
Reqwest and immediate.

`#[cfg(feature = "dangerous-raw-response")]` exposes
`concord_core::dangerous::BuiltResponse` and `execute_raw_response()`. It skips
endpoint decode but retains construction, collision checks, authentication,
rate limiting, hooks, selected Reqwest retry mode, auth rejection handling,
  and response limits.

HTTP status errors expose only sanitized stored headers. Sensitive response
headers are redacted; safe metadata such as content type and Retry-After may
remain. Tests should match variants or `ErrorCategory` and use string checks
only to prove sentinel absence across error and observer surfaces.
