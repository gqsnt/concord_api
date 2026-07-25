# Managed lifecycle

Construction and planning are inert. Execution begins only when a request
terminal is polled.

The logical order is:

1. validate operation and build the safe public request head;
2. run the before-send hook with sanitized semantic data;
3. resolve and acquire the ordered rate-limit plan;
4. acquire credential material;
5. materialize the bounded request body;
6. build and send the private native request;
7. sanitize the received response head and let the core-selected limiter
   observe it under the remaining operation deadline;
8. run the user response-head hook under that same deadline;
9. perform at most one authentication-recovery attempt when allowed;
10. classify status and acquire the selected sealed response mode;
11. fix the result and run deterministic completion outside the deadline.

After a response arrives, limiter failure or timeout wins before a response
hook can run. A response-hook failure or timeout wins before challenge, status,
or body handling. This ordering lets the limiter learn from every received
final head even when later user policy rejects the operation.

Generated direct methods, explicit generated endpoint values, runtime-defined
endpoints, every pagination page, and dangerous bounded terminals use this one
protocol. Provider HTTP is isolated support work rather than a second endpoint
executor.

Cancellation does not detach correctness work. Once result fixation reaches
required completion, dropping the caller cannot leave a background task whose
outcome determines the request result. Completion events contain safe
operation, attempt, timing, and outcome data only.

The client produced by `.build()` is an immutable runtime snapshot. Clones
share that snapshot, so hooks, limiter state, transport policy, and providers
cannot silently diverge between cloned clients.

See [Finite behavior](finite-behavior.md) and
[Security and redaction](security-and-redaction.md).
