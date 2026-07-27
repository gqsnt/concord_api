# Error reference

`concord::ApiError` is the safe structured failure returned by managed
endpoint execution. Match its non-exhaustive `ErrorCategory` and focused kind
accessors. `Display` and `Debug` are for people; they are not stable parsing
formats.

## Top-level categories

| `ErrorCategory` | Meaning |
|---|---|
| `Configuration` | Client, endpoint, public shaping, or request configuration was invalid |
| `RequestEncoding` | A buffered request codec failed |
| `RequestBody` | A managed request body could not be prepared, reconstructed, or transmitted |
| `AuthenticationUnavailable` | Required credential material could not be supplied |
| `AuthenticationProvider` | Acquisition, refresh, provider HTTP, or material construction failed |
| `AuthenticationRejected` | The protected service finally rejected attributed material |
| `RateLimit` | Plan preparation, admission, state capacity, or response observation failed |
| `Hook` | A result-determining application hook rejected the operation |
| `Timeout` | The selected operation deadline expired |
| `Connect` | Native connection establishment failed |
| `Transport` | Another native transport operation failed |
| `HttpStatus` | A rejected status was received |
| `ResponseBody` | An accepted response body could not be acquired or later streamed |
| `ResponseDecoding` | An accepted bounded response could not be decoded |
| `ResponseContract` | Accepted response metadata or specification output violated the declared contract |
| `Pagination` | Page limits, controller work, identity, or aggregate item limits failed |
| `Internal` | Concord reduced an invariant failure without exposing internal state |

The enum is `#[non_exhaustive]`. Match with a fallback so an application
continues to compile when a future version adds a category:

```rust,ignore
match error.category() {
    concord::error::ErrorCategory::Timeout
    | concord::error::ErrorCategory::Connect => retry_later(error),
    concord::error::ErrorCategory::AuthenticationRejected => rotate_credentials(error),
    _ => propagate(error),
}
```

Application policy still decides whether an operation is safe to repeat.
Categories do not assert idempotency.

## Stable context and focused kinds

Every `ApiError` exposes:

| Accessor | Value |
|---|---|
| `endpoint()` / `endpoint_id()` | stable generated or runtime endpoint identity |
| `method()` | application HTTP method |
| `category()` | top-level `ErrorCategory` |
| `configuration_kind()` | optional `ConfigurationErrorKind` |
| `auth_kind()` | optional `AuthErrorKind` |
| `rate_limit_kind()` | optional `RateLimitErrorKind` |
| `pagination_kind()` | optional `PaginationErrorKind` |
| `http_status()` | final status only when a terminal response wrapper is retained |
| `http_headers()` | sanitized final headers only with that wrapper |
| `http_response()` | borrowed `HttpResponseFailure`, when present |
| `into_http_response()` | owning extraction or the unchanged non-response error |
| `retry_after()` | parsed, non-negative, capped terminal `Retry-After` metadata |

Focused kinds are also non-exhaustive. Their canonical item paths are:

- `concord::error::ClientBuildErrorKind` and `ClientInputKind`;
- `concord::error::ConfigurationErrorKind`;
- `concord::error::AuthErrorKind`;
- `concord::error::RateLimitErrorKind`;
- `concord::error::PaginationErrorKind`;
- `concord::error::BodyErrorKind`, `CodecErrorKind`,
  `ResponseBodyErrorKind`, and `RecordErrorKind`.

`ApiError::source()` retains only the category-safe authentication,
rate-limit, or pagination error when one exists. Codec, body, hook, response
specification, and native transport sources are deliberately dropped during
reduction.

## Reduction rules

Subsystem errors reduce predictably:

| Input failure | `ApiError` result |
|---|---|
| `AuthErrorKind::Unavailable` | `AuthenticationUnavailable` plus `auth_kind()` |
| `AuthErrorKind::Rejected` | `AuthenticationRejected` plus `auth_kind()` |
| every other `AuthErrorKind` | `AuthenticationProvider` plus `auth_kind()` |
| any `RateLimitError` | `RateLimit` plus `rate_limit_kind()` |
| any `PaginationError` | `Pagination` plus `pagination_kind()` |
| `ResponseBodyErrorKind::InvalidMediaType` or `InvalidOutput` | `ResponseContract` |
| `ResponseBodyErrorKind::Decode` | `ResponseDecoding` |
| `ResponseBodyErrorKind::Internal` | `Internal` |
| managed request-source or native request-body failure | `RequestBody` |
| native timeout | `Timeout` |
| native connect failure | `Connect` |
| other native error | `Transport` |
| `HookError` | `Hook`; hook-authored text is dropped |

An `EncodeBody` error becomes `RequestEncoding`; a full
`RequestBodySpec` producer/materialization error becomes `RequestBody`.
Record opening errors reduce through the response contract, while failures
after a `RecordStream` opens are typed `RecordError` stream items.
`ByteStream` body failures remain `ApiError` items, normally
`ResponseBody`.

Safe messages on `AuthError`, `RateLimitError`, and `PaginationError` are
explicit application trust boundaries and may be retained by `ApiError`.
They must be bounded by the extension and contain no credential, body, cursor,
key, header, URL, or backend diagnostic. `HookError` text is not retained by
the resulting `ApiError`.

## Rejected HTTP statuses

Rejected `HttpStatus` and final `AuthenticationRejected` errors retain an
`HttpResponseFailure`. It owns:

- endpoint and method;
- final status;
- sanitized response headers;
- parsed capped `Retry-After`;
- one opaque finite `StatusBody`.

The status body has an explicit `StatusBodyState`:

| State | Meaning |
|---|---|
| `Disabled` | capture was disabled |
| `Empty` | the complete body had zero bytes |
| `Complete` | complete bytes were retained within the configured cap |
| `OmittedKnownTooLarge` | known length exceeded the cap before polling |
| `OmittedLimitExceeded` | unknown-length collection crossed the cap and discarded its prefix |
| `UnavailableReadFailure` | body reading failed after the head |
| `UnavailableDeadline` | the deadline expired during capture |

Formatting the error, response wrapper, or status body never prints retained
bytes or header values. The application can nevertheless call
`StatusBody::bytes`, `into_bytes`, or
`HttpResponseFailure::decode_body<C>` for an explicitly retained `Complete`
body. Those bytes may be sensitive and are then application-owned.

Malformed `Retry-After` is ignored. Delta seconds and HTTP dates are parsed,
past dates become zero, and future durations are clamped to the configured
cap.

## Failure ordering

Before transport, the first failure in the managed order wins: public
preparation, before-send hook, body preparation, credential preflight,
limiter admission, private placement, then native send.

For a received final head, ordering is:

```text
limiter observation
→ response-head hook
→ authentication challenge
→ declared status acceptance
→ accepted response validation and acquisition, or rejected-status capture
```

A limiter observation error is `RateLimit`; an observation or hook timeout is
`Timeout`. The received status is still available to completion metadata, but
those errors do not acquire a status body or retain an
`HttpResponseFailure`.

Authentication recovery may run one additional visible attempt. A second
challenge becomes the final `AuthenticationRejected` response. Pagination
wraps page failures in the aggregate operation without exposing accumulated
items as partial success.

`ResponseBodySpec::accepts_status` defaults to 2xx. An explicitly accepted
non-2xx status returns through `Ok` and uses the declared buffered, streaming,
or discard acquisition limit and completion outcome. The deadline is checked
immediately after the synchronous acceptance callback.

## Construction and extension errors

`ClientBuildError` is separate from `ApiError` because no endpoint execution
exists yet. It reports one safe `ClientBuildErrorKind`:

| `ClientBuildErrorKind` | Meaning |
|---|---|
| `MissingInput` | a required generated input was absent |
| `InvalidCredential` | built-in credential material violated its finite contract |
| `InvalidBaseUrl` | the generated fixed base was malformed or structurally unsafe |
| `HttpsRequired` | HTTPS-only policy rejected a fixed HTTP base |
| `TlsUnavailable` | the fixed HTTPS base requires a TLS feature that is not compiled |
| `InvalidProxy` | the explicit proxy could not be installed by the private transport |
| `InvalidRootCertificate` | a root PEM exceeded its structural bounds or failed parsing |
| `InvalidClientIdentity` | an identity PEM exceeded its structural bound or failed parsing |
| `HttpClientBuild` | final private native client construction failed |
| `RuntimeConfiguration` | runtime configuration violated a construction invariant |
| `LimiterUnavailable` | generated rate limits require a limiter that is not installed |
| `Internal` | construction failed a safe internal invariant |

Missing generated inputs and invalid built-in credentials may also report the
public generated input name plus `ClientInputKind` (`Value`, `Secret`, or
`CredentialProvider`). No supplied value is retained. The other kinds are
value-free.

`ClientBuildError::source()` is always `None`. Its `Display` and `Debug` output
never retain proxy URL text, PEM bytes, parser diagnostics, certificate or
identity details, Reqwest errors, or native source chains. Branch on `kind()`
for remediation; do not parse formatting.

Extension-level `BodyError`, `CodecError`, `RecordError`, `AuthError`,
`RateLimitError`, and `PaginationError` are category-safe contracts at their
own boundary. Do not treat their source-less design as a reason to log the
discarded native or parser error separately without redaction review.

## Related

- [Requests, planning, and errors](../guide/requests-planning-errors.md)
- [Security and redaction](../concepts/security-and-redaction.md)
- [Finite behavior](../concepts/finite-behavior.md)
- [Hooks and observability](../features/hooks-and-observability.md)
- [Limits](limits.md)
- Rustdoc: `concord::error::ApiError` and
  `concord::error::ErrorCategory`
