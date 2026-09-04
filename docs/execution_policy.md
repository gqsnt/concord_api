# Execution policy

Every generated request, unbound-bound request, runtime endpoint, retry, auth recovery, accepted-byte terminal, stream acquisition, and pagination page uses one executor and one ordering model.

## Operation identities

```text
logical operation
  ├─ visible attempt 0
  │    └─ optional transport-private physical recovery
  ├─ policy backoff
  ├─ visible attempt 1
  └─ fixed outcome

pagination aggregate
  ├─ page operation 0 → visible attempts
  ├─ page operation 1 → visible attempts
  └─ aggregate outcome
```

Do not collapse these identities, counts, or budgets.

## Terminal start

When a terminal is first polled:

1. validate terminal-local input;
2. `PageLimit::Prefix(0)` returns empty before operation allocation or effects;
3. allocate operation/aggregate identity;
4. capture immutable client/request/runtime snapshot;
5. create one absolute deadline and finite counters.

Construction, controls, descriptions, and planning remain inert.

## Public preparation

For a direct request or each page:

1. resolve effective contract and representation defaults;
2. apply page-controlled endpoint values;
3. shape method, logical URL, public query, and public headers once;
4. derive real and sanitized observation atoms from the same encoding events;
5. enforce component/count/byte/reserved-owner bounds;
6. obtain body structural metadata without materialization.

No credentials, body bytes, rate admission, or native request exist yet.

## Body preparation

If a body is present:

1. call representation `prepare` exactly once for this direct/page request;
2. validate metadata/media/body limits;
3. retain managed prepared state;
4. check deadline after synchronous extension code.

Deterministic local preparation failure consumes no remote rate admission.

## Visible attempt loop

For each visible attempt candidate:

1. check deadline and local/aggregate attempt ceilings;
2. allocate attempt identity and emit read-only attempt-start evidence;
3. resolve current credential material/version under private state;
4. construct/reconstruct one managed attempt body from prepared state;
5. acquire the complete effective rate plan under remaining deadline;
6. place private auth and construct the native request in core;
7. cross the transport boundary and advance dispatch knowledge;
8. execute transport, including only protocol-proven physical recovery;
9. receive a response head or transport failure.

### Why this order

- public shaping is credential-free and inspectable;
- local body errors do not spend quota;
- known unavailable credentials do not spend provider quota;
- attempt body construction happens before admission when it can fail synchronously;
- private data is placed only after admission and immediately before dispatch;
- a body producer failure during transport is a real attempted request.

If a body family demonstrably holds excessive attempt resources while waiting for rate admission, introduce a narrow two-phase prepared-body contract rather than reordering the entire executor.

## Transport failure

1. preserve exact transport phase and dispatch knowledge;
2. finish attempt evidence;
3. if protocol proves no processing and native body supports physical replay, perform finite transport-private recovery inside the same visible attempt;
4. otherwise build a bounded retry view;
5. ask application retry classifier;
6. apply effect/body/count/budget/deadline gates;
7. compute jittered backoff and server not-before;
8. wait under the same absolute deadline;
9. acquire shared retry-load budget immediately before the new visible attempt;
10. loop or return the original failure.

## Response head

1. set `ResponseObserved`;
2. parse standardized server delay once;
3. deliver synchronous non-result-determining limiter feedback;
4. attribute at most one auth challenge and possibly recover/replay once;
5. apply endpoint status;
6. rejected status enters policy retry or bounded status error;
7. accepted status enters representation validation/acquisition/conversion;
8. finish visible-attempt evidence.

No generic response-head veto hook exists.

## Complete outcome

1. fix `Result<T, ApiError>`;
2. call non-fallible bounded `OperationFinished` observer event inline;
3. return the already fixed result.

Intermediate observer time contributes to the remaining budget and is checked
after return. Completion observation begins only after the semantic result is
fixed. A slow synchronous completion callback can therefore delay caller
delivery past the nominal deadline, but its return or contained panic cannot
change the fixed result.

## Stream outcome

1. fix successful accepted stream acquisition;
2. emit operation finish/stream-open evidence;
3. return the stream;
4. the stream wrapper later emits correlated finish/failure/drop evidence.

Request success never claims complete stream consumption.

## Pagination aggregate

1. one aggregate identity, deadline, shared retry budget, page/item/byte/send/cycle limits;
2. validate controller;
3. bind one page snapshot;
4. execute the complete page lifecycle above;
5. construct bounded `PageResponse<Page>`;
6. call controller `advance`;
7. enforce item, byte, identity, and progress limits;
8. continue or fix one aggregate outcome;
9. emit one aggregate final event.

A page has its own visible-attempt ceiling but no fresh operation deadline or shared retry budget.

## Cancellation

Dropping the terminal stops further local polling and propagates cancellation to owned futures/waits. It cannot retract bytes already handed to transport or prove a remote effect did not commit. Current dispatch knowledge is preserved.

No correctness-critical detached task survives without an explicit owner, finite bound, shutdown, and error path.

## Cooperative synchronous code

`RequestAtom`, representation metadata/preparation, controller transition, retry classification, and observer calls are synchronous trust boundaries. They must be prompt, finite, non-blocking, and avoid external I/O. Core can check the deadline before/after but cannot preempt arbitrary blocking Rust code.
