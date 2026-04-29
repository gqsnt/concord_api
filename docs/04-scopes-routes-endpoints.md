# 4. Scopes, Routes, and Endpoints

Scopes and endpoints are the heart of Concord.

Use scopes to mirror the upstream API structure.

## Scopes

A scope groups route fragments and shared policies.

```rust
scope jsonplaceholder {
    host [vars.subdomain]

    scope posts {
        path ["posts"]

        GET GetPost(id: i32)
            as get_post
            path [id]
            -> Json<Post>
    }
}
```

Usage:

```rust
api.jsonplaceholder()
   .posts()
   .get_post(1)
   .await?;
```

## Scope parameters

Scopes can have parameters.

```rust
scope regional(region: RegionalRoute) {
    host [region, "api"]

    scope account_v1_accounts {
        path ["riot", "account", "v1", "accounts"]

        GET GetAccountByPuuid(puuid: String)
            as by_puuid
            path ["by-puuid", puuid]
            -> Json<AccountDto>
    }
}
```

Usage:

```rust
api.regional(RegionalRoute::Europe)
   .account_v1_accounts()
   .by_puuid(puuid)
   .await?;
```

Scope parameters are inherited by child scopes and endpoints.

## Host fragments

`host [...]` prepends labels to the client domain.

```rust
client RiotClient {
    base https "riotgames.com"
}

scope platform(platform: PlatformRoute) {
    host [platform, "api"]
}
```

For `platform = EUW1`:

```text
euw1.api.riotgames.com
```

Host fragments are DNS labels. Do not include dots in dynamic host labels.

## Path fragments

`path [...]` appends URL path segments.

```rust
scope posts {
    path ["posts"]

    GET GetPostComments(post_id: i32)
        as comments
        path [post_id, "comments"]
        -> Json<Vec<Comment>>
}
```

For `post_id = 1`:

```text
/posts/1/comments
```

Dynamic path values are percent-encoded as one segment.

## Root endpoints

Root endpoints are allowed:

```rust
GET Ping
    as ping
    path ["ping"]
    -> Json<Ping>
```

For real APIs, prefer scopes so the shape remains readable.

## Endpoint aliases

The endpoint type keeps its declared name:

```rust
GET GetSummonerByPuuid(puuid: String)
```

The facade method can be customized:

```rust
GET GetSummonerByPuuid(puuid: String)
    as by_puuid
    path ["by-puuid", puuid]
    -> Json<SummonerDto>
```

Usage:

```rust
api.platform(EUW1)
   .summoner_v4()
   .by_puuid(puuid)
   .await?;
```

If no alias is written, Concord derives a snake-case method from the endpoint name.

## Endpoint parameters

```rust
GET GetPosts(user_id?: u32, x_debug: bool = true)
    -> Json<Vec<Post>>
{
    query {
        "userId" = user_id
    }

    headers {
        "x-debug" = fmt["test:", x_debug]
    }
}
```

Parameter kinds:

| Form | Meaning |
| --- | --- |
| `id: u32` | required |
| `user_id?: u32` | optional |
| `x_debug: bool = true` | defaulted |

Required params are constructor/facade method arguments.

Optional and defaulted params usually become builder setters on the pending request.

Example:

```rust
api.jsonplaceholder()
   .posts()
   .get_posts()
   .user_id(1)
   .x_debug(false)
   .await?;
```

## Empty endpoint body

Semicolon form is valid for simple endpoints without path/policy additions:

```rust
POST CreatePost(body: Json<NewPost>) -> Json<Post>;
```

Use a block when the endpoint needs path/query/headers/auth/cache/retry/rate-limit/pagination.

## Large API style

Large APIs should match the upstream documentation structure.

```rust
scope regional(region: RegionalRoute) {
    host [region, "api"]

    scope match_v5_matches {
        path ["lol", "match", "v5", "matches"]

        GET GetMatchIdsByPuuid(
            puuid: String,
            queue?: u16,
            start_time?: i64,
            end_time?: i64,
            start: u64 = 0,
            count: u64 = 20,
        )
            -> Json<Vec<String>>
        {
            path ["by-puuid", puuid, "ids"]

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
    }
}
```
