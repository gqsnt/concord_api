# Riot-style client

## Goal

Compose one typed client across platform and regional routing families, private
API-key authentication, endpoint groups, pagination, local rate policy, and a
separate public-data client without turning the recipe into a replacement for
the feature guides.

## Complete code

Routing uses distinct typed platform and regional values with canonical
`Display` output:

```rust,ignore
{{#include ../../crates/concord-examples/src/riot/domain/routing.rs:riot_routes}}
```

The generated Riot client owns only common base/authentication policy:

```rust,ignore
{{#include ../../crates/concord-examples/src/riot/api/riot.rs:riot_client}}
```

This compiled composition constructs a platform request, a regional paginated
request, and an independent Data Dragon request:

```rust,ignore
{{#include ../../crates/concord-examples/src/riot/usage.rs:riot_composition}}
```

The local observer demonstrates how service-specific cooldown classification
can delegate baseline bounded `Retry-After` handling:

```rust,ignore
{{#include ../../crates/concord-examples/src/riot/usage.rs:riot_observer}}
```

## Why these declarations

`PlatformRoute` and `RegionalRoute` prevent arbitrary strings from spreading
through application calls. Their `Display` values become validated DNS labels
when the `platform(...)` and `regional(...)` scopes select final hosts.

The client block owns the required protected API key, private `X-Riot-Token`
placement, public user agent, and default endpoint timeout. Descendant scopes
own service-family paths and response types. Call sites remain domain-shaped:

```rust
riot
    .regional(RegionalRoute::Europe)
    .match_v5()
    .match_ids_by_puuid(puuid)
```

The match-ID endpoint binds offset and page size to `OffsetPagination`, so the
generated request exposes `.paginate(...)`. Optional match filters remain
ordinary endpoint controls.

Rate rules at platform/regional scope use `by [host]` so illustrative local
state follows the final routing host. Endpoint rules add narrower policy. The
observer first delegates to `RetryAfterObserver`, then chooses a bounded
cooldown scope from a sanitized service header.

Data Dragon is a separate generated client because it has a different public
base and no Riot API-key contract. Sharing one declaration would blur
authentication and transport ownership.

## Adaptations

Split remote domain structs into focused Rust modules while keeping generated
declarations in service-oriented modules. Application types remain ordinary
Serde values; the macro does not need Riot-specific knowledge.

Add or remove endpoint scopes according to the service contract. Keep endpoint
IDs explicit when telemetry or rate state must survive Rust source renames.
Use `set_*`, `clear_*`, and `reset_*` only for capabilities actually generated
by the declaration.

Replace the illustrative local limiter or observer when an authoritative
distributed policy exists. Preserve baseline `Retry-After` delegation if the
custom observer should retain it.

The optional live workflow reads environment configuration and is gated behind
`riot-live`. Treat it as a manual application example, not deterministic test
support.

## Production notes

The numeric rate windows and interpretation of
`X-Rate-Limit-Type` in this repository demonstrate Concord composition. They
are not a claim about the service's current quotas or headers. Verify current
vendor contracts independently before live use.

No default test contacts Riot. Contract tests import the real domain,
declaration, and usage modules, then assert safe plans and generated types.
Keep CI deterministic and secrets absent.

Load the API key from an approved secret source, enforce HTTPS, review proxy
and trust configuration, and bound all page/item/response limits. Avoid
logging PUUIDs, match IDs, public URLs, or response fields unless the
application's data policy permits it.

Large route families can generate substantial Rust. Keep declarations grouped
by service domain, preserve one canonical type owner, and run the macro/UI
scale checks when expanding the surface.

## Related

- [Regional APIs](regional-apis.md)
- [API-key authentication](api-key-authentication.md)
- [Paginate all results](paginate-all.md)
- [Per-host and per-account rate limits](rate-limit-partitioning.md)
- [Production configuration](production-configuration.md)
- [Generated API reference](../reference/generated-api.md)
