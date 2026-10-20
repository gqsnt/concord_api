# ADR 0007 — Read-only operation observation

- **Status:** Retained unchanged in v3.0.0-beta.1 (v3 specification §13.3 EXEC-04, §17.1).
- **v2.1 status (historical):** Selected and mechanically supported on Windows/Linux; human operator comprehension remains unknown for public freeze. That platform evidence is v2.1 evidence; v3 was not run on Windows.

## Decision

Remove generic pre-send and response-head veto hooks. Expose one synchronous
non-fallible `OperationObserver` plus stream lifecycle evidence. Keep policy
decisions at domain, retry, rate, auth, response, or application owners.

Ordinary operation events identify the endpoint, method, attempt, page, and a
value-free `RequestTarget`. That target contains only bounded path-segment,
query-occurrence, and public-header-occurrence counts. Exact URLs, path/query
names and values, and request/response header names and values are absent.
Exact credential-free public metadata remains explicit caller authority through
`RequestPlan` and `ApiResponse`, where documentation warns that it can still be
personally or operationally sensitive.

## Property earned

Observation cannot silently mutate, reject, retry, acquire credentials, or import a middleware framework into every request.

## Contract

Bounded borrowed value-free events; prompt/non-blocking/no-I/O by contract;
final event cannot change result. Async export is an application/extras-owned
bounded queue. Panics remain result-neutral and are handled as specified by ADR
0013; observer health is not publicly observable in v2.1.

## Revisit

If a concrete cross-endpoint decision has no legitimate existing owner, design a separately named narrow policy. Do not restore a generic hook.
