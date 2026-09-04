# Rate limits

Concord separates provider quota semantics from application-selected enforcement topology.

## Declare a reusable policy

This is a partial client-owned policy registry clause. The surrounding
compiled contract supplies the client and referenced values:

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

<!-- concord-source: crates/concord-examples/src/rate_limiting/lexical.rs#configured_rate_limiter -->
```rust
pub fn construct() -> Result<(), concord::prelude::ClientBuildError> {
    let limiter = LocalRateLimiter::new();
    let client = RateLimitApi::builder()
        .set_tenant("example-tenant".to_owned())
        .configure_runtime(|runtime| runtime.rate_limiter(Arc::new(limiter)))
        .build()?;
    let _: rate_limit_api::region::ItemRequest =
        client.region("eu".to_owned()).item("record-42".to_owned());
    let _: rate_limit_api::LookupRequest = client.lookup("account-7".to_owned());
    let _: rate_limit_api::PublicRequest = client.public();
    Ok(())
}
```

Do not silently install a process-local limiter because a Cargo feature is enabled.

## Whole-plan admission

For one visible attempt, core supplies the complete effective conjunctive policy
plan to the limiter. A returned `RateLimitPermit` represents admission of that
whole plan. The bundled process-local limiter evaluates and reserves every
policy/window atomically: a wait or rejection causes no partial local
reservation, and its state lock is released before waiting.

An external/distributed implementation owns and documents whether failed
multi-policy evaluation is atomic or conservatively debits an earlier
reservation. Concord cannot promise or perform rollback across authority it
does not own.

Every policy retry, auth-recovery replay, and page visible attempt reacquires the complete plan. Protocol-proven physical transport recovery does not.

## Response feedback

```text
trait RateLimiter {
    async fn acquire(
        &self,
        request: RateLimitRequest<'_>,
    ) -> Result<RateLimitPermit, RateLimitError>;

    fn observe(&self, feedback: RateLimitResponse<'_>);
}
```

Only admission can fail the current attempt. Response feedback is synchronous, prompt, bounded, and non-result-determining.

`RateLimitRequest` contains endpoint and method, page/visible-attempt identity,
the complete canonical plan, and normalized logical origin only when that plan
uses the `origin` dimension. It contains no complete URL, path/query values, or
request headers.

Feedback adds only status, the standard server delay parsed once by core, and
the configured cooldown cap. It contains no response header map and offers no
vendor-header iteration. It may:

- update cooldown/not-before state for future acquisitions;
- mark the limiter degraded;
- emit bounded diagnostics.

It must not hide or replace the current endpoint response. A distributed implementation that needs async storage owns a bounded handoff and applies its fail-open/closed policy on future `acquire` calls.

Provider-specific feedback requires a future finite purpose-specific contract;
it is not obtained through arbitrary header access.

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
