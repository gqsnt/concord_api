# Concord v4 Style

Concord v4 code should read as a tree:

```text
client = root
scope = branch
endpoint = leaf
policy = inherited branch behavior
request = walking the generated tree to a leaf
```

## Client Order

Use this order unless an example has a narrow reason not to:

```rust
client Api {
    base https "example.com"
    secret api_key: String
    credential key = api_key(secret.api_key)

    headers {
        "user-agent" = "Api/1.0"
    }

    default {
        retry read
        rate_limit app
    }

    retry read {
        attempts 2
        methods [GET]
        on [429, 500]
        retry_after
    }

    rate_limit app {
        bucket application by [host] {
            500 / 10s
        }
    }
}
```

## Scope Order

Route first, policy second, children and endpoints last:

```rust
scope regional(region: RegionalRoute) {
    host [region, "api"]
    path ["riot", "account", "v1"]

    auth header "X-Riot-Token" = key
    retry read
    rate_limit app

    scope accounts {
        path ["accounts"]

        GET GetAccountByPuuid(puuid: String)
            as by_puuid
            path ["by-puuid", puuid]
            -> Json<AccountDto>
    }
}
```

## Endpoint Leaves

Small endpoints should stay compact:

```rust
GET Me
    as me
    path ["me"]
    -> Json<User>
```

Use blocks for local query/header/pagination/policy:

```rust
GET GetMatchIdsByPuuid(
    puuid: String,
    queue?: u16,
    start_time?: i64,
    end_time?: i64,
    start: u64 = 0,
    count: u64 = 20,
)
    as ids_by_puuid
    path ["by-puuid", puuid, "ids"]
    -> Json<Vec<String>>
{
    query {
        queue
        "startTime" = start_time
        "endTime" = end_time
        start
        count
    }

    paginate OffsetLimitPagination {
        offset = start
        limit = count
    }

    rate_limit match_v5_method
}
```

## Aliases

Use aliases when the generated facade name would be noisy:

```text
GetAccountByPuuid       -> as by_puuid
GetAccountByRiotId      -> as by_riot_id
GetSummonerByPuuid      -> as by_puuid
GetMatchIdsByPuuid      -> as ids_by_puuid
GetTimeline             -> as timeline
GetChampionRotations    -> as rotations
GetLeagueEntries        -> as by_queue
```

Do not rely on aggressive automatic alias inference. Make aliases explicit.

## Facade-First Usage

Examples should teach tree walking first:

```rust
let ids = riot
    .regional(region)
    .match_v5_matches()
    .ids_by_puuid(puuid)
    .count(100)
    .paginate()
    .collect()
    .await?;
```

Explicit endpoint constructors remain available for tests and advanced usage:

```rust
api.request(endpoints::GetUser::new(1)).execute().await?;
```

