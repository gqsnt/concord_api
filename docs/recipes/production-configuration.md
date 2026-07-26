# Production configuration

Treat the generated builder as the one composition root:

```rust
let client = ServiceApi::builder()
    .set_tenant(settings.tenant)
    .token(concord::SecretString::try_new(settings.token)?)
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

An explicit proxy and custom root certificates configured through
`configure_http` apply to both application traffic and isolated credential
provider HTTP. A configured application client identity is installed only on
the application client; provider HTTP never receives that identity. The two
clients retain separate connection pools and lifecycle roles.

Use lifecycle hooks for safe metrics and tracing, not request mutation or
credential inspection. Attach endpoint ID, operation ID, attempt, safe status,
and elapsed time. Keep application labels finite and do not reflect arbitrary
remote values into metric cardinality.

Generated rate plans use `LocalRateLimiter` under ordinary defaults. It is a
continuously refilling token bucket local to one process and one limiter
object; it does not claim exact remote fixed/sliding-window or distributed
quota enforcement. Client clones share the built client's object.
Independently built clients can share by receiving the same
`Arc<dyn RateLimiter>`.

Choose `by` dimensions conservatively because their cardinality determines
local state demand. `StateCapacity` is a recoverable pre-transport signal that
a new key did not fit; existing buckets were not evicted. Increase a finite
capacity from workload evidence, reduce partition cardinality, wait for safely
prunable state, or supply a custom bounded limiter. A custom limiter takes
precedence and must bound its own state.

The importable
[custom limiter example](../../crates/concord-examples/src/rate_limiting/custom_limiter.rs),
[partitioning example](../../crates/concord-examples/src/rate_limiting/partitioning.rs),
and
[authentication example](../../crates/concord-examples/src/authentication/usage.rs)
are exercised by
[deterministic tests](../../crates/concord-examples/tests/local_runtime.rs).
