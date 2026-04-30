# Auth

Concord auth is declared in the DSL and applied by the generated client before a request reaches transport.

## Secrets And Credentials

Secrets are client inputs. Credentials adapt those secrets into auth material.

```rust
client SessionApi {
    base "https://example.com"

    secret upstream_key: String
    secret bearer_token: String

    credential upstream = api_key(secret.upstream_key)
    credential session = bearer(secret.bearer_token)
}
```

## Auth Clauses

Attach credentials at the client, scope, or endpoint layer.

```rust
auth header "X-Upstream-Key" = upstream
auth query "api_key" = upstream
auth bearer session
```

Inherited auth applies to every endpoint below the layer where it is declared.

## Endpoint-Backed Credentials

An endpoint can produce a credential for later requests. Declare the credential as an endpoint path and map the auth endpoint response into the credential material.

```rust
client SessionApi {
    base "https://example.com"
    secret upstream_key: String

    credential upstream = api_key(secret.upstream_key)
    credential session = endpoint auth_api::LoginForSession
}

scope auth_api {
    POST LoginForSession(body: Json<LoginRequest>)
        path ["login"]
        auth header "X-Upstream-Key" = upstream
        -> Json<LoginResponse>
        map AccessToken { AccessToken::new(r.access_token) }
}

scope protected {
    auth bearer session

    GET Me
        as me
        path ["me"]
        -> Json<User>
}
```

Acquire the credential explicitly from the auth endpoint request.

```rust
api.auth_api()
    .login_for_session(LoginRequest {
        username: "ada".to_string(),
        password: "secret".to_string(),
    })
    .acquire_as_session()
    .await?;
```

Then call protected endpoints through the normal facade.

```rust
let me = api.protected().me().await?;
```

Protected calls fail before transport if a required endpoint-backed credential has not been acquired.

## Auth State

Generated auth state accessors expose explicit checks and clearing.

```rust
if api.auth_state().session().is_set().await {
    api.auth_state().session().clear().await;
}
```

## Redaction

Secret values are wrapped before storage. User-facing errors and diagnostics should identify the credential or header by name, not render raw secret values.
