# ADR 0013 — Observer panic diagnostics

- **Status:** Selected and mechanically supported on Windows/Linux; public reporting deferred

## Decision

Continue to contain every `OperationObserver` panic so it cannot alter an
endpoint, pagination, or stream result. Each contained panic saturatingly
increments one private process diagnostic counter. The counter records only the
failure occurrence; it retains no event, URL, header, body, credential, public
value, or native panic text.

Do not add a new public diagnostics subsystem during v2.1 release closure. The
existing debug dispatcher is request-local and is not carried by post-result
stream lifecycle observation, so it is not a complete stable sink for this
condition. Recursively invoking the observer is forbidden.

## Property earned

Observer failure is no longer silently discarded inside the runtime, while the
already-fixed caller result and the information-flow boundary remain unchanged.

## Limitation and revisit trigger

The private counter is not yet externally queryable and emits no diagnostic.
Add a bounded public/trusted health surface only when a concrete operator can
specify ownership, cardinality, rate limiting, shutdown behavior, and how all
direct, pagination, and lazy-stream callbacks report through the same sink.
