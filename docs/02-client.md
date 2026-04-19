# 2. Client Blocks

A client block is the root of an API definition. It names the generated client, sets the base URL, declares shared runtime values, and defines default policy profiles.

```rust
api! {
    client ExampleApi {
        scheme: https,
        host: "example.com",

        vars {
            tenant: String,
            client_trace: bool = false
        }

        secret {
            api_key: String
        }

        headers {
            "user-agent" = "ExampleApi/1.0",
            "x-client-trace" = vars.client_trace
        }
    }
}
```

## Scheme and host

`scheme` is either `http` or `https`.

`host` is the base domain. Scopes can prepend labels with `host[...]`.

```rust
client RiotClient {
    scheme: https,
    host: "riotgames.com",
}

scope platform(platform: PlatformRoute) {
    host[platform, "api"]
    path["lol"]
}
```

That pattern builds hosts like `euw1.api.riotgames.com`, depending on the endpoint value for `platform`.

## Client variables

`vars { ... }` declares non-secret values shared by all endpoints. Use them for host labels, default headers, feature toggles, tenant IDs, locale, or any value that is not a credential.

```rust
vars {
    subdomain: String = "jsonplaceholder".to_string(),
    client_trace: bool
}

headers {
    "x-client-trace" = vars.client_trace
}
```

Required vars become constructor arguments. Defaulted vars do not.

```rust
let api = client::Client::new(true);
api.set_subdomain("staging".to_string());
```

The generated client also exposes lower-level accessors through the core client shape, such as `vars()`, `vars_mut()`, `set_vars(...)`, and `update_vars(...)`.

## Secrets

`secret { ... }` declares sensitive values. It is the DSL-facing form for auth variables. Secret values are stored as `SecretString` internally.

```rust
secret {
    api_key: String,
    username: String,
    password: String
}
```

Required secrets become constructor arguments. Generated setter methods update the secret and rebuild auth state where needed.

```rust
let mut api = riot_client::RiotClient::new(api_key);
api.set_api_key("rotated-key");
```

Use `secret` values for credentials only. Use `vars` for ordinary configuration.

For endpoint-backed manual credentials, generated clients also expose async lifecycle helpers:

```rust
api.acquire_auth_session(endpoints::LoginForSession::new(...)).await?;
api.set_auth_session_value(AccessToken::new("seed")).await;
let has = api.has_auth_session().await;
api.clear_auth_session().await;
```

`acquire_auth_*` performs network I/O. `set/has/clear` only manipulate shared credential state.

## Global policy blocks

A client can define default headers, query parameters, timeout, retry profiles, rate-limit profiles, cache profiles, and authentication.

```rust
client ExampleApi {
    scheme: https,
    host: "example.com",

    headers {
        "user-agent" = "ExampleApi/1.0"
    }

    query {
        "sdk" = "concord"
    }

    timeout: core::time::Duration::from_secs(30)

    retry {
        profile read {
            attempts 2
            methods [GET, HEAD]
            on status[429, 500, 502, 503, 504]
            retry_after honor
            backoff none
        }
        default read
    }
}
```

These policies are inherited by scopes and endpoints unless overridden.

## Generated constructors

For a client with no required vars or secrets, the constructor takes no arguments.

```rust
let api = users_api::UsersApi::new();
```

For required vars and secrets, constructor arguments follow the generated client shape. In examples:

```rust
client Client {
    vars {
        subdomain: String = "jsonplaceholder".to_string(),
        client_trace: bool
    }
}
```

The defaulted `subdomain` is omitted, while required `client_trace` is passed:

```rust
let api = client::Client::new(true);
```

For tests and custom transports, use `new_with_transport` on the generated client when available.

```rust
let api = users_api::UsersApi::new_with_transport(mock_transport);
```

## Runtime configuration

The generated client wraps `concord_core::ApiClient` behavior. The generated wrapper exposes the common runtime knobs:

- `with_debug_level(DebugLevel::V)` and `set_debug_level(...)`
- `with_rate_limiter(Arc<dyn RateLimiter>)` and `set_rate_limiter(...)`
- `with_cache_store(Arc<dyn CacheStore>)` and `set_cache_store(...)`
- `with_inflight_policy(Arc<dyn InflightPolicy>)` and `set_inflight_policy(...)`
- `with_pagination_caps(Caps::default().max_pages(10))`

The lower-level `concord_core::ApiClient` also has hooks for debug sinks, runtime hooks, retry policy, and auth retry budget. Those are core APIs; generated wrappers only forward the common knobs listed above.

Builder-style `with_*` methods are convenient when constructing a client. `set_*` methods are useful when mutating an existing client.

```rust
let api = users_api::UsersApi::new()
    .with_debug_level(DebugLevel::V)
    .with_pagination_caps(Caps::default().max_pages(20));
```

## Feature notes

The `Json<T>` codec is exported from the prelude when `concord_core` is built with the `json` feature.

The built-in weighted cache store is exported as `MokaCacheStore` when `concord_core` is built with the `cache-moka` feature.

The governor-based rate limiter is available with the `rate-limit-governor` feature, which is part of `concord_core` default features.
