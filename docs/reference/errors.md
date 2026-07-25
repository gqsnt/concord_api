# Error reference

`concord::ApiError` is the safe structured failure from endpoint execution.
Match its non-exhaustive `ErrorCategory` instead of formatting native causes.

Top-level categories are:

```text
Configuration
RequestEncoding
RequestBody
AuthenticationUnavailable
AuthenticationProvider
AuthenticationRejected
RateLimit
Hook
Timeout
Connect
Transport
HttpStatus
ResponseBody
ResponseDecoding
ResponseContract
Pagination
Internal
```

`ApiError` exposes the safe endpoint ID, application method, category, and
focused optional sub-kinds. A terminal HTTP response may expose status,
sanitized headers, bounded status-body state, and parsed `Retry-After`
metadata. It never exposes a native response, authenticated URL/header set,
credential material, provider payload, or unbounded body.

Focused stable non-exhaustive kinds include:

- `ClientBuildErrorKind` and `ClientInputKind`;
- `ConfigurationErrorKind`;
- `AuthErrorKind`;
- `RateLimitErrorKind`;
- `PaginationErrorKind`;
- `BodyErrorKind`, `CodecError`, `ResponseBodyErrorKind`, and
  `RecordErrorKind` where the capability applies.

Builder errors distinguish a missing public value, protected secret, or
credential provider without including its value. Provider and extension error
messages cross a safe-text contract. Application-approved safe messages are
trusted application-owned text, are not automatically covered by one universal
Concord size limit, and must not include protected inputs or other secrets.
Applications and extensions must bound their own retained message storage.

For a received response, error precedence is core limiter observation, then the
user response-head hook, then authentication challenge, status, and acquisition
handling. A limiter observation failure is `ErrorCategory::RateLimit` with
`RateLimitErrorKind::Observe`; an observation timeout is
`ErrorCategory::Timeout`. Both retain the final response status for completion
metadata without acquiring the response body.

Debug/display representations are redaction-aware but are not intended as
stable parsing formats. Treat the typed category/kind accessors as the
compatibility contract.
