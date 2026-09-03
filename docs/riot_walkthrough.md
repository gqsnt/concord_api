# Nested API walkthrough

The Riot example demonstrates how a larger contract remains readable when
ownership follows the service domain. A root client owns shared values,
credentials, policy, and routing. Regional/platform groups are scopes. Individual
operations own their required/optional/defaulted inputs and local wire deltas.

## Read one endpoint locally

<!-- concord-source: crates/concord-examples/src/riot/api/riot.rs#riot_match_ids_endpoint -->
```rust
            GET match_ids_by_puuid(
                puuid: String,
                start_time?: i64,
                end_time?: i64,
                queue?: u32,
                match_type?: crate::riot::domain::MatchType,
                start: u64 = 0,
                count: std::num::NonZeroU32 = crate::riot::domain::DEFAULT_MATCH_PAGE_SIZE
            ) -> Vec<String> using Json<Vec<String>> {
                path ["by-puuid", endpoint.puuid, "ids"];
                query {
                    "startTime" = endpoint.start_time;
                    "endTime" = endpoint.end_time;
                    "queue" = endpoint.queue;
                    "type" = endpoint.match_type;
                    "start" = endpoint.start;
                    "count" = endpoint.count;
                }
                rate_limit { allow 2000 per 10s; }
                paginate with concord::pagination::OffsetPagination {
                    offset -> endpoint.start;
                    page_size -> endpoint.count;
                }
            }
```

The caller supplies `puuid`. Optional filters become exact controls. `start` and
`count` retain one evaluated default each and are legal controls before
pagination. The controller then owns only those two live endpoint slots. The
declared `Vec<String>` is one page's domain output; pagination flattens pages.

## Generated use keeps the tree

<!-- concord-source: crates/concord-examples/src/riot/usage.rs#riot_paginated_request -->
```rust
    let _: concord::pagination::PaginatedRequest<_> = riot
        .regional(super::domain::RegionalRoute::Europe)
        .match_v5()
        .match_ids_by_puuid("example-puuid".to_owned())
        .with_match_type(MatchType::Ranked)
        .set_count(NonZeroU32::new(10).expect("positive page size"))
        .paginate(PageLimit::Prefix(2));
```

The direct navigation, generated modules, unbound navigation, and contract
description all retain `regional -> match_v5 -> match_ids_by_puuid`. The caller
does not pass ancestor values again or manipulate a generic request bag.

## Operational reading

The effective endpoint carries inherited host/path/auth/rate/timeout facts plus
local query, rate, pagination, output, and capability facts with origins. A plan
formats only credential-free public shape and redacts declared public fields.
Execution adds private material after admission and runs every page through the
same executor under one aggregate deadline.

The example is deliberately a modeling fixture, not a claim that Concord is an
OpenAPI replacement. It shows the product law: an API author, caller, reviewer,
and operator can predict the same resolved contract without understanding macro
internals.
