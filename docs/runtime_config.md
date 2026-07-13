# Runtime Configuration

`RuntimeConfig` owns request-execution limits and observers that may change
after a client is constructed. General retry is deliberately absent: retry
mode is a Reqwest client property selected once with `RetryMode` during managed
client construction.

Runtime configuration includes:

- debug level and debug sink;
- runtime hooks;
- rate limiter and response observer;
- pagination loop detection and limits;
- request, response, streaming, and auth-internal body limits;
- `max_rate_limit_cooldown`;
- dangerous local development capture when its feature is explicitly enabled.

It does not include `retry_policy`, `max_attempts`,
endpoint retry policy, retry budgets, resend numbering, or
per-endpoint retry configuration.

## Retry construction

The default generated constructor selects `RetryMode::ProtocolRecovery` and
installs no custom Reqwest policy. Use a retry-aware constructor for the other
modes:

```rust,ignore
let disabled = MyApi::new_with_retry_mode(RetryMode::Disabled)?;

let status = MyApi::new_with_retry_mode(RetryMode::Status(
    StatusRetryConfig::new(1, [http::StatusCode::SERVICE_UNAVAILABLE])?,
))?;
```

`RetryMode::Status` validation, origin eligibility, and managed Reqwest client
construction finish before protected request execution and before credential
provider or body-factory side effects. The selected mode cannot be overridden
on an endpoint or pending request.

## Per-request overrides

Pending requests may override request options such as debug level and timeout.
Pagination supplies a private page index. There is no public attempt index,
retry count, response-body-limit override, hook override, rate-limiter
override, retry-mode override, or auth-recovery-count override.

## Hooks and visible executions

Hooks receive sanitized, body-free metadata. `pre_send` runs after rate-limit
acquisition and before raw authentication materialization. `post_response`
observes the final HTTP response returned by Reqwest, before endpoint body read
and decode. `request_error` observes a terminal visible-execution request
failure through a sanitized category, without exposing Reqwest errors.

These callbacks run once per visible call to `reqwest::Client::execute`:
initial execution, one authentication recovery, and each pagination page.
Reqwest-internal protocol or status retries do not rerun hooks, rate-limit
acquisition, or credential preparation.

## Retry-After cooldown

A final `429` may install a future-call cooldown through the rate limiter. A
valid positive `Retry-After` delta or HTTP date is capped by
`max_rate_limit_cooldown`. Past dates produce no positive delay and malformed
values are ignored. The current call is never automatically resent.

The governor also bounds the number of stored cooldown entries. Expired
entries are pruned before that capacity check. Advanced callers may install a
governor with a different fixed entry cap:

```rust,ignore
client.configure_mut(|config| {
    config.rate_limiter(std::sync::Arc::new(
        GovernorRateLimiter::new().with_max_cooldown_entries(1024),
    ));
});
```

## Body limits

Request limits are enforced before excess bytes reach Reqwest. Exact-length
guards remain structural. Buffered responses are collected under a bound;
streaming responses stay lazy and surface terminal body errors. Response
limits apply after decompression. Auth-internal HTTP uses its own response
limit.

Reqwest hidden retries clone only materialized bodies that Reqwest itself can
clone. Concord body factories do not run for hidden resends. Authentication
recovery reconstructs from the logical body recipe and runs a fresh factory
where applicable.

## Dangerous local capture

Development body capture is separate from errors, debug sinks, hooks, and
rate-limit metadata. It is feature-gated, disabled by default, and unsuitable
for production or shared artifacts. It never changes retry authority or body
limits.
