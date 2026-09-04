# ADR 0008 — Future-admission rate feedback and one server-delay fact

- **Status:** Selected

## Decision

Only rate admission can fail the current attempt. Response feedback is synchronous/non-fallible for the current result and influences future acquisitions. Parse Retry-After/vendor reset once into one core not-before fact shared by retry and limiter.

## Property earned

A limiter persistence/update failure cannot hide a remote response or provoke unsafe caller retry. Retry and limiter do not double-wait the same server delay.

## Consequences

- explicit limiter selection;
- complete policy-plan admission;
- additive policies;
- retry/auth/page visible attempts reacquire;
- physical transport recovery does not;
- distributed async update is limiter-owned bounded handoff and future fail-open/closed policy.

