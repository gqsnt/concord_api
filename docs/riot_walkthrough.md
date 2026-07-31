# Riot walkthrough

- **Audience:** evaluators, API authors, and reviewers
- **Purpose:** product-scale explanation
- **Source:** `crates/concord-examples/src/riot/`

The Riot example demonstrates why a domain tree becomes valuable at scale. The
same client contains platform-routed and region-routed services, inherited API
key authentication, shared and local rate policies, versioned service scopes,
typed domain values, and paginated endpoints.

## Domain map

```text
RiotApi
├── platform(PlatformRoute)
│   ├── champion_mastery_v4
│   ├── league_v4
│   └── summoner_v4
└── regional(RegionalRoute)
    ├── account_v1
    └── match_v5
        ├── match_ids_by_puuid
        ├── match_by_id
        └── timeline_by_id
```

The platform and regional scope values contribute validated host labels and
shared host-keyed policies. Service/version scopes contribute route prefixes.
Endpoints contribute only local route suffixes, wire query names, local
policies, and pagination bindings.

## Read the endpoint as an API consumer

<!-- concord-source: crates/concord-examples/src/riot/api/riot.rs#riot_match_ids_endpoint -->
```rust
            GET match_ids_by_puuid(
                puuid: String,
                start_time: Option<i64>,
                end_time: Option<i64>,
                queue: Option<u32>,
                match_type: Option<crate::riot::domain::MatchType>,
                start: u64 = 0,
                count: std::num::NonZeroU32 = crate::riot::domain::DEFAULT_MATCH_PAGE_SIZE
            ) -> Json<Vec<String>> {
                path ["by-puuid", ep.puuid, "ids"];
                query {
                    "startTime" = ep.start_time;
                    "endTime" = ep.end_time;
                    "queue" = ep.queue;
                    "type" = ep.match_type;
                    "start" = ep.start;
                    "count" = ep.count;
                }
                rate_limit { 2000 / 10s; }
                paginate concord::pagination::OffsetPagination {
                    offset: bind ep.start,
                    page_size: bind ep.count,
                }
            }
```

Before reading the braces, a developer can identify:

- a read operation returning match IDs for one PUUID;
- optional time, queue, and typed match-kind filters;
- caller-visible initial pagination state;
- a non-zero page-size constraint;
- a JSON-decoded ordered collection.

The braces answer different questions: exact wire path and query names, a local
quota policy, and the two fields the controller may advance.

## Read it as an endpoint author

The author needs local knowledge:

- the endpoint belongs under regional Match v5;
- `puuid` maps to a path segment;
- four filters and two pagination values map to query keys;
- the method has a local rate window;
- `start` and `count` are live offset-controller bindings.

The author does not restate the fixed base, regional host construction,
`/lol/match/v5/matches` path, API-key placement, shared timeout, generated
scope/request types, encoding, limiter implementation, or page loop.

## Read the generated usage

<!-- concord-source: crates/concord-examples/src/riot/usage.rs#riot_paginated_request -->
```rust
    let _: concord::pagination::PaginatedRequest<_> = riot
        .regional(super::domain::RegionalRoute::Europe)
        .match_v5()
        .match_ids_by_puuid("example-puuid".to_owned())
        .set_match_type(MatchType::Ranked)
        .set_count(NonZeroU32::new(10).expect("positive page size"))
        .paginate(PageLimit::Take(2));
```

The caller navigates the domain, supplies the required PUUID, selects two
optional/defaulted controls, and chooses a finite pagination terminal. The
caller never spells `startTime`, builds a URL, places the Riot token, or updates
the offset.

## Read it as the compiler

The compiler can derive:

- direct method, explicit endpoint, request wrapper, modules, and names;
- `set_*`/`clear_*` controls for optional filters;
- `set_*`/`reset_*` controls for defaults;
- pagination capability and hidden binding implementation;
- inherited public request and policy configuration;
- visibility, collision, and pagination/rate dependency checks;
- descriptions and safe-planning metadata.

Rust checks the opaque domain types, formatting traits, controller fields, and
body/response associated types.

## Read it as the runtime

Each page uses the ordinary executor:

```text
public head
→ before-send hook
→ body preparation
→ credential acquisition
→ limiter admission
→ private auth placement
→ HTTP
→ response observation
→ recovery/retry/status decision
→ bounded response acquisition and JSON conversion
→ page completion
```

The aggregate loop clones the endpoint, writes only declared bindings, detects
canonical public-request cycles, bounds pages/items/identity state, and returns
all results or one error unless the caller explicitly chose a finite prefix.

## What the example proves about Concord

The tree is simultaneously:

- a readable API map;
- a generated navigation hierarchy;
- an ownership and visibility graph;
- an inheritance and policy attachment graph;
- a source of information used to remove repeated and error-prone code.

The example also shows the honest limit of local reasoning: an endpoint author
must understand the meaning of its parent scope and inherited policy, but does
not need to reimplement them.
