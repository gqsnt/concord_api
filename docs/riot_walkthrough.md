# Riot v3 contract walkthrough

The maintained Riot example is Concord's large-contract proof. One `RiotApi`
declaration compiles 46 endpoints across a platform and a regional scope tree
with inherited JSON, private API-key placement, `subdomain replace`, direct
query placements, derived endpoint identities, 28 explicitly identified rate
policies over reusable window sets, and offset pagination. Quota numbers and
partitioning are illustrative Concord policy, not claims about Riot's current
service quotas.

The maintained contract targets V3 directly. Its full recording and rate-state
relations are checked against the current [V3 contract evidence](#v3-contract-evidence).

## Compiled root preamble

This is an exact, deliberately partial excerpt from the compiled `RiotApi`
declaration; the source continues with its platform and regional scopes.

<!-- concord-source: crates/concord-examples/src/riot/api/riot.rs#riot_client -->
<!-- concord-pseudocode -->
```rust
    client RiotApi {
        base "https://riotgames.com/";

        credentials {
            secret api_key;
            credential riot_api_key = api_key(secret.api_key);
        }

        headers { "User-Agent" = "ConcordRiotExample/3"; }
        auth { header "X-Riot-Token" = riot_api_key; }
        timeout 20s;
        request Json;
        response Json;

        // Window sets hold numbers only. They share no state: every
        // attachment below names its own policy ID, and the ID with the
        // ordered key alone selects quota state.
        rate_limits {
            windows application { allow 500 per 10s; allow 30000 per 10m; }
            windows standard { allow 20000 per 10s; allow 1200000 per 10m; }
            windows apex_leagues { allow 30 per 10s; allow 500 per 10m; }
            windows league_entries { allow 50 per 10s; }
            windows clash_teams { allow 200 per 1m; }
            windows clash_tournaments { allow 10 per 1m; }
            windows spectator { allow 3000 per 10s; allow 180000 per 10m; }
            windows summoners { allow 1600 per 1m; }
            windows accounts { allow 1000 per 1m; }
            windows match_reads { allow 2000 per 10s; }
        }
```

The inherited request and response representations are declared once at the
client. Inline `via` clauses and explicit fences remain visible only where an
endpoint or subtree deliberately differs: `status_v4` and `tournament_stub_v5`
repeat a default so its provenance is their scope, and `tournament_v5`
declares `request explicit; response explicit;` so that every endpoint below it
names its representations with `via`.

## Window sets, policy IDs and sharing

The `rate_limits` registry holds only numbers. A window set grants no identity
and shares no state (§12.1). State is selected by a policy ID together with
its ordered key, so every attachment names its own ID:

<!-- concord-pseudocode -->
```rust
rate_limit "concord.riot.21" key [origin] using accounts;
rate_limit "concord.riot.22" key [origin] using accounts;
```

These two account endpoints use equal numbers but separate quota state. Nine
policy IDs reuse the `standard` set and two reuse `application`, and none of
them shares state with another. Sharing exists only where the source declares
it:

- one application bucket per routing host: `concord.riot.1` covers the 27
  platform endpoints and `concord.riot.20` covers the 19 regional endpoints,
  both keyed by `origin`;
- three scope-shared method buckets, declared once at their scope and covering
  every endpoint below it: `concord.riot.2` (`champion_mastery_v4`),
  `concord.riot.15` (`challenges_v1`) and `concord.riot.28`
  (`tournament_stub_v5`);
- every other policy covers exactly one endpoint.

Most plans therefore carry two policies, the application bucket plus a method
bucket. `league_by_id` and the seven `tournament_v5` endpoints carry only
their application bucket. Merging equal numbers into one ID, or hoisting an
attachment to a wider scope, would be a sharing change and is not a style
edit; the baseline edit battery demonstrates both.

## Platform and regional subdomains

The scopes use `subdomain replace [platform, "api"];` and
`subdomain replace [regional, "api"];`. Replacement discards inherited
generated labels while keeping the fixed base host, so `PlatformRoute::Euw1`
plans against `euw1.api.riotgames.com` and `RegionalRoute::Europe` plans
against `europe.api.riotgames.com`. Ordinary `subdomain [...]` composition
would prepend labels instead. The example uses replacement because each route
value owns the complete generated prefix for its branch.

## A compiled representative endpoint

The pagination proof is the following complete endpoint declaration:

<!-- concord-source: crates/concord-examples/src/riot/api/riot.rs#riot_match_ids_endpoint -->
<!-- concord-dsl -->
```rust
            GET match_ids_by_puuid(
                puuid: String,
                query("startTime") start_time?: i64,
                query("endTime") end_time?: i64,
                query queue?: u32,
                query("type") match_type?: MatchType,
                query start: u64 = 0,
                query count: NonZeroU32 = DEFAULT_MATCH_PAGE_SIZE,
            ) -> Vec<String> {
                path "by-puuid/{puuid}/ids";
                rate_limit "concord.riot.24" key [origin] using match_reads;
                paginate via concord::pagination::OffsetPagination {
                    init concord::pagination::OffsetInit { offset: start, page_size: count };
                    cursor -> start;
                }
            }
```

The endpoint demonstrates the selected ownership rules:

- each `query` parameter places its own value directly; `query("startTime")`
  keeps the provider's key spelling, and declaration order is wire order;
- postfix `?` owns Concord outer presence independently of the Rust payload,
  so an absent `queue` emits no query occurrence;
- `start` and `count` keep typed defaults. `init` builds the controller's
  public `OffsetInit` from them, and `cursor -> start` maps the controller's
  entire cursor to the `start` slot for continuation requests (PAGE-01);
- the result arrow is the complete `Vec<String>` output;
- JSON is inherited from the client and is visible, with its provenance, in
  effective descriptions;
- the endpoint ID is navigation-derived:
  `riot_api.regional.match_v5.match_ids_by_puuid` (IDENTITY-01). It changes if
  the client, scope or endpoint is renamed or moved;
- the attachment `concord.riot.24` gives this endpoint its own quota state
  over the shared `match_reads` numbers. `match_by_id` and `timeline_by_id`
  use the same numbers under their own IDs.

The example contains no explicit endpoint `id` clauses and no named `policy`
declarations. Those are documented choices for this example, not a
recommendation that every Riot integration use the same identity or quota
structure.

## Compiled caller usage

<!-- concord-source: crates/concord-examples/src/riot/usage.rs#riot_paginated_request -->
```rust
    let _: concord::pagination::CollectPrefix<String> = riot
        .regional(super::domain::RegionalRoute::Europe)
        .match_v5()
        .match_ids_by_puuid("example-puuid".to_owned())
        .set_match_type(MatchType::Ranked)
        .set_count(NonZeroU32::new(10).expect("positive page size"))
        .collect_prefix(2);
```

Construction is inert. The optional `match_type` takes its payload directly
through `set_match_type`; `replace_match_type(Option<_>)` is the exact
presence control when a caller already holds an `Option`. `collect_prefix(2)`
bounds the complete aggregate to at most two pages and needs no numeric
pagination coercion.

## Authentication and evidence boundaries

The builder accepts one bounded secret through `.api_key(SecretString)`, and
the construction example installs an explicit process-local limiter, which any
client with rate policies requires. The API key reaches the wire only through
the declared `X-Riot-Token` header placement of the `riot_api_key` credential.
The generated request surface has no public setter for that header. Plans and
descriptions expose the authentication and policy structure, and tests prove
the secret value is absent from plans, public headers and debug output.

The contract uses controller-owned offset pagination over a vector page. An
envelope or cursor Riot endpoint is not part of the compiled example and is
not claimed as Riot evidence; generic envelope, cursor and link-controller
coverage lives in the maintained pagination modules and tests.

## V3 contract evidence

`crates/concord-examples/tests/baseline_recording.rs` drives the generated V3
client through a recording limiter.
It compares all 92 recorded contexts (46 endpoints, each under two routing
values) with `fixtures/contracts/v3_riot_recording.json` field by field:
endpoint ID, method, URL scheme, host and route, public query and header
occurrences in order, timeout selection, auth placement, and for every rate
policy its ID, cost, windows, key labels, encoding IDs and canonical key bytes.
It then confirms the sharing relation dynamically with the real
`LocalRateLimiter` on the generated keys. The complete fresh recording must
equal the committed V3 fixture byte for byte. There are no interversion
exceptions in this assertion.

The edit battery independently checks eight contract changes, including scope
renaming with pinned IDs, moving an endpoint, shadowing an ancestor value,
hoisting quotas, merging policy identities, changing cost or key encoding, and
adding an initialized value. Each edit declares the facts that should change
and asserts that the remaining facts are preserved.

## Maintained proof

`crates/concord-examples/tests/riot_contract.rs` verifies:

- platform and regional origin composition, policy counts and redacted
  authentication
  (`platform_and_regional_plans_compose_hosts_limits_and_redacted_auth`);
- optional and default controls, pagination binding and explicit descriptions
  (`optional_defaults_pagination_binding_and_explicit_description_are_predictable`);
- provider query spelling and order
  (`query_programs_keep_provider_key_spelling_and_order`);
- the original sharing groups
  (`static_policy_coverage_keeps_the_original_sharing_groups`) and window-set
  reuse without shared state
  (`window_sets_factor_numbers_without_sharing_state`);
- timeout, derived identity and auth placement on all 46 endpoints
  (`every_endpoint_keeps_timeout_identity_and_auth_placement`);
- inert, owned requests with common controls, the independent Data Dragon
  contract, and representation provenance.

The Riot tests require the `json`, `rate-limit` and `default-tls` features.
