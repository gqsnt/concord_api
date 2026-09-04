# Concord v2.1 overview

## Product thesis

Concord is not a general HTTP builder and not an OpenAPI replacement. It is a contract language that turns one API-shaped domain model into a generated Rust client and one bounded HTTP execution model.

```text
author declaration
  → canonical semantic contract
  → generated caller API
  → plan, docs, reflection, and semantic diff
  → managed execution and operational evidence
```

The macro and core exist to make these projections truthful. They are not the product center.

## Primary experiences

### Client creator

Builds one immutable client/security-context snapshot with required values, secrets/providers, retry policy, explicit rate limiter, and grouped HTTP/runtime configuration.

### Endpoint author

Works inside a literal domain tree. A normal JSON endpoint contains only exact domain inputs/output plus local route or policy changes. Stable representation defaults are declared once at a client or scope.

### Caller

Navigates generated scopes and endpoints through inherent methods. Optional/default controls are exact and capability-specific terminals appear only when declared.

### Reviewer

Can inspect the effective representation, route, public patches, status, effect, auth structure, rate policies, pagination, and provenance without mentally executing inheritance.

### Extension author

Implements one semantic role—atom, representation, credential provider, limiter, retry classifier, or pagination controller—without native request/response authority.

### Operator

Sees operation, page, attempt, retry, auth recovery, stream, deadline, rate wait, and dispatch-knowledge evidence without body, credential, rate-key, or sensitive-value leakage.

## Selected language shape

This complete overview contract is maintained and compiled with the quick-start
program:

<!-- concord-source: crates/concord-examples/src/getting_started/domain_tree.rs#domain_tree_contract -->
```concord
api! {
    client CatalogApi {
        timeout 30s;
        base "http://catalog.example.test/";
        response Json;

        values {
            tenant: String;
        }

        path ["api"];
        query { "tenant" = client.tenant; }

        scope regional(region: String) {
            subdomain replace [regional.region, "api"];
            path ["v1", "catalog"];

            GET item_summaries(
                category?: String,
                offset: u64 = 0,
                count: std::num::NonZeroU32 = DEFAULT_PAGE_SIZE,
            ) -> Vec<ItemSummary> {
                path ["items"];
                query {
                    "category" = endpoint.category;
                    "offset" = endpoint.offset;
                    "count" = endpoint.count;
                }
                paginate via concord::pagination::OffsetPagination {
                    offset -> endpoint.offset;
                    page_size -> endpoint.count;
                }
            }
        }
    }
}
```

## What v2.1 deliberately separates

```text
Rust payload type               ≠ Concord presence
presence                        ≠ default/initializer
collection value                ≠ query/header occurrence count
request representation          ≠ response representation
representation                  ≠ caller delivery lifecycle
status acceptance               ≠ response conversion
mechanical body replay          ≠ remote effect class
policy retry                    ≠ auth recovery
visible attempt                 ≠ transport-private physical retry
page operation                  ≠ pagination aggregate
public protocol data            ≠ non-sensitive data
inspection                      ≠ mutation authority
```

## One runtime

Every generated, unbound, runtime-defined, retry, auth-recovery, accepted-byte, stream-acquisition, and pagination request enters the same executor. Core alone owns native requests/responses, deadline propagation, private auth placement, rate admission, body polling, response acquisition, and pagination sequence.

## Negative space

Concord intentionally does not expose arbitrary native middleware, automatic redirects, arbitrary origins, cookie/cache/browser semantics, CONNECT/TRACE/upgrades, or unbounded execution. Applications needing those use a lower-level HTTP client or a separately designed product boundary.
