# Managed lifecycle

Construction and planning are inert. Execution begins only when a request
terminal is polled.

The logical order is:

1. build the public request head;
2. compute structural request-body metadata;
3. validate the head and body metadata against their finite limits;
4. run the before-send hook before body materialization;
5. materialize the managed body and prepare replay state;
6. perform credential preflight without placing private material;
7. acquire the ordered rate-limit plan;
8. finalize credential acquisition;
9. create the attempt's native body and request;
10. place private authentication;
11. perform the HTTP exchange while the final managed request stream
    suppresses and bounds empty chunks before transport;
12. sanitize the received response head and let the core-selected limiter
    observe it under the remaining operation deadline;
13. run the user response-head hook under that same deadline;
14. perform at most one authentication recovery when allowed, or classify
    status and acquire the selected sealed response mode while suppressing and
    bounding empty transport chunks;
15. fix the result;
16. await deterministic completion outside the operation deadline.

After a response arrives, limiter failure or timeout wins before a response
hook can run. A response-hook failure or timeout wins before challenge, status,
or body handling. This ordering lets the limiter learn from every received
final head even when later user policy rejects the operation.

Generated direct methods, explicit generated endpoint values, runtime-defined
endpoints, every pagination page, and dangerous bounded terminals use this one
protocol. Provider HTTP is isolated support work rather than a second endpoint
executor. Its body-acquisition loop uses the same response empty-chunk
contract and retains its existing deadline.

Cancellation does not detach correctness work. Completion is an awaited,
delivery-blocking observation of an already-fixed result: it runs exactly once
outside the operation deadline, has no automatic completion timeout, and can
delay delivery if application code remains pending. This is a trusted
application liveness boundary, not background correctness work. Completion
events contain safe operation, attempt, timing, and outcome data only.

The client produced by `.build()` is an immutable runtime snapshot. Clones
share that snapshot, so hooks, limiter state, transport policy, and providers
cannot silently diverge between cloned clients.

See [Finite behavior](finite-behavior.md) and
[Security and redaction](security-and-redaction.md).
