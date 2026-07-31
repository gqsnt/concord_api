# Execution policy

- **Audience:** API callers, runtime configurators, and extension authors
- **Purpose:** explanation and operational guide
- **Normative reference:** [runtime model](runtime.md)

## One operation

Polling a request terminal starts one operation with:

- one operation identity;
- one resolved deadline selection;
- one final result;
- one completion event.

Retries and authentication recovery are visible attempts inside that operation.
Pagination creates child page operations plus one aggregate operation.

## One managed executor

Generated direct requests, explicit endpoints, runtime-defined endpoints,
endpoint-backed credential acquisition, retries, recovery attempts, pagination
pages, and response-byte terminals all use the same executor.

This is the internal cost of promising that every public request surface has the
same hooks, limiting, credential boundary, deadline, body replay, response
handling, cancellation, errors, and completion.

## Exact stage order

```text
1  resolve and validate public method, logical URL, headers, identity, timeout, budgets
2  inspect body metadata
3  run before-send hook
4  prepare or reconstruct body
5  resolve current credential material without placing it
6  acquire every rate policy
7  place private authentication and materialize native request
8  perform HTTP under the operation deadline
9  present sanitized response head to limiter observer
10 present sanitized response head to user response hook
11 apply authentication recovery or admitted retry decision
12 apply declared status acceptance
13 acquire buffered, streaming, or discard response data
14 dispatch one completion event after the result is fixed
15 deliver the result
```

## Why ordering matters

| Ordering | Guarantee |
|---|---|
| hook before body preparation | a veto avoids expensive or sensitive body work |
| limiter admission before private placement | limiter and waiting logic never require credentials |
| limiter observer before user response hook | quota state is updated before application observation |
| challenge/retry before status acceptance | a declared accepted status cannot bypass credential recovery |
| status acceptance before body success | rejected bodies are handled under bounded error rules |
| completion after fixed outcome | observation cannot alter success or failure |

## Deadline

The operation timeout covers preparation, hooks, credential acquisition,
limiter waiting, transport, response-head observation, and complete buffered
acquisition and conversion.

Completion runs outside the operation timeout because it observes an already
fixed result. It is awaited and not detached. An accepted streaming response
completes when the stream opens; later polling is consumer-owned and follows
stream limits rather than the completed operation deadline.

## Cancellation

Dropping a request future cancels remaining foreground work. Concord does not
detach hooks, completion, or correctness work. Already completed external
effects are not rolled back. Application extensions can spawn their own work,
but that work is outside Concord’s guarantee.

## Hooks

<!-- concord-source: crates/concord-examples/src/runtime_policy/hooks.rs#hooks_client -->
```rust
pub fn construct() -> Result<ObservedApi, concord::ClientBuildError> {
    ObservedApi::builder()
        .configure_runtime(|runtime| {
            runtime
                .hooks(Arc::new(CountingHooks::default()))
                .debug(DebugLevel::Basic)
        })
        .build()
}
```

Before-send and response hooks may reject their current stage with safe errors.
Completion receives one fixed outcome. Hooks receive purpose-specific sanitized
structures, not body bytes, credential material, native objects, or native
error chains.

## Retry and recovery

Retry remains bounded by policy, extra-load budget, remaining deadline, body
replayability, and limiter admission. Authentication recovery remains separately
bounded and may replace still-current credential material. Both use the same
attempt pipeline.

## Related

- [Runtime model](runtime.md)
- [Retry](retry.md)
- [Authentication](auth.md)
- [Planning and observation](planning_and_observation.md)
