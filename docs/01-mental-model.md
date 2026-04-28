# 1. Mental Model

Concord v4 is easiest to understand as an API tree.

```text
client Api
├─ scope users
│  ├─ GET list
│  └─ GET get
└─ scope posts
   ├─ GET list
   └─ POST create
```

## Root, branch, leaf

| DSL concept | Meaning |
| --- | --- |
| `client` | API root: base URL, constructor inputs, credentials, global defaults |
| `scope` | API branch: route prefix, host prefix, shared policy, child scopes/endpoints |
| endpoint | API leaf: HTTP method, params, body, response, endpoint-specific policy |

## Policies flow downward

A policy written on a client applies to every child unless changed.

A policy written on a scope applies to everything inside that scope.

A policy written on an endpoint is the most specific.

Example:

```rust
client Api {
    base https "example.com"

    default {
        retry read
    }

    retry read {
        attempts 2
        methods [GET]
        on [503]
        retry_after
    }
}

scope protected {
    auth bearer session

    GET Me
        as me
        path ["me"]
        -> Json<User>
}
```

`Me` inherits:

- base URL from the client;
- retry policy from the client default;
- bearer auth from `protected`;
- path from the endpoint.

## Route fragments compose

```rust
client RiotClient {
    base https "riotgames.com"
}

scope platform(platform: PlatformRoute) {
    host [platform, "api"]
    path ["lol"]

    scope summoner_v4 {
        path ["summoner", "v4", "summoners"]

        GET GetSummonerByPuuid(puuid: String)
            as by_puuid
            path ["by-puuid", puuid]
            -> Json<SummonerDto>
    }
}
```

For `platform = EUW1`, this builds:

```text
https://euw1.api.riotgames.com/lol/summoner/v4/summoners/by-puuid/{puuid}
```

## Generated usage mirrors the tree

The DSL:

```rust
scope jsonplaceholder {
    scope posts {
        GET GetPost(id: i32)
            as get_post
            path [id]
            -> Json<Post>
    }
}
```

The usage:

```rust
api.jsonplaceholder()
   .posts()
   .get_post(1)
   .await?;
```

## Explicit endpoint structs still exist

The facade is the normal DX.

The explicit endpoint API is useful for generic code:

```rust
let endpoint = client::endpoints::jsonplaceholder::posts::GetPost::new(1);

api.request(endpoint)
    .execute()
    .await?;
```

## Runtime is plan-based

Generated endpoints produce a request plan.

The runtime executes that plan through a fixed pipeline:

```text
build request
prepare auth
cache before-send
rate-limit acquire
send
rate-limit observe
auth response handling
cache after-response
retry decision
decode
```

You normally do not interact with request-plan internals directly. They matter because extension points plug into the runtime without changing the DSL.
