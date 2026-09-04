# ADR 0007 — Read-only operation observation

- **Status:** Selected; event vocabulary gate pending

## Decision

Remove generic pre-send and response-head veto hooks. Expose one synchronous non-fallible `OperationObserver` plus stream lifecycle evidence. Keep policy decisions at domain, retry, rate, auth, response, or application owners.

## Property earned

Observation cannot silently mutate, reject, retry, acquire credentials, or import a middleware framework into every request.

## Contract

Bounded borrowed sanitized events; prompt/non-blocking/no-I/O by contract; final event cannot change result. Async export is an application/extras-owned bounded queue.

## Revisit

If a concrete cross-endpoint decision has no legitimate existing owner, design a separately named narrow policy. Do not restore a generic hook.

