# Managed retry contract

Managed retry is Concord-owned application policy. It is distinct from
Reqwest's private known-safe protocol recovery and from provider HTTP retry.
The policy decides whether an eligible application failure should request
another visible managed attempt; a shared rolling budget limits the aggregate
extra load.

## Purpose

A per-operation retry maximum prevents one request from looping forever but
does not prevent many failing requests from creating a retry storm. The retry
contract therefore combines:

- an explicit synchronous application classifier;
- a hard per-request or per-page maximum;
- body replayability and the original operation deadline;
- a bounded origin-scoped aggregate load budget;
- the ordinary hook, authentication, limiter, observation, and completion
  lifecycle for every admitted retry.

There is no managed policy by default. Private protocol recovery is enabled
by default, can be disabled with `protocol_recovery(false)`, and does not
enable status or application-failure retry.

## Runtime installation

The public semantic surface is `concord::retry`. A policy is installed on the
immutable client runtime:

```rust
let policy = concord::retry::RetryPolicy::builder()
    .classify_fn(|attempt| match attempt.failure() {
        concord::retry::RetryFailure::RejectedStatus { status, .. }
            if status == concord::http::StatusCode::SERVICE_UNAVAILABLE =>
        {
            concord::retry::RetryDecision::Retry
        }
        _ => concord::retry::RetryDecision::Stop,
    })
    .build()?;

let builder = builder.retry(policy);
```

The lower-level equivalent is
`configure_runtime(|runtime| runtime.retry_policy(policy))`.
`RuntimeConfigBuilder::disable_retry_policy` restores the no-policy runtime
selection. Replacing or disabling a policy affects only the client snapshot
being built. It does not add DSL structure and does not expose Reqwest, Tower,
or native retry types.

## Semantic model

`RetryPolicy::builder` configures:

| Setting | Default | Validation |
|---|---:|---|
| maximum extra load | `0.20` | finite, `0.0..=1000.0` |
| maximum policy retries per operation | `2` | `1..=16` |
| budget window | 10 seconds | fixed |
| budget bucket width | 1 second | fixed |
| retained origin scopes | 4,096 | positive |
| origin idle TTL | 15 minutes | positive |
| classifier | none | required |

`without_budget` removes shared budget admission. It does not remove the hard
maximum of 16 policy retries per operation.

The builder accepts either a `RetryClassifier` object or a closure through
`classify_fn`. Invalid application configuration produces a source-less
`RetryPolicyBuildError` and stable `RetryPolicyBuildErrorKind`; it does not
panic or retain a rejected value.

## Classifier boundary

The classifier receives one borrowed `RetryAttempt` after an eligible failure.
It can inspect:

- stable endpoint ID and application method;
- final logical host;
- pagination page index;
- the failed `AttemptInfo`;
- policy retries already performed;
- `RetryFailure::Connect`, `Transport`, or `RejectedStatus`.

Rejected status includes the status and `SanitizedHeaders`. The classifier
never receives the native request or URI, logical path/query, request headers,
authentication material, body bytes, Reqwest errors, provider errors, limiter
internals, or resolved rate-limit key values.

`RetryDecision::Retry` is only a request for another attempt. Replayability,
the per-operation maximum, budget authority, and deadline remain
authoritative. `Stop` preserves the current failure or rejected response.

The callback is synchronous trusted application work. Concord checks the
operation deadline immediately after it returns, does not catch a panic, and
cannot bound classifier allocation, blocking, retained state, logging, or
metric cardinality.

## Exact rolling budget

Budget state is partitioned by final credential-sanitized logical origin:

```text
scheme + normalized host/IP + effective port
```

Path, query, user information, and credentials are not retained. Explicit and
implicit default ports share a scope; different schemes or effective ports do
not. The private scope-key type implements equality and hashing but no
formatting surface.

Each scope retains exactly ten one-second buckets. Active buckets count:

- initial managed native dispatches;
- approved policy retry reservations.

Only the first visible attempt of one request or page contributes initial
load. Policy retries, authentication recovery, and private protocol recovery
do not.

For fixed-point ratio `r` and `n` active initial dispatches:

```text
allowed retries = ceil(n × r)
```

A reservation succeeds only while active approved retries are below that
limit. At the default ratio, one initial dispatch permits one low-volume
retry, five permit one, six permit two, and 100 permit 20. Ratio zero permits
none. Policy construction converts the floating-point configuration to a
fixed-point integer; admission uses checked integer arithmetic only.

Reservations occur after classification, per-operation maximum, and body
replayability approve another attempt, but before the next `before_send`.
They are not rolled back if a later hook, body reconstruction, provider,
limiter, materialization, or transport stage fails.

## State and sharing

The origin map has an explicit positive capacity and idle TTL. On new-origin
pressure, Concord:

1. removes idle scopes whose TTL has expired;
2. preserves every remaining active scope;
3. declines insertion if capacity is still full;
4. denies retry for the untracked origin;
5. preserves the original failure or status.

Capacity pressure and poisoned internal synchronization fail closed for retry
admission; they do not replace a useful remote failure with a new public
error.

`RetryPolicy::clone` shares the classifier and budget state. Client clones
therefore share budget when configured with the same policy. Independently
built policies are isolated, while explicitly cloning one policy into
independently built clients intentionally shares it. Origin partitioning still
separates targets.

## Managed lifecycle integration

The executor now admits policy retry inside the same request/page operation.
An admitted policy retry re-enters:

```text
before_send
→ body reconstruction
→ current authentication preflight
→ limiter admission
→ credential finalization and private materialization
→ native execution with private protocol recovery
→ limiter response observation
→ response hook
→ challenge, status, and retry handling
```

The request or page retains one operation ID, one deadline, one final result,
and one completion event. `AttemptInfo`, `ExecutionMeta`, page completion, and
pagination aggregate completion account for the additional visible work.
Authentication recovery remains globally limited to one and does not consume
the policy-retry count.

Accepted statuses are successful response candidates and are never offered to
the classifier. Timeout, hook, limiter, authentication, request-body,
accepted-response, pagination-controller, and internal failures are likewise
not classifier inputs.

Connect and transport failures are classified while the original `ApiError`
remains owned by the executor. A stop or suppression returns that same safe
structural result. Rejected responses are classified only after limiter
observation, response hook, challenge handling, and status acceptance. An
admitted response retry drops the intermediate body without polling it;
suppression instead uses the ordinary bounded terminal capture path.

After `Retry`, authority is checked in this exact order:

1. configured per-operation maximum;
2. non-consuming prepared-body replayability;
3. shared budget reservation.

The reservation occurs before retry `before_send` and is not rolled back.
That retry then increments the visible index and policy-retry index, while a
later authentication recovery increments only the visible index and sets its
authentication flag. The maximum visible total is one initial attempt, 16
policy retries, and one authentication recovery.

`RequestBodySpec::prepare` and a buffered encoder run once. Reusable bytes are
cloned into each native request; stream and multipart factories reconstruct
once per admitted visible attempt after its hook. A one-shot body causes
optional retry suppression before budget admission.

## Redaction and formatting

Policy and classifier formatting omits classifier-owned state. Retry-attempt
formatting omits host text and header values; rejected-status formatting
reports status and structural header count only. Builder errors contain only a
finite kind and fixed Concord-owned text.

`SanitizedHeaders` can still expose application data when explicitly
inspected. A classifier owns its logging, storage, and data-classification
policy. Safe automatic formatting is not permission to use hosts, operation
IDs, or header values as metric labels.

## Failure and change hazards

Maintain these failure-closed properties:

- missing or invalid policy configuration never panics;
- checked counter or ratio arithmetic never wraps;
- expired buckets cannot authorize from stale initial load;
- capacity denial cannot evict active state;
- budget denial preserves the original result;
- a retry reservation is conservative and never rolled back;
- disabling the shared budget cannot disable the hard operation maximum.

Moving initial accounting after native send can undercount concurrent load.
Reserving before classifier/replay checks wastes budget on impossible retries;
reserving after `before_send` lets an unbudgeted attempt perform application
work. Scoping by complete URL introduces attacker- or tenant-controlled
cardinality. Exposing native errors or complete headers changes the
confidentiality boundary.

## Verification

Primitive proofs belong in `crates/concord-core/src/retry/` and use Tokio
paused time for bucket rotation, window expiry, TTL, capacity, and origin
isolation. Concurrency tests must prove atomic reservation across policy
clones. Lifecycle integration tests must additionally prove attempt order,
deadline reuse, one completion, body replay suppression, rate-limit
re-admission, authentication interaction, and preservation of the original
failure on budget denial.

Executor integration proofs live in
`crates/concord-core/src/execute/tests/retry.rs`, authentication interleaving
proofs in `execute/auth_tests/recovery.rs`, and page/aggregate accounting in
`pagination/runtime_tests/sequence.rs`.

After focused tests, run all-feature core tests, workspace doctests, strict
documentation checks, warning-denied Rustdoc, and the Rust 1.85 semantic gate.

## Related

- [Managed executor](../architecture/managed-executor.md)
- [Authentication contract](authentication.md)
- [Rate-limit contract](rate-limits.md)
- [Errors, hooks, and completion](errors-hooks-completion.md)
- [Finite behavior](../../concepts/finite-behavior.md)
