# Large API Authoring Style

Concord works best when the DSL mirrors the API contract, not when it becomes a list of unrelated paths. Use this page as the style guide for large clients.

## Model Concepts, Not Path Dumps

Prefer scopes that match provider concepts:

```rust
scope regional(region: RegionalRoute) {
    host[region, "api"]

    scope match_v5 {
        path["lol", "match"]

        GET GetMatch(match_id: String) -> Json<MatchDto> {
            path["matches", match_id]
        }
    }
}
```

Avoid one huge flat file of endpoints with repeated `host[...]`, `path[...]`, auth, retry, and rate-limit policy. Repetition hides the contract.

## Good Scope Boundaries

Use a scope when it gives a name to shared meaning:

1. routing family, such as platform versus regional host
2. upstream API family, such as account, match, summoner, or league
3. auth requirement shared by many endpoints
4. rate-limit key or profile shared by many endpoints
5. path prefix that appears in upstream docs as a section

Do not create scopes only to save one path segment if the name does not help users understand the API.

## Riot-Style Shape

A clean large API shape is nested by route family, then provider feature:

```rust
scope platform(platform: PlatformRoute) {
    host[platform, "api"]
    use_auth HeaderAuth("X-Riot-Token", riot_api_key)
    rate_limit key platform = platform

    scope summoner {
        path["lol", "summoner", "summoners"]

        GET ByPuuid(puuid: String) -> Json<SummonerDto> {
            path["by-puuid", puuid]
        }
    }

    scope champion {
        path["lol", "platform"]

        GET GetChampionRotations -> Json<ChampionRotationsDto> {
            path["champion-rotations"]
        }
    }
}
```

Call sites then stay normal and discoverable:

```rust
api.request(endpoints::platform::summoner::ByPuuid::new(platform, puuid))
    .execute()
    .await?;
```

## Generated Rust API Shape

Generated clients should feel like ordinary Rust:

1. required client vars and secrets are constructor args
2. required scope params appear before endpoint params
3. required endpoint params appear in written order
4. required request body appears after params
5. optional/defaulted values use generated setters
6. scoped endpoints live under matching endpoint modules

Example:

```rust
scope teams(region: Region) {
    path["teams"]

    GET GetTeam(id: u64, trace?: bool = false) -> Json<Team> {
        path[id]
        query { "trace" = trace }
    }
}

let endpoint = endpoints::teams::GetTeam::new(region, 42).trace(true);
let team = api.request(endpoint).execute().await?;
```

## Runtime Lifecycle Is Explicit

Lifecycle-changing operations should be visible at the call site:

```rust
api.acquire_auth_session(endpoints::auth::Login::new(username, password)).await?;

let fresh = api.request(endpoints::teams::GetTeam::new(region, 42))
    .cache_refresh()
    .debug_level(DebugLevel::V)
    .execute()
    .await?;
```

Keep runtime/environment concerns in runtime traits and generated client setters:

```rust
let api = Api::new(secret)
    .with_rate_limiter(Arc::new(GovernorRateLimiter::default()))
    .with_runtime_hooks(Arc::new(MetricsHooks))
    .with_debug_sink(Arc::new(MyDebugSink));
```

Do not add DSL syntax for process-local instrumentation, debug sinks, or transport concerns.

## Common Mistakes

Do not expect root endpoint aliases for scoped endpoints:

```rust
api.request(endpoints::platform::summoner::ByPuuid::new(platform, puuid));
```

Do not declare params inside route or policy blocks:

```rust
GET Search(q: String) -> Json<Vec<Item>> {
    query { "q" = q }
}
```

Do not compose path strings manually when typed route pieces are available:

```rust
path["summoners", "by-puuid", puuid]
```

Do not hide auth acquisition inside unrelated requests. Use explicit `acquire_auth_*` methods for endpoint-backed credentials.

## Rule Of Thumb

If the DSL line describes the upstream API contract, it belongs in the DSL. If it describes this process, deployment, instrumentation, or runtime environment, prefer a generated client setter or a `concord_core` trait.
