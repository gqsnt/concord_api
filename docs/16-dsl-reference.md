# 16. DSL Reference

Compact reference for the Concord v4 DSL.

## Root

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

## Client

```rust
client Api {
    base https "example.com"

    var tenant: String = "public".to_string()
    var trace: bool

    secret api_key: String

    credential key = api_key(secret.api_key)
    credential session = endpoint auth_api::LoginForSession

    default {
        header "user-agent" = "Api/1.0"
        retry read
        rate_limit app
        cache short
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

    cache short {
        ttl 60 seconds
    }

    observe rate_limit MyObserver
}
```

## Variables

```rust
var name: Type
var name: Type = expr
```

## Secrets

```rust
secret api_key: String
```

## Credentials

```rust
credential key = api_key(secret.api_key)
credential token = bearer(secret.access_token)
credential admin = basic(secret.username, secret.password)
credential session = endpoint auth_api::LoginForSession
```

## Scope

```rust
scope name(param: Type, opt?: Type, defaulted: Type = expr) {
    host [param, "api"]
    path ["v1"]

    auth header "X-Key" = key
    retry read
    rate_limit app
    cache short

    scope child { ... }

    GET Endpoint -> Json<T>;
}
```

## Endpoint

```rust
GET Name(required: Type, optional?: Type, defaulted: Type = expr)
    as alias
    path ["x", required]
    -> Json<T>
```

With block:

```rust
GET Name(id: u64)
    -> Json<T>
{
    path [id]

    query {
        "include" = "profile"
    }

    headers {
        "x-debug" = true
    }

    retry read
    cache short
    rate_limit method_read
}
```

With body:

```rust
POST Create(body: Json<NewItem>)
    as create
    path ["items"]
    -> Json<Item>
```

With mapping:

```rust
GET Titles(user_id: u64)
    -> Json<Vec<Post>>
    map Vec<String> {
        IntoIterator::into_iter(r).map(|p| p.title).collect()
    }
{
    path ["users", user_id, "posts"]
}
```

## Route

```rust
host [region, "api"]
path ["users", id]
path ["prefix", part["user-", id]]
```

## Headers

```rust
header "x-client" = "v4"

headers {
    "user-agent" = "Api/1.0"
    "x-debug" = debug
    -"x-old"
}
```

## Query

```rust
query "debug" = true

query {
    page
    "userId" = user_id
    "tag" += tag
    -"old"
}
```

## Timeout

```rust
timeout std::time::Duration::from_secs(10)
```

## Auth

```rust
auth bearer session
auth header "X-Api-Key" = key
auth query "api_key" = key
auth basic admin
```

## Retry

```rust
retry read

retry read {
    attempts 2
    methods [GET]
    on [429, 500]
    retry_after
}

retry off
```

## Rate limit

```rust
rate_limit app
rate_limit [app, method_read]
rate_limit only method_read
rate_limit off

rate_limit app {
    bucket application by [host] {
        500 / 10s
    }
}
```

## Cache

```rust
cache short
cache only short
cache off
cache 5m

cache short {
    ttl 60 seconds
    revalidate
    on_error serve_stale
}
```

## Pagination

```rust
paginate OffsetLimitPagination {
    offset = start
    limit = count
}

paginate PagedPagination {
    page = page as u64
    per_page = page_size as u64
}

paginate CursorPagination {
    cursor = page_cursor
    per_page = page_size
}
```

## Migration

Removed syntax and replacement examples are documented in [Migration Notes](17-migration-notes.md).
