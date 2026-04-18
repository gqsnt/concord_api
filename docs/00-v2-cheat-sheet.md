# 0. V2 Cheat Sheet

This chapter is the fast entry point for Concord DSL v2.

## Canonical DSL vocabulary

- `vars { ... }`: non-secret shared values
- `secret { ... }`: sensitive inputs
- `auth { credential ... }`: credential source/lifecycle
- `use_auth ...`: wire application
- `scope name { host[...] path[...] ... }`: shared route + policy
- `part[...]`: template composition for route/header/query values

## Minimal v2 shape

```rust
api! {
    client Api {
        scheme: https,
        host: "example.com",

        vars {
            tenant: String = "public".into()
        }
        secret {
            api_key: String
        }

        auth {
            credential key: ApiKey(secret.api_key)
        }
    }

    scope v1 {
        host[vars.tenant, "api"]
        path["v1"]
        use_auth HeaderAuth("X-Api-Key", key)

        GET Me {
            path["me"]
            -> Json<User>;
        }
    }
}
```

## Endpoint-backed manual credentials

```rust
auth {
    credential session: Endpoint(LoginForSession)
}
```

```rust
api.acquire_auth_session(endpoints::LoginForSession::new(...)).await?;
api.request(endpoints::Me::new()).execute().await?;
```

## Removed in v2

- `auth_vars { ... }` -> use `secret { ... }`
- `fmt[...]` / `fmt?[...]` -> use `part[...]`
- top-level `prefix ... {}` or `path ... {}` layers -> use `scope name { host[...] path[...] ... }`
- `cx.*` and `auth.*` aliases -> use `vars.*` and `secret.*`

## Suggested reading order

1. This cheat sheet.
2. [Introduction](01-introduction.md)
3. [Client Blocks](02-client.md)
4. [Routing and Endpoints](03-routing-and-endpoints.md)
5. [Authentication](07-authentication.md)
