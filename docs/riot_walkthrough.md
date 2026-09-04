# Riot v2.1 contract walkthrough

The maintained Riot example is Concord's large-contract proof. It compiles a
platform/regional tree with inherited JSON, private API-key placement,
`subdomain replace`, optional query values, derived endpoint identities,
explicit rate-policy IDs, and offset pagination. Quota numbers are illustrative
test policy, not claims about Riot's current service.

## Compiled root preamble

This is an exact, deliberately partial excerpt from the compiled `RiotApi`
declaration; the source continues with its platform and regional scopes.

<!-- concord-source: crates/concord-examples/src/riot/api/riot.rs#riot_client -->
```concord-pseudocode
    client RiotApi {
        base "https://riotgames.com/";

        credentials {
            secret api_key;
            credential riot_api_key = api_key(secret.api_key);
        }

        headers { "User-Agent" = "ConcordRiotExample/2.1"; }
        auth { header "X-Riot-Token" = riot_api_key; }
        timeout 20s;
        request Json;
        response Json;
```

The inherited request and response representations are declared once at the
client. Inline `via` clauses and explicit fences remain visible only where an
endpoint or subtree deliberately differs.

## Platform and regional subdomains

The compiled scopes use `subdomain replace`, for example
`subdomain replace [platform.platform, "api"];`. Replacement discards inherited
generated labels while retaining the fixed base host, so `Euw1` plans against
`euw1.api.riotgames.com` and the Europe route plans against
`europe.api.riotgames.com`. The Riot planning tests cover both results,
redacted authentication, and their additive rate-policy counts.

This is different from ordinary `subdomain [...]` composition. The example
uses replacement because each route value owns the complete generated prefix
for that branch.

## A compiled representative endpoint

The current pagination proof is the following complete endpoint declaration:

<!-- concord-source: crates/concord-examples/src/riot/api/riot.rs#riot_match_ids_endpoint -->
```concord
            GET match_ids_by_puuid(
                puuid: String,
                start_time?: i64,
                end_time?: i64,
                queue?: u32,
                match_type?: crate::riot::domain::MatchType,
                start: u64 = 0,
                count: std::num::NonZeroU32 = crate::riot::domain::DEFAULT_MATCH_PAGE_SIZE
            ) -> Vec<String> {
                path ["by-puuid", endpoint.puuid, "ids"];
                query {
                    "startTime" = endpoint.start_time;
                    "endTime" = endpoint.end_time;
                    "queue" = endpoint.queue;
                    "type" = endpoint.match_type;
                    "start" = endpoint.start;
                    "count" = endpoint.count;
                }
                rate_limit { id "concord.riot.24"; key [origin]; cost 1;  allow 2000 per 10s; }
                paginate via concord::pagination::OffsetPagination {
                    offset -> endpoint.start;
                    page_size -> endpoint.count;
                }
            }
```

The endpoint demonstrates the selected ownership rules:

- postfix `?` owns Concord outer presence independently of the Rust payload;
- `start` and `count` retain typed defaults and receive pagination updates;
- the result arrow is the exact complete `Vec<String>` output;
- JSON is inherited from the client and remains visible in effective
  descriptions with its provenance;
- the endpoint ID is derived from its source path in this unpublished beta;
- the local anonymous rate-policy attachment has the explicit stable policy ID
  `concord.riot.24`.

The example does not currently contain explicit endpoint `id` clauses or a
named reusable `rate_limits` policy. Its rate rules are anonymous attachments
with explicit IDs. Those choices are documented facts, not evidence that every
production Riot integration should use the same identity or quota structure.

## Compiled caller usage

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

Construction is inert. `PageLimit::Prefix(2)` explicitly bounds the complete
aggregate and avoids numeric pagination coercion.

## Authentication and evidence boundaries

The builder accepts one bounded secret and installs an explicit process-local
limiter in the construction example. The generated endpoint surface has no
public API-key header setter. Plans and descriptions expose authentication and
policy structure, while tests prove the secret value is absent from plans,
headers, and debug output.

The contract uses controller-owned offset pagination over a vector page. An
envelope/cursor Riot endpoint is not part of the current compiled example and
is not claimed as Riot evidence; generic envelope, cursor, and link-controller
coverage lives in the maintained pagination modules and tests.

## Maintained proof

`crates/concord-examples/tests/riot_contract.rs` verifies platform and regional
origin composition, additive policy counts, optional/default controls,
pagination capability, derived endpoint identity, authentication redaction,
and representation provenance. The construction test imports the large
contract under its required `json`, `rate-limit`, and TLS feature combination.
Large expansion and semantic-diff checks measure the broader projection costs.
