# Dangerous raw responses and custom retry

> Audience: Advanced  
> Stability: Opt-in dangerous features

Dangerous features deliberately reduce an ordinary Concord guarantee. Enable them only when the application accepts the exact additional responsibility described here.

## Dangerous raw response

The feature adds `.raw_response()` to eligible generated requests.

```rust
use concord_core::dangerous::RawResponseRequest;

let request: RawResponseRequest<_> = client.record().raw_response();
let response = request.await?;
```

`RawResponse` exposes:

- logical URL;
- status;
- HTTP version;
- actual response headers;
- execution metadata;
- complete bounded successful body bytes.

### Preserved guarantees

Raw success still uses ordinary:

- request/body preparation;
- hooks;
- authentication;
- limiter acquisition and observation;
- private native HTTP execution;
- status classification;
- response body limit;
- deadline and cancellation;
- hidden retry containment;
- completion.

Non-success responses remain ordinary `HttpStatus` errors. Known oversize fails before body polling; first unknown excess returns no partial value. Native Reqwest response/client/request/URL does not escape.

### Reduced guarantee

The caller intentionally receives actual successful headers and complete body bytes. This is a reduced secrecy boundary. Store, log, and forward them carefully. `RawResponse::Debug` exposes body length only.

### Availability

Cargo features are unified for the compiled binary. Enabling this feature anywhere exposes the public raw terminal where generated capability rules allow it.

### Exclusions

Raw success is absent from streaming, paginated, prepared, feature-disabled, and incompatible endpoint surfaces. There is no raw non-success terminal or response-limit bypass.

## Dangerous custom retry

This feature exposes Reqwest's native retry builder through:

```rust
concord::dangerous::ReqwestRetryBuilder
```

Select the builder as explicit runtime policy:

```rust
let client = Api::builder()
    .configure_http(|http| http.dangerous_custom_retry(bounded_retry()))
    .build()?;
```

Custom transport retry is runtime policy and has no DSL form.

### Why it is dangerous

A native retry classifier may observe native materialized URLs, including query authentication. Application-owned diagnostics inside the classifier are outside Concord's redaction boundary.

### Lifecycle containment

Native retry remains inside one Concord attempt. It does not re-enter:

- before-send hooks;
- request body specification preparation;
- body factories;
- credential preflight/finalization;
- limiter acquisition;
- Concord completion.

The body must still be replayable under Reqwest's native requirements. Concord's one bounded authentication recovery remains separate.

### Recommendations

- Prefer `retry off` unless the remote operation is known to be safe.
- Keep retry count and classifications finite.
- Never log native URLs or headers.
- Do not retry non-idempotent operations without an application protocol.
- Test physical attempts and logical Concord attempts separately.

### Feature unification

The feature is binary-wide through Cargo feature unification and is not default.
