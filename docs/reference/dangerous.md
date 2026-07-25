# Dangerous capabilities

Dangerous capabilities are explicit, separately feature-gated, non-default,
and bounded.

## Bounded raw successful response

Feature: `dangerous-raw-response`.

Eligible named requests gain `.raw_response()`. It skips declared decoding but
still uses status classification, the sealed acquisition mode, configured
finite response limit, lifecycle hooks, rate limiting, authentication,
completion, and redaction. It does not return a native transport response or
an unbounded body.

Use it only when an application must inspect a complete bounded successful
payload outside the declared decoder. Prefer a normal response specification
for reusable behavior.

## Custom native retry policy

Feature: `dangerous-custom-retry`.

Configure the policy through the generated client builder:

```rust
let client = Api::builder()
    .configure_http(|http| http.dangerous_custom_retry(policy))
    .build()?;
```

The policy remains inside private managed transport. It does not create a
second executor and cannot bypass Concord status, authentication-recovery,
acquisition, or completion behavior. It is distinct from the one-additional
authentication recovery attempt. A native policy may perform multiple
physical wire sends inside one visible Concord attempt. Those sends share that
attempt's limiter admission, lifecycle-hook accounting, and exactly-once
completion event; Concord does not report each physical send as a separate
attempt.

Retrying a non-replayable body is rejected by runtime replayability control.
Keep policies finite and understand idempotency of the remote operation.
