# ADR 0008 — Future-admission rate feedback and one server-delay fact

- **Status:** Selected

## Decision

Only rate admission can fail the current attempt. Response feedback is synchronous/non-fallible for the current result and influences future acquisitions. Core parses standard `Retry-After` once into one server-delay fact shared by retry and limiter. Rate feedback contains typed status, delay, plan, and attempt context; it contains no generic response-header view.

## Property earned

A limiter persistence/update failure cannot hide a remote response or provoke unsafe caller retry. Retry and limiter do not double-wait the same server delay.

## Consequences

- explicit limiter selection;
- complete policy-plan admission;
- additive policies;
- retry/auth/page visible attempts reacquire;
- physical transport recovery does not;
- distributed async update is limiter-owned bounded handoff and future fail-open/closed policy.
