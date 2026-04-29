# 2. DSL Overview

This chapter summarizes the v5 DSL vocabulary.

## Top-level shape

```rust
api! {
    client Api {
        base https "example.com"
    }

    scope users {
        path ["users"]

        GET GetUser(id: u64)
            as get
            path [id]
            -> Json<User>
    }
}
```

## Client declarations

```rust
client Api {
    base https "example.com"

    var tenant: String = "public".to_string()
    secret api_key: String

    credential key = api_key(secret.api_key)

    default {
        header "user-agent" = "Api/1.0"
        retry read
        rate_limit app
    }

    retry read {
        max_attempts 2
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

## Scope declarations

```rust
scope regional(region: RegionalRoute) {
    host [region, "api"]
    path ["riot", "account", "v1"]

    GET GetAccountByPuuid(puuid: String)
        as by_puuid
        path ["accounts", "by-puuid", puuid]
        -> Json<AccountDto>
}
```

## Endpoint declarations

Simple endpoint:

```rust
GET Ping
    as ping
    path ["ping"]
    -> Json<PingResponse>
```

Endpoint with body:

```rust
POST CreatePost(body: Json<NewPost>)
    as create
    path ["posts"]
    -> Json<Post>
```

Endpoint with mapping:

```rust
GET GetUserPosts(id: i32, user_id?: u32)
    path [id, "posts"]
    query {
        "userId" = user_id
    }
    -> Json<Vec<Post>>
    map Vec<String> {
        IntoIterator::into_iter(r).map(|p| p.title).collect()
    }
```

## Policy declarations

Single line:

```rust
header "x-client" = "v5"
query "debug" = true
auth bearer session
retry read
rate_limit app
cache short
timeout std::time::Duration::from_secs(10)
```

Block form:

```rust
headers {
    "user-agent" = "Api/1.0"
    "x-client-trace" = vars.trace
}

query {
    "userId" = user_id
    page
}
```

## v5 form

Use the v5 root/client/auth form:

```rust
client Api {
    base https "example.com"
    secret api_key: String
    credential key = api_key(secret.api_key)
}

auth header "X-Api-Key" = key
```
