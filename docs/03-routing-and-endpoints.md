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

            GET GetPost(id: i32) -> Json<Post> {
                path[id]
            }
        }
    }
}
```

`endpoints::jsonplaceholder::posts::GetPost::new(1)` targets:

```text
https://jsonplaceholder.typicode.com/posts/1
```

## Scopes

A `scope` groups child scopes and endpoints.

```rust
scope platform(platform: PlatformRoute) {
    host[platform, "api"]
    path["lol"]

    scope summoner {
        path["summoner", "summoners"]

        GET GetSummonerByPuuid(puuid: String) -> Json<SummonerDto> {
            path["by-puuid", puuid]
        }
    }
}
```

The generated endpoint constructor includes required parameters from parent scopes and the endpoint. The generated `endpoints` module mirrors the scope tree, so a call can keep the same structure:

```rust
api.request(endpoints::platform::summoner::GetSummonerByPuuid::new(
        PlatformRoute::Euw1,
        "abc".to_string(),
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

scope platform(platform: PlatformRoute) {
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

    GET GetPostComments(post_id: i32) -> Json<Vec<Comment>> {
        path[post_id, "comments"]
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
GET One(opt?: String) -> Json<()> {
    path["x", opt, "y"]
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
GET One(v: String) -> Json<()> {
    path["x", part["p", v]]
}
```

If `v` is `a/b`, the final path is `/x/pa%2Fb`. The slash is encoded because `part[...]` still produces one path segment.

When a `part[...]` references an optional value and that value is missing, the containing route item or policy value is omitted.

```rust
GET One(v?: String) -> Json<()> {
    path["x", part["p", v], "y"]
}
```

`One::new()` builds `/x/y`. `One::new().v("z".to_string())` builds `/x/pz/y`.

## Endpoint shape

The canonical endpoint form makes the contract visible in the header: required and optional inputs first, response second, detail block after.

```rust
GET GetUser(id: u32) -> Json<User> {
    path[id]
    headers { "x-debug" = true }
    query { "include" = "profile" }
}
```

The method is an HTTP method identifier such as `GET`, `POST`, `PUT`, `DELETE`, `PATCH`, `HEAD`, or `OPTIONS`.

The endpoint name becomes a generated Rust type under `endpoints`. Nested scopes become nested endpoint modules; scoped endpoints are not reexported at the root.

```rust
let ep = endpoints::users::GetUser::new(42);
let user = api.request(ep).execute().await?;
```

## Endpoint route inheritance

Routes are concatenated from outer scopes to inner scopes to endpoint.

```rust
scope api {
    path["api"]

    scope public {
        path["public"]

        GET GetUser(id: u32) -> Json<User> {
            path["users", id]
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
scope regional(region: RegionRoute) {
    host[region, "api"]

    scope match_v5_matches {
        path["lol", "match", "matches"]

        GET GetMatchIdsByPuuid(
            puuid: String,
            start: u64 = 0,
            count: u64 = 20
        ) -> Json<Vec<String>> {
            path["by-puuid", puuid, "ids"]
            query {
                "start" = start,
                "count" = count
            }
        }
    }
}
```

This keeps each endpoint small while still making the full route obvious.

