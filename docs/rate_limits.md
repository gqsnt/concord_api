# Rate limits

Concord separates provider quota semantics from application-selected enforcement topology.

## Declare a reusable policy

```concord-pseudocode
rate_limits {
    policy tenant_reads(tenant) {
        id "catalog.tenant.read";
        key [origin, endpoint, tenant];
        cost 1;
        allow 120 per 1m;
        allow 5_000 per 1h;
    }
}
```

Attach it:

```concord
rate_limit tenant_reads(tenant = client.tenant);
```

Anonymous policies use the same explicit ID/key/cost/windows inline.

## Ownership

DSL owns:

- stable policy ID;
- formal dimension schema and bindings;
- cost;
- conjunctive windows;
- additive attachment.

Application/limiter owns:

- local versus distributed implementation;
- clock/state storage;
- wait/reject;
- fail-open/closed/fallback;
- cleanup and operational evidence.

A declaration does not imply distributed enforcement.

## Canonical key

Use a bounded `RateKeyPart`, not `Display` or ordinary `RequestAtom`.

Key encoding includes policy/dimension identities and length-framed components. Equal semantic values must produce equal canonical bytes.

Built-ins:

- `origin`: normalized logical scheme, lowercase ASCII host, effective port, never proxy route;
- `endpoint`: explicit stable endpoint ID;
- explicit domain dimensions.

Reject:

- secrets/credentials/private values;
- Concord-optional values;
- repeated/dynamic dimensions;
- page-live pagination values;
- unordered collections or collections without an explicit canonical scalar adapter.

## Policy validation

Require:

- globally unique stable policy IDs;
- one positive cost;
- at least one positive finite window;
- cost no greater than any capacity;
- all formal bindings exactly once;
- bounded dimensions/key bytes/windows;
- duplicate effective attachment of one policy ID rejected;
- endpoint built-in dimension only when the endpoint has an explicit stable ID.

All windows in a policy are conjunctive.

## Additive inheritance

Client, outer scope, inner scope, and endpoint policy attachments compose additively. A descendant cannot disable an ancestor provider constraint.

## Explicit limiter selection

A generated client with effective policies must choose explicitly:

```rust
.rate_limiter(shared_or_distributed)
.local_rate_limits()
.noop_rate_limits_for_test()
```

Do not silently install a process-local limiter because a Cargo feature is enabled.

## Whole-plan admission

For one visible attempt, core supplies the complete effective policy plan to the limiter. A local implementation can apply several windows/keys atomically where possible.

A distributed implementation documents whether failed multi-key admission can conservatively debit an earlier reservation. Concord does not promise universal transactional rollback across independent authorities.

Every policy retry, auth-recovery replay, and page visible attempt reacquires the complete plan. Protocol-proven physical transport recovery does not.

## Response feedback

```rust
trait RateLimiter {
    async fn acquire(
        &self,
        request: RateLimitRequest<'_>,
    ) -> Result<RateLimitPermit, RateLimitError>;

    fn observe(&self, feedback: RateLimitFeedback<'_>);
}
```

Only admission can fail the current attempt. Response feedback is synchronous, prompt, bounded, and non-result-determining.

Feedback may:

- update cooldown/not-before state for future acquisitions;
- update remaining/capacity observations;
- mark the limiter degraded;
- emit bounded diagnostics.

It must not hide or replace the current endpoint response. A distributed implementation that needs async storage owns a bounded handoff and applies its fail-open/closed policy on future `acquire` calls.

The limiter declares a finite static set of vendor header interests. Core supplies only those plus standardized `ServerDelayHint`, not an unrestricted response header map.

## Local limiter extra

The first-party local limiter documents:

- process-local scope;
- bucket/window algorithm;
- fairness and wakeup behavior;
- state/cardinality bounds and eviction;
- deterministic clock dependency;
- cooldown semantics;
- sharing across client clones and independent builds.

It must never be presented as distributed quota enforcement.

## Observation and security

Plans/descriptions expose policy ID, schema, cost, windows, bindings, and provenance—not resolved raw keys. Errors/events expose policy identity and wait/reject category, not key values.
