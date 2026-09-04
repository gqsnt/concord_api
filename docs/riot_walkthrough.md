# Riot v2.1 contract walkthrough

The Riot example is the large-contract proof for v2.1. It exercises inherited JSON, regional/platform subdomains, many endpoint leaves, optional query values, stable IDs, private API-key auth, additive rate policies, pagination, and semantic-diff scale.

## Root contract

Target shape:

```concord-pseudocode
api! {
    client RiotApi {
        base "https://riotgames.com/";
        timeout 20s;
        response Json;

        credentials {
            secret api_key;
            credential riot_api_key = api_key(secret.api_key);
        }

        headers { "User-Agent" = "ConcordRiotExample/2.1"; }
        auth { header "X-Riot-Token" = riot_api_key challenge never; }

        scope platform(platform: PlatformRoute) {
            subdomain [platform.platform, "api"];
            path ["lol"];
            // endpoints...
        }

        scope regional(region: RegionalRoute) {
            subdomain [regional.region, "api"];
            // endpoints...
        }
    }
}
```

The declaration places `response Json;` at the narrowest stable owner and leaves only local representation exceptions inline.

## Endpoint modeling

```concord-pseudocode
GET top_masteries_by_puuid(
    puuid: String,
    count?: u32,
) -> Vec<ChampionMasteryDto> {
    id "riot.champion-mastery.top-by-puuid";
    path ["champion-masteries", "by-puuid", endpoint.puuid, "top"];
    query { "count" = endpoint.count; }
}
```

Review points:

- postfix `?` owns the Concord presence layer independently of the Rust payload type;
- the result arrow is the exact awaited vector;
- JSON is inherited and must appear in effective docs;
- add a stable endpoint ID rather than deriving operations from source names;
- string/enum route values need `RequestAtom` or explicit adapters.

## Subdomains

`subdomain` composes labels. Place stable common labels at the parent and only local labels in descendants when that better matches the actual hierarchy.

Test platform and regional routes for:

- normalized logical origin;
- retained port/base path;
- proxy independence;
- ASCII label validation;
- plan/redaction and rate-origin identity.

## Query cardinality

Audit every collection/optional query:

- optional scalar: `name?: T` with assignment or `when_present` according to inheritance intent;
- repeated collection: explicit `each`;
- comma/list collection: explicit scalar adapter;
- nested maps or complex filters: use a domain atom adapter or body rather than hidden serialization.

Example repeated IDs:

<!-- concord-source: crates/concord-examples/src/riot/usage.rs#riot_paginated_request -->
```rust
    let _: concord::pagination::PaginatedRequest<_> = riot
        .regional(super::domain::RegionalRoute::Europe)
        .match_v5()
        .match_ids_by_puuid("example-puuid".to_owned())
        .set_match_type(Some(MatchType::Ranked))
        .set_count(NonZeroU32::new(10).expect("positive page size"))
        .paginate(PageLimit::Prefix(2));
```

## Rate policy migration

Current anonymous windows should receive explicit stable IDs and cost. Shared platform/regional quotas should become reusable policies when the dimension schema is genuinely common.

```concord-pseudocode
rate_limits {
    policy platform_reads(platform) {
        id "riot.platform.reads";
        key [origin, platform];
        cost 1;
        allow 500 per 10s;
        allow 30_000 per 10m;
    }
}
```

Do not claim the example numbers are current provider quotas unless verified. Keep them explicitly illustrative in docs/tests.

The example client must choose a limiter explicitly. A local limiter is acceptable for deterministic demonstration but must be named process-local.

## Authentication

Preserve one versioned slot/private placement. No endpoint receives a public API-key header setter. Plans, descriptions, errors, semantic diff, and observer events show auth structure but never material.

## Pagination

The Riot example should include at least one real-shaped cursor/page endpoint whose one-page output is an envelope rather than relying only on `Vec<T>` extraction. Use controller-owned items/continuation and test complete aggregate behavior.

## Large-contract evidence

Use Riot to measure:

- authored representation-line reduction;
- generated method count and autocomplete burden;
- macro token/compile/rust-analyzer cost;
- effective representation provenance in Rustdoc;
- semantic diff expansion for parent representation, query, auth, and rate changes;
- invalid pair diagnostic locality;
- migration-report quality for ambiguous optional and collection payloads.

## Completion criteria

- no retired declaration surface or implicit optional/repetition behavior;
- every endpoint has a stable ID or an explicit documented reason for retaining
  derived identity in the unpublished integration candidate;
- effective docs show inherited representation and policy origins;
- all illustrative quota/auth statements remain clearly labeled;
- contract compiles under no-default and required feature combinations;
- Riot-specific runtime/planning tests and large-contract benchmarks pass.
