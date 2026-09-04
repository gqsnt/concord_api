# Effects, retry, time, cancellation, and server delay

## Endpoint effect class

```concord-pseudocode
effect safe;
effect idempotent;
effect non_idempotent;
```

Defaults:

| Method | Default effect |
|---|---|
| GET, HEAD, OPTIONS | `safe` |
| PUT, DELETE | `idempotent` |
| POST, PATCH, permitted custom methods | `non_idempotent` |

- `safe` means the requested semantics are read-like/no requested state-changing effect;
- `idempotent` may mutate but repeating has the same intended effect;
- `non_idempotent` may multiply the requested effect.

An idempotency header does not infer the class. The endpoint author owns the provider contract. Generated docs show inferred versus explicit origin.

## One finite operation deadline

Every client has one positive root timeout. Scope/endpoint/request may select another finite value. No generated operation can disable it.

One absolute deadline begins when the terminal is first polled and covers:

- public shaping and validation;
- body metadata/preparation;
- credential acquisition/recovery;
- rate waits;
- attempts and transport;
- response status/acquisition/conversion;
- retry backoff;
- every pagination page/controller call;
- all intermediate observer calls.

It never restarts for an attempt, auth recovery, or page. Final completion
observation is post-fixation: it normally consumes caller time inside the
budget, but synchronous Rust is not preemptible, so a slow final callback may
return after nominal expiry without changing the result. Lazy streams own
separate post-return limits.

## Dispatch knowledge

```rust
pub enum DispatchKnowledge {
    NotDispatched,
    PossiblyDispatched,
    ResponseObserved,
}
```

It moves monotonically and describes client knowledge, not remote business commit.

## Application-owned retry policy

Representative builder:

```rust
let retry = RetryPolicy::builder()
    .max_retries_per_operation(2)
    .backoff(RetryBackoff::exponential(
        Duration::from_millis(100),
        Duration::from_secs(2),
        RetryJitter::Full,
    )?)
    .max_extra_load(RetryLoadRatio::percent(20)?)
    .honor_server_delay_up_to(Duration::from_secs(30))
    .classify_fn(classify)
    .build()?;
```

Retry is disabled unless installed. Policy owns appetite/backoff/load willingness; endpoint/body/core own semantic gates.

## Retry admission

For a failure conclusively before dispatch, endpoint effect is not required. After possible dispatch or observed rejected response:

```text
classifier recommends another policy attempt
+ failure category is eligible
+ body can reconstruct another visible-attempt body
+ (NotDispatched OR effect is safe/idempotent)
+ direct/page retry count remains
+ aggregate send ceiling remains
+ shared retry-load budget admits
+ selected delay fits remaining deadline
→ one new visible attempt
```

Accepted response representation validation/decode failure is terminal by default.

## Backoff and budget order

1. classify failure;
2. apply semantic/mechanical/count gates;
3. compute policy backoff plus jitter;
4. combine with applicable server minimum;
5. reject retry if the minimum exceeds willingness/deadline;
6. sleep under the absolute deadline;
7. acquire shared retry budget immediately before the new visible attempt;
8. start the attempt.

Cancellation during backoff consumes no shared retry capacity.

## Server delay

Parse `Retry-After` and supported vendor rate-reset data once at response head into a bounded core fact such as:

```rust
ServerDelayHint {
    not_before: Instant,
    source: RetryAfter | VendorRateReset,
    scope: CurrentOperation | Origin | Buckets,
}
```

- use one injected/core clock;
- delta seconds and valid HTTP date are supported;
- malformed/past hints are ignored by one documented rule;
- retry waits `max(backoff, applicable not-before)`, not the sum;
- limiter records cooldown for future admission;
- the next admission waits only the remaining interval;
- auth recovery uses no policy backoff but respects cooldown/deadline;
- if the minimum cannot fit, deny retry rather than retry early.

## Transport-private recovery

A low-level physical retry remains inside one visible attempt only when the protocol proves the server did not process the first try and the native body can be replayed safely.

It:

- uses a small finite physical-try ceiling;
- stays under the same deadline;
- does not re-run representation prepare, provider, rate admission, auth placement, public attempt events, or application retry budget;
- is disabled for one-shot/ambiguous native-body cases;
- appears as bounded physical-try evidence inside the visible attempt.

## Authentication recovery

Auth recovery is not policy retry. It has one attributed challenge, source-specific refresh/invalidation, at most one replay per direct/page operation, body replay gate, new rate admission, and no policy backoff/counter.

## Pagination

Each page may use its per-page visible retry ceiling. The aggregate shares one deadline, retry load budget, and hard send ceiling. Ordinary pagination requires `effect safe`.

## Cancellation

Dropping a future stops local polling and propagates cancellation. It cannot retract an already dispatched request. Error/event dispatch knowledge remains truthful.

## Evidence

Tests must cover:

- pre-dispatch retry of non-idempotent operation;
- ambiguous-dispatch denial for non-idempotent operation;
- idempotent/safe replay with replayable body;
- one-shot denial;
- accepted decode no-retry;
- backoff/jitter intervals and deterministic test randomness;
- valid/malformed/expired/over-cap server hints;
- cancellation during backoff;
- auth/policy/transport-recovery separation;
- one aggregate deadline through pages.
