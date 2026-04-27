# 7. Authentication

Concord v4 separates two concepts:

| Concept | Meaning |
| --- | --- |
| credential | where secret material comes from |
| auth placement | how that material is applied to a request |

## Credential declarations

Credentials live in the client block.

```rust
client Api {
    base https "example.com"

    secret api_key: String
    secret username: String
    secret password: String

    credential key = api_key(secret.api_key)
    credential bearer = bearer(secret.api_key)
    credential admin = basic(secret.username, secret.password)
}
```

Supported built-in credential sources:

```rust
credential key = api_key(secret.api_key)
credential token = bearer(secret.access_token)
credential admin = basic(secret.username, secret.password)
credential session = endpoint auth_api::LoginForSession
```

OAuth2 client credentials may be available when the relevant feature is enabled.

## Applying auth

Apply credentials with `auth`.

```rust
auth header "X-Api-Key" = key
auth bearer session
auth query "api_key" = key
auth basic admin
```

Auth can be written at client, scope, or endpoint level.

```rust
scope protected {
    auth bearer session

    GET Me
        as me
        path ["me"]
        -> Json<User>
}
```

## Multiple auth lines

Multiple auth lines mean all listed requirements apply.

```rust
auth header "X-Api-Key" = key
auth bearer session
```

v4 does not document `auth any` / `auth all` groups as stable user-facing syntax.

## Header auth

```rust
credential riot = api_key(secret.api_key)

scope platform(platform: PlatformRoute) {
    auth header "X-Riot-Token" = riot
}
```

## Bearer auth

```rust
credential session = endpoint auth_api::LoginForSession

scope protected {
    auth bearer session
}
```

## Query auth

```rust
credential key = api_key(secret.api_key)

scope legacy {
    auth query "api_key" = key
}
```

Use query auth only for APIs that require it.

## Basic auth

```rust
credential admin = basic(secret.username, secret.password)

scope admin {
    auth basic admin
}
```

## Endpoint-backed session credentials

Endpoint-backed credentials are manual. Concord does not login automatically.

DSL:

```rust
client SessionApi {
    base https "example.com"

    secret upstream_key: String

    credential upstream = api_key(secret.upstream_key)
    credential session = endpoint auth_api::LoginForSession
}

scope auth_api {
    POST LoginForSession(body: Json<LoginRequest>)
        -> Json<LoginResponse>
        map AccessToken {
            AccessToken::new(r.access_token)
        }
    {
        path ["login"]
        auth header "X-Upstream-Key" = upstream
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

Usage:

```rust
let api = session_api::SessionApi::new("upstream-key".to_string());

api.auth_state()
    .session()
    .acquire(api.auth_api().login_for_session(LoginRequest {
        username: "alice".to_string(),
        password: "secret".to_string(),
    }))
    .await?;

let me = api.protected().me().await?;

api.auth_state().session().clear().await;
```

## Auth retry behavior

When a request fails because a credential is rejected, Concord can invalidate the exact credential generation used by the request and retry within the configured auth retry limit.

This behavior is internal to the runtime. Users usually only need to:

1. declare the credential;
2. apply it with `auth`;
3. acquire manual credentials when needed.

## Extension points

Credential providers are extension points under `concord_core::advanced`.

Use custom providers for new ways of obtaining credential material.

Custom auth placements are not documented as stable v4. Prefer built-in placements:

- bearer;
- header;
- query;
- basic;
- certificate if supported by your transport/runtime.
