# Execution policy

Concord separates domain contract from application/runtime policy while keeping
one execution authority.

## One managed pipeline

Every direct, unbound-bound, runtime-defined, credential-source, retry, recovery,
page, response, and accepted-byte path enters the same executor. The stable
authority order is public shaping, request gate, prerequisites/body work,
credential validation/resolution, rate admission, private placement, dispatch,
challenge handling, response gate/status, acquisition, conversion, fixed result,
then completion observation.

No extension receives a native request/response mutation handle. Transport
proxy and client identity remain HTTP configuration. Endpoint declarations do
not choose backoff, jitter, token buckets, or distributed coordination.

## Gates versus observers

`RequestGate` and `ResponseGate` may reject only at their named pre-outcome
phase. Their bounded lifecycle views contain IDs, method, observed URL/headers,
attempt/page facts, and structural body information—not bodies, credentials,
native state, or decoded output.

`CompletionObserver` runs after the result is fixed, returns no decision, and
cannot transform success/failure. Applications needing asynchronous export use
an explicitly bounded queue/sink with owned shutdown and backpressure policy;
core does not spawn unbounded detached work.

<!-- concord-source: crates/concord-examples/src/runtime_policy/hooks.rs#hooks_client -->
```rust
pub fn construct() -> Result<ObservedApi, concord::ClientBuildError> {
    let observation = Arc::new(CountingHooks::default());
    ObservedApi::builder()
        .configure_runtime(move |runtime| {
            runtime
                .request_gate(observation.clone())
                .response_gate(observation.clone())
                .completion_observer(observation)
                .debug(DebugLevel::Basic)
        })
        .build()
}
```

The example implementation happens to combine all three traits in one object;
their authorities remain separate.

## Deadline and cancellation

One terminal gets one aggregate absolute deadline, covering every wait,
callback, attempt, page, and conversion. Core checks before/after synchronous
trusted callbacks but cannot preempt them.

Dropping the future stops further local polling. It does not prove a dispatched
remote operation was canceled or rolled back. No hidden task continues request
execution unless an explicitly named runtime service owns that lifecycle.

See the normative [runtime model](runtime.md).
