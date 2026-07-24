# Production configuration

Treat the generated builder as the one composition root:

```rust
let client = ServiceApi::builder()
    .tenant(settings.tenant)
    .token(concord::SecretString::new(settings.token))
    .configure_http(|http| {
        http.connect_timeout(settings.connect_timeout)
    })
    .configure_runtime(|runtime| {
        runtime
            .runtime_endpoint_timeout(settings.runtime_endpoint_timeout)
            .request_body_limit(settings.request_body_limit)
            .response_body_limit(settings.response_body_limit)
            .hooks(settings.hooks)
    })
    .build()?;
```

Choose finite limits from workload evidence. Exact defaults and types are in
[Limits](../reference/limits.md). A setter requiring `NonZeroUsize` or
`NonZeroU32` cannot create an unbounded zero sentinel. Disabling terminal
status capture or the default inherited by runtime endpoints is an explicit
operation, not an infinite limit. Generated endpoints retain their resolved
declaration timeout.

Keep one built client per stable configuration and clone it. The clone shares
the immutable transport/runtime snapshot. Rebuild only when configuration
changes intentionally.

Use lifecycle hooks for safe metrics and tracing, not request mutation or
credential inspection. Attach endpoint ID, operation ID, attempt, safe status,
and elapsed time. Keep application labels finite and do not reflect arbitrary
remote values into metric cardinality.

Generated rate plans use the façade governor under ordinary defaults. Supply a
custom limiter explicitly when process-local enforcement is insufficient; it
takes precedence and must bound its own state. Concord does not claim
distributed strict quota enforcement.

The importable
[custom limiter example](../../crates/concord-examples/src/rate_limiting/custom_limiter.rs)
and
[authentication example](../../crates/concord-examples/src/authentication/usage.rs)
are exercised by
[deterministic tests](../../crates/concord-examples/tests/local_runtime.rs).
