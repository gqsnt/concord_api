# Managed executor

Core owns one executor for generated endpoints and
`RuntimeEndpoint`. Endpoint types describe public-head, body, and response
semantics; they do not send HTTP. This keeps authentication, limiter policy,
hooks, deadlines, redaction, retries, and response acquisition in one ordered
state machine.

## Lifecycle sequence

```text
inert Request<E>
    |
    | first poll
    v
operation + deadline
    -> public head and body metadata
    -> before-send approval
    -> body preparation
    -> authentication preflight
    -> limiter authority
    -> authentication finalization and private material
    -> native send
    -> limiter response observation
    -> response-head hook
    -> challenge handling and declared status acceptance
    -> accepted response acquisition, managed retry, or rejected-status capture
    -> fixed result and completion
```

Every ordinary stage from the first poll through result fixation shares the
selected operation deadline. Completion observes an already-fixed result
outside that deadline.

## Managed boundary ownership

Generated and runtime endpoints implement the same semantic endpoint
contract. Core owns:

- allocation of operation and parent/page identity;
- the final public URL/header component and byte budgets;
- timeout resolution and deadline checks;
- body metadata validation and managed request streams;
- credential-provider HTTP and private material attachment;
- rate-limit acquisition and response observation;
- sanitized hook/debug inputs;
- native application transport;
- rejected-status capture and accepted response modes;
- error reduction and completion outcome.

Extensions can supply finite decisions or transformations at named points.
They cannot take over the request, obtain the native response, add a second
completion path, or continue background draining after a terminal failure.

## Stage 0: inert construction

**Input and output:** client/scope/endpoint snapshots produce an owned
`Request<E>` or named wrapper.

**Errors:** none from core execution. Runtime endpoint construction and client
construction may already have failed before this stage.

**Deadline and cancellation:** no deadline exists. Dropping the request is
ordinary value disposal.

**Protected material:** credential slots may be shared by the client, but no
credential is acquired or copied into a request.

**Side effects:** no operation allocation, hook, body work, policy call, or
transport.

This is why delaying the first poll consumes no timeout budget.

## Stage 1: operation and public preparation

**Input and output:** the pending request is split into its shared client core,
endpoint snapshot, and request-local options. Core allocates an operation ID,
resolves timeout/debug selections, fixes the deadline, asks the endpoint to
prepare its public head, obtains body metadata, and validates the combined
contract.

Validation covers endpoint identity/method consistency, public head budgets,
media headers, rate-limit key preparation bounds, authentication placement,
body metadata, and transport URL preflight. The output is a prepared operation
or a fixed preparation failure.

**Errors:** configuration, timeout, request-body metadata, response-contract,
rate-key, URL/preflight, or internal identity failures. No native request has
been sent.

**Deadline and cancellation:** the deadline begins at the first poll and is
checked around preparation. Synchronous application metadata work cannot be
preempted mid-call, so extension contracts require finite trusted work and
core checks the deadline after it returns.

**Protected material:** authentication structure may be present, but private
credential values are neither acquired nor attached. The safe URL is already
available for failure/completion observation.

**Side effects:** one operation ID is allocated; the endpoint and body
specification may perform their documented finite preparation work. No hook or
network I/O occurs.

**Cancellation:** dropping the future drops owned preparation state.
Completion is not guaranteed if cancellation prevents the executor from
reaching completion delivery.

## Stage 2: before-send approval

**Input and output:** `RuntimeHooks::before_send` receives operation identity,
safe URL, sanitized public headers, page index, `AttemptInfo`, and structural
body metadata. It returns approval or `HookError`.

**Errors:** a hook error becomes `ApiError` category `Hook`; its authored text
is not retained. A veto prevents encoding, provider work, limiter work, and
transport.

**Deadline and cancellation:** the hook future runs under the operation
deadline. Timeout becomes category `Timeout`. Cancellation drops the hook
future.

**Protected material:** the event contains no authentication material or body
bytes.

**Side effects:** only application hook work. This is the last veto point
before body preparation for the attempt.

The initial invocation carries `AttemptInfo { index: 0,
policy_retry_index: 0, authentication_recovery: false }`. Every admitted
policy retry and the one possible authentication recovery invoke this stage
again. The global `index` increases for either transition; only policy retry
increments `policy_retry_index`, while the authentication transition sets
`authentication_recovery`.

## Stage 3: request-body preparation

**Input and output:** the endpoint yields its body input to
`RequestBodySpec::prepare`. Core validates the produced body against the
earlier metadata and converts it into a bounded prepared representation with a
known-length and replayability contract.

Buffered codecs, body factories, streams, and multipart inputs converge on a
core-owned native body path. Streaming forms remain lazy until transport polls
them; the managed wrapper enforces cumulative bytes, exact-length claims,
empty-chunk progress, and cooperative yield.

**Errors:** producer/encoding failures reduce to `RequestEncoding` or
`RequestBody`. Known impossible lengths and multipart framing excess fail
before native execution.

**Deadline and cancellation:** core checks the deadline around synchronous
preparation. Later stream polling happens while native execution is bounded by
the same deadline. Cancellation drops the prepared body and any owned stream.

**Protected material:** body bytes may be sensitive. They do not enter safe
errors, debug formatting, or hook events.

**Side effects:** an approved codec or factory may run. A vetoed hook means
the encoder/factory is never called and a source stream is never polled.

The specification prepares once for the whole request or page. A policy retry
reuses buffered bytes or reconstructs a factory/multipart recipe only after
its retry `before_send`; it does not rerun the buffered encoder. One-shot
bodies suppress optional policy retry and preserve the current failure.

## Stage 4: authentication preflight

**Input and output:** the prepared public authentication plan resolves current
credential selections. Static slots can be read; custom providers can perform
bounded work through managed provider HTTP. The output is a prepared plan that
still has not modified the native application request.

**Errors:** provider and credential state failures become finite `AuthError`
kinds inside `ApiError`. Authored secret-bearing messages do not cross the
boundary.

**Deadline and cancellation:** provider futures and provider HTTP share the
operation deadline. Cancellation drops the in-flight future.

**Protected material:** providers may access credential state; public hooks,
limiter requests, and the native application request still do not contain
attached material.

**Side effects:** credential caching/refresh and provider HTTP can occur.
Provider HTTP uses its own bounded request/response contract rather than the
application executor recursively.

## Stage 5: limiter authority

**Input and output:** the selected `RateLimiter` receives endpoint/method,
sanitized URL, safe host, page index, `AttemptInfo`, and the bounded effective
plan. A granted permit authorizes the attempt.

**Errors:** limiter failures become finite `RateLimitError` kinds. A fallback
limiter rejects a nonempty runtime plan, so missing policy fails before send.

**Deadline and cancellation:** acquisition and waiting run under the same
deadline. Cancellation drops the acquisition future.

**Protected material:** authentication is not finalized or materialized until
after authority. Generated key dimensions come only from bounded declared
public values.

**Side effects:** a limiter can mutate bounded policy state or wait. It cannot
inspect a native request or consume a response body.

The ordering prevents unauthorized work from receiving credentials merely
because it reached request construction.

## Stage 6: finalize, materialize, and send

**Input and output:** core finalizes the prepared authentication plan, creates
the managed native body, builds a native request from the validated public
head, emits safe request-ready debug output, then materializes protected
authentication into the private request. The native application transport
returns a managed response head/body.

**Errors:** final authentication, body production, native request
construction, connect/transport, and timeout failures reduce to stable
`ApiError` categories.

**Deadline and cancellation:** auth finalization, native execution, and
request-body polling are all under the operation deadline. Dropping the future
drops owned request/response state; core starts no background drain.

**Protected material:** credentials enter only the private native request,
after limiter authority and after safe debug observation. They are never
copied back into public head state.

**Side effects:** provider work may finish, body sources may be polled, and
native HTTP may execute. Known-safe protocol recovery remains inside this
transport operation; it does not create additional public hook/completion
attempts or retry statuses. Concord-managed policy retry is outside this
private transport boundary and therefore creates another visible attempt.

## Stage 7: response policy and observation

**Input and output:** when a response head arrives, core fixes the current
status, emits sanitized structural debug output, runs limiter response
observation, and then invokes `RuntimeHooks::on_response_head`. Both receive
the `AttemptInfo` of the dispatch that produced the head.

**Errors:** limiter observation can produce `RateLimit`; the hook can produce
`Hook`; either can time out. Those failures stop challenge/status handling and
body acquisition.

**Deadline and cancellation:** limiter and hook futures run under the
operation deadline. Cancellation drops the managed response; its body remains
unpolled.

**Protected material:** both observers receive sanitized headers and safe URL
data. They do not receive response body bytes, raw authentication values, or a
native response.

**Side effects:** limiter state changes before the application hook runs. This
ordering makes policy observation authoritative and ensures its failure cannot
be hidden by later body work.

The response-head hook sees each received attempt, including a challenge that
will subsequently be recovered.

## Stage 8: challenge, acceptance, and retry authority

**Input and output:** after response observation, core checks whether an
applied credential owns the challenge status. At most one recovery attempt is
allowed, and only when the body is replayable and the challenge policy permits
it. The challenged response is dropped without polling; before-send runs
again, the body is reconstructed, the attributed credential recovers, and the
attempt sequence repeats.

If recovery is unavailable or fails, the challenge becomes an authentication
rejection. Otherwise core calls `ResponseBodySpec::accepts_status` and checks
the deadline immediately after the synchronous callback. The default accepts
2xx. An explicitly accepted non-2xx proceeds as an ordinary accepted response
and is never a retry candidate.

Eligible connect/transport failures and rejected statuses are offered to the
optional synchronous `RetryClassifier`. For a rejected status this happens
only after limiter observation, the response hook, challenge handling, and
declared acceptance. Core checks the deadline immediately after the
classifier. A `Retry` decision is then gated, in order, by the per-operation
maximum, managed-body replayability, and the origin-scoped shared budget.
Budget reservation fixes the retry as started. The rejected response is
dropped unpolled before retry work, then the retry runs `before_send`,
reconstruction, current credential preflight, limiter admission,
materialization, and native execution again.

Classifier stop, maximum denial, one-shot suppression, or budget denial
preserves the current transport error or performs ordinary bounded terminal
status capture. Once a budget reservation succeeds, a later hook, body,
authentication, limiter, deadline, or transport failure is the final ordinary
category; core does not synthesize a retry error.

**Errors:** authentication rejection/recovery, status acquisition,
response-body progress, and timeout.

**Deadline and cancellation:** recovery, status acceptance, classification,
retry attempts, and rejected-status capture consume the original deadline.
No transition creates a fresh timeout. Cancellation drops current
retry/recovery/response state.

**Protected material:** recovery can replace credential state, but errors and
completion retain only safe identity, category, status, and bounded explicit
status-body state.

**Side effects:** one attributed credential may refresh. Each admitted policy
retry conservatively consumes one budget reservation, reruns current
credential preflight without invalidating material merely because of
transport/503, and reacquires limiter authority. Authentication recovery is
globally limited to one across all of those attempts.

## Stage 9: accepted response acquisition

**Input and output:** core validates the selected `ResponseBodySpec` against a
borrowed semantic response head, applies its sealed acquisition mode, then
converts the result:

- `Buffered` completely acquires a post-decompression bounded body under the
  operation deadline.
- `Discard` drops the accepted body without polling it.
- `Streaming` validates and returns a bounded `ByteStream` without polling
  unknown chunks.

**Errors:** response-contract validation, bounded body acquisition,
decoding/conversion, and timeout. Native read errors reduce to safe response
errors.

**Deadline and cancellation:** buffered/discard acquisition and specification
conversion use the endpoint deadline. A returned `ByteStream` is deliberately
caller-driven after the endpoint operation is complete; later polling is not
governed by that deadline. It remains cumulatively byte/empty-chunk bounded,
and callers add their own consumption timeout when needed. Dropping it drops
the owned native body.

**Protected material:** a response specification can inspect actual accepted
headers and body through its finite semantic context. It cannot retain a
native response through the trait.

**Side effects:** codec conversion or stream creation. Record decoding happens
as the returned stream is consumed and cannot emit another endpoint completion
event.

## Stage 10: fixed result and completion

**Input and output:** core fixes elapsed time, final status, recovery flag, and
the structural outcome from the already-determined result. It also records the
number of attempts started and policy retries started. It emits built-in
completion debug output and awaits `RuntimeHooks::on_complete`, then delivers
the original result.

**Errors:** completion cannot replace the primary result and returns no
application-authored error. The result is already fixed before the hook begins.

**Deadline and cancellation:** completion runs outside the operation deadline
and can delay delivery. Its elapsed value does not grow while the hook waits.
Cancellation while completion is pending can prevent the hook from returning
and prevent result delivery.

**Protected material:** the event contains safe identity, safe URL, status,
elapsed time, page index, attempt/retry totals, authentication-recovery
summary, and a structural outcome only.

**Side effects:** debug output and application completion observation. A
successful stream reports `StreamOpened`, describing handoff rather than later
consumption.

## Pagination wraps, but does not fork, the executor

A paginated request owns an aggregate operation and controller loop. Each page
is prepared through the same endpoint contract and executed by the same stage
sequence with a parent operation ID and page index. Page bodies, deadlines,
hooks, authentication, limiter calls, and completion therefore do not gain a
special transport path.

The outer loop enforces page/item/identity bounds, applies one controller
transition at a time, and produces its own aggregate completion after the
result is fixed. The aggregate uses checked arithmetic to sum page
`attempts_started` and `policy_retries`; no intermediate attempt creates a
second page completion. Pagination details belong to the runtime contract page
added with the maintenance manual.

## Change hazards

- Moving credential materialization earlier can expose secrets to policy or
  debugging.
- Moving the response hook before limiter observation can make body work
  proceed after policy failure.
- Starting the deadline at request construction breaks inert composition.
- Letting completion alter the primary result makes observation part of
  execution success.
- Draining bodies after failure creates hidden work and cancellation
  ambiguity.
- Treating stream consumption as endpoint completion would either duplicate
  completion or require background ownership.
- Reserving retry authority after `before_send` permits unbudgeted application
  work; reserving before maximum/replay checks consumes authority for an
  impossible retry.
- Polling an intermediate rejected body before retry changes both work and
  confidentiality semantics.
- Adding a feature-specific send loop creates different ordering guarantees
  for the same endpoint.

## Verification

The primary deterministic proofs live in:

- `crates/concord-core/src/execute/tests/laziness.rs`
- `crates/concord-core/src/execute/tests/ordering.rs`
- `crates/concord-core/src/execute/tests/cancellation.rs`
- `crates/concord-core/src/execute/tests/completion.rs`
- `crates/concord-core/src/execute/tests/status.rs`
- `crates/concord-core/src/execute/tests/retry.rs`
- `crates/concord-core/src/execute/tests/streaming.rs`
- `crates/concord-core/src/execute/auth_tests/recovery.rs`
- `crates/concord-core/src/pagination/runtime_tests/sequence.rs`

Run:

```bash
cargo test -p concord-core --all-features --locked
cargo test -p concord --all-features --locked
cargo test -p concord-examples --all-features --locked
```

## Related

- [Architecture overview](../architecture.md)
- [Compile-time to runtime flow](compile-runtime-flow.md)
- [The managed lifecycle](../../concepts/managed-lifecycle.md)
- [Security and redaction](../../concepts/security-and-redaction.md)
- [Hooks and observability](../../features/hooks-and-observability.md)
