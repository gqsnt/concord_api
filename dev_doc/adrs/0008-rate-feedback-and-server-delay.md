# ADR 0008 — Future-admission rate feedback and one server-delay fact

- **Status:** Retained in v3.0.0-beta.1 except the custom-limiter partial-reservation allowance below, which is superseded by [ADR 0023](0023-rate-window-sets-policy-identity-and-atomic-admission.md) (atomic all-or-none admission, RATE-04).
- **v2.1 status (historical):** Selected

## Decision

Only rate admission can fail the current attempt. Response feedback is
synchronous/non-fallible for the current result and influences future
acquisitions. The immutable client runtime owns a private wall-clock source.
Core captures it once when a response head arrives and parses standard
`Retry-After` once into one typed `ServerDelayHint` shared by retry, limiter
feedback, observer summaries, and rejected-status capture. Rate feedback
contains typed status, delay, plan, and attempt context; it contains no generic
response-header view or complete URL. A normalized logical origin is supplied
only when the effective plan explicitly contains the built-in `origin`
dimension.

## Property earned

A limiter persistence/update failure cannot hide a remote response or provoke unsafe caller retry. Retry and limiter do not double-wait the same server delay.

## Consequences

- explicit limiter selection;
- complete conjunctive policy-plan admission: one returned permit admits the
  whole plan;
- the bundled process-local limiter evaluates and reserves every policy
  atomically; custom distributed implementations document whether a rejected
  multi-policy evaluation is atomic or conservatively consumes an earlier
  reservation because core cannot roll back external authority;
- additive policies;
- retry/auth/page visible attempts reacquire;
- physical transport recovery does not;
- distributed async update is limiter-owned bounded handoff and future fail-open/closed policy.
