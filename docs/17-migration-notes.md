# 17. Migration Notes

This page explains how to read older Concord documentation and translate it to v4.

## Client root

Old:

```rust
client Api {
    scheme: https,
    host: "example.com",
}
```

v4:

```rust
client Api {
    base https "example.com"
}
```

## Secret

Old:

```rust
secret {
    api_key: String
}
```

v4:

```rust
secret api_key: String
```

## Credential declaration

Old:

```rust
auth {
    credential key: ApiKey(secret.api_key)
}
```

v4:

```rust
credential key = api_key(secret.api_key)
```

## Auth usage

Old:

```rust
use_auth HeaderAuth("X-Api-Key", key)
use_auth BearerAuth(session)
```

v4:

```rust
auth header "X-Api-Key" = key
auth bearer session
```

## Endpoint-backed credential acquisition

Old:

```rust
api.acquire_auth_session(endpoints::auth::LoginForSession::new(...)).await?;
api.clear_auth_session().await;
```

v4:

```rust
api.auth_state()
   .session()
   .acquire(api.auth_api().login_for_session(...))
   .await?;

api.auth_state().session().clear().await;
```

## Retry

Old:

```rust
retry {
    profile read {
        attempts 2
        methods [GET]
        on status[429, 500]
        retry_after honor
        backoff none
    }
    default read
}
```

v4:

```rust
default {
    retry read
}

retry read {
    attempts 2
    methods [GET]
    on [429, 500]
    retry_after
}
```

## Rate limit

Old:

```rust
rate_limit {
    response custom RiotRateLimitHeaders

    profile app {
        bucket application by [route.host] {
            limit 500 every 10 seconds
        }
    }

    default app
}
```

v4:

```rust
observe rate_limit RiotRateLimitHeaders

default {
    rate_limit app
}

rate_limit app {
    bucket application by [host] {
        500 / 10s
    }
}
```

## Rate-limit observer

Old code often used low-level target selection.

v4 observer:

```rust
impl RateLimitObserver for RiotRateLimitHeaders {
    fn observe(&self, ctx: RateLimitResponseContext<'_>) -> RateLimitObservation {
        ctx.on_429()
            .scope_header("x-rate-limit-type")
            .retry_after()
    }
}
```

## Cache

Old docs may mention storage tuning in the DSL:

```rust
capacity 64 mib
max_body 2 mib
shared false
```

v4 docs treat storage tuning as runtime/backend configuration.

DSL cache policy focuses on semantic behavior:

```rust
cache static_data {
    http
    ttl 1h
    revalidate
    on_error serve_stale
}
```

## Mapping

Old:

```rust
GET Titles -> Json<Vec<Post>> | Vec<String> => {
    ...
} {
    path ["posts"]
}
```

v4:

```rust
GET Titles
    -> Json<Vec<Post>>
    map Vec<String> {
        ...
    }
{
    path ["posts"]
}
```

## Generated usage

Old docs often show:

```rust
api.request(endpoints::users::GetUser::new(42))
    .execute()
    .await?;
```

This still works.

v4 teaches the tree facade first:

```rust
api.users().get_user(42).await?;
```

## Unsupported or intentionally omitted

v4 documentation does not present these as stable user-facing features:

- custom auth placement in the DSL;
- `auth any` / `auth all`;
- `backoff none`;
- storage cache knobs in DSL;
- old rate-limit target machinery.
