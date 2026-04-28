# 5. Generated Client Usage

Concord v4 generates two usage layers:

1. the tree facade;
2. explicit endpoint structs.

Use the facade by default.

## Tree facade

For this DSL:

```rust
scope jsonplaceholder {
    scope posts {
        path ["posts"]

        GET GetPost(id: i32)
            as get_post
            path [id]
            -> Json<Post>
    }
}
```

Use:

```rust
let post = api
    .jsonplaceholder()
    .posts()
    .get_post(1)
    .await?;
```

Facade calls return a pending request. Because pending requests implement `IntoFuture`, you can `.await` them directly.

## Request customization

Facade calls can still be configured before awaiting:

```rust
let posts = api
    .jsonplaceholder()
    .posts()
    .get_posts()
    .debug_level(DebugLevel::VV)
    .timeout(std::time::Duration::from_secs(5))
    .await?;
```

Common request modifiers include:

```rust
.debug_level(DebugLevel::V)
.timeout(duration)
.clear_timeout()
.inherit_timeout()
.cache_default()
.cache_bypass()
.cache_refresh()
.paginate()
```

## Explicit endpoint API

Every endpoint also has an explicit struct under `endpoints`.

```rust
let endpoint = client::endpoints::jsonplaceholder::posts::GetPost::new(1);

let post = api.request(endpoint)
    .execute()
    .await?;
```

This is useful for:

- generic code;
- tests;
- storing endpoint values;
- building requests conditionally.

The facade is better for normal application code.

## `execute` and `execute_decoded`

`.execute()` returns the endpoint output:

```rust
let user: User = api.users().get(42).execute().await?;
```

Because `PendingRequest` is awaitable, this is equivalent:

```rust
let user: User = api.users().get(42).await?;
```

`.execute_decoded()` returns metadata plus the decoded value:

```rust
let response = api.users().get(42).execute_decoded().await?;

println!("status = {}", response.status);
println!("url = {}", response.url);
println!("value = {:?}", response.value);
```

Use `execute_decoded()` when you need status, headers, URL, or request metadata.

## Pagination

Paginated endpoints support:

```rust
let items = api
    .regional(region)
    .match_v5_matches()
    .ids_by_puuid(puuid)
    .paginate()
    .max_items(10_000)
    .collect()
    .await?;
```

A paginated endpoint can still be executed once:

```rust
let first_page = api
    .regional(region)
    .match_v5_matches()
    .ids_by_puuid(puuid)
    .await?;
```

## Auth state facade

Endpoint-backed credentials can be acquired from the login request itself:

```rust
api.auth_api()
    .login_for_session(LoginRequest {
        username: "alice".to_string(),
        password: "secret".to_string(),
    })
    .acquire_as_session()
    .await?;

api.auth_state().session().clear().await;
```

The explicit auth-state API remains available for advanced flows.

## Constructor examples

No required vars/secrets:

```rust
let api = users_api::UsersApi::new();
```

Required secret:

```rust
let api = session_api::SessionApi::new("upstream-key".to_string());
```

Test transport:

```rust
let api = users_api::UsersApi::new_with_transport(mock_transport);
```

Required var and secret:

```rust
let api = api::Api::new(trace_enabled, api_key);
```
