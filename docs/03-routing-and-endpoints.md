# 3. Routing and Endpoints

Routes are built by composing host labels and path segments from client, scope, and endpoint definitions.

The client provides the base scheme and host. Scopes and endpoints add route pieces.

```rust
api! {
    client Client {
        scheme: https,
        host: "typicode.com",

        vars {
            subdomain: String = "jsonplaceholder".to_string()
        }
    }

    scope jsonplaceholder {
        host[vars.subdomain]

        scope posts {
            path["posts"]

            GET GetPost {
                params { id: i32 }
                path[id]
                -> Json<Post>;
            }
        }
    }
}
```

`GetPost::new(1)` targets:

```text
https://jsonplaceholder.typicode.com/posts/1
```

## Scopes

A `scope` groups child scopes and endpoints.

```rust
scope platform {
    params { platform: PlatformRoute }
    host[platform, "api"]
    path["lol"]

    scope summoner_v4 {
        path["summoner", "v4", "summoners"]

        GET GetSummonerByPuuid {
            params { puuid: String }
            path["by-puuid", puuid]
            -> Json<SummonerDto>;
        }
    }
}
```

The generated endpoint constructor includes required parameters from parent scopes and the endpoint. A call might look like:

```rust
api.request(endpoints::GetSummonerByPuuid::new(
        "abc".to_string(),
        PlatformRoute::Euw1,
    ))
    .execute()
    .await?;
```

Use scopes when multiple endpoints share a host prefix, path prefix, auth requirement, retry policy, rate-limit plan, cache policy, headers, query values, or route parameters.

## Host labels

`host[...]` prepends labels to the client host.

```rust
client RiotClient {
    scheme: https,
    host: "riotgames.com",
}

scope platform {
    params { platform: PlatformRoute }
    host[platform, "api"]
}
```

If `platform` formats as `euw1`, the host becomes `euw1.api.riotgames.com`.

Host labels must be valid DNS labels after formatting. The routing tests reject invalid labels such as values containing dots, labels starting with a dash, or labels containing underscores.

## Path segments

`path[...]` appends URL path segments.

```rust
scope posts {
    path["posts"]

    GET GetPostComments {
        params { post_id: i32 }
        path[post_id, "comments"]
        -> Json<Vec<Comment>>;
    }
}
```

`post_id = 10` builds `/posts/10/comments`.

Dynamic path values are percent-encoded as a single segment. If `match_id` is `a/b`, then:

```rust
path["matches", match_id]
```

builds:

```text
/matches/a%2Fb
```

## Optional path values

Optional parameters can appear in a path. Missing values are omitted without leaving a double slash.

```rust
GET One {
    params { opt?: String }
    path["x", opt, "y"]
    -> Json<()>;
}
```

```rust
api.request(endpoints::One::new()).execute().await?;
// /x/y

api.request(endpoints::One::new().opt("z".to_string())).execute().await?;
// /x/z/y
```

## `part[...]` values

Use `part[...]` to build one segment, header value, or query value from multiple pieces.

```rust
GET One {
    params { v: String }
    path["x", part["p", v]]
    -> Json<()>;
}
```

If `v` is `a/b`, the final path is `/x/pa%2Fb`. The slash is encoded because `part[...]` still produces one path segment.

When a `part[...]` references an optional value and that value is missing, the containing route item or policy value is omitted.

```rust
GET One {
    params { v?: String }
    path["x", part["p", v], "y"]
    -> Json<()>;
}
```

`One::new()` builds `/x/y`. `One::new().v("z".to_string())` builds `/x/pz/y`.

## Endpoint shape

Most endpoint definitions use the block form.

```rust
GET GetUser {
    params { id: u32 }
    path[id]
    headers { "x-debug" = true }
    query { "include" = "profile" }
    -> Json<User>;
}
```

The method is an HTTP method identifier such as `GET`, `POST`, `PUT`, `DELETE`, `PATCH`, `HEAD`, or `OPTIONS`.

The endpoint name becomes a generated Rust type under `endpoints`.

```rust
let ep = endpoints::GetUser::new(42);
let user = api.request(ep).execute().await?;
```

## Endpoint route inheritance

Routes are concatenated from outer scopes to inner scopes to endpoint.

```rust
scope api {
    path["api"]

    scope public {
        path["public"]

        GET GetUser {
            params { id: u32 }
            path["users", id]
            -> Json<User>;
        }
    }
}
```

This endpoint builds `/api/public/users/{id}`.

## Path versus host policy

Use `host[...]` for DNS labels and `path[...]` for URL path segments. Do not put slashes in host labels. Do not put untrusted strings into static path literals. Put dynamic values in route references so Concord can encode them correctly.

## Large API structure

For a large API, mirror the upstream documentation with nested scopes.

```rust
scope regional {
    params { region: RegionRoute }
    host[region, "api"]

    scope match_v5_matches {
        path["lol", "match", "v5", "matches"]

        GET GetMatchIdsByPuuid {
            params {
                puuid: String,
                start: u64 = 0,
                count: u64 = 20
            }
            path["by-puuid", puuid, "ids"]
            query {
                "start" = start,
                "count" = count
            }
            -> Json<Vec<String>>;
        }
    }
}
```

This keeps each endpoint small while still making the full route obvious.

