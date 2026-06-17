# Auth

Concord auth is declared in the DSL and applied by the generated client before a request reaches transport.

## Secrets And Credentials

Secrets are client inputs. Credentials adapt those secrets into auth material.

```rust
client SessionApi {
    base "https://example.com"

    auth {
        secret upstream_key: String
        secret bearer_token: String
        secret username: String
        secret password: String
        secret client_id: String
        secret client_secret: String

        credential upstream = api_key(secret.upstream_key)
        credential session = bearer(secret.bearer_token)
        credential login = basic(secret.username, secret.password)
        credential oauth = oauth2_client {
            token_url: "https://auth.example.com/oauth/token",
            client_id: secret.client_id,
            client_secret: secret.client_secret,
            scope: "read",
        }
    }
}
```

For compact examples, `secret` and `credential` may still be written directly in the client block. For larger clients, prefer grouping them under `auth { ... }`.

See `docs/dsl.md` for the complete public DSL reference.

## Auth Clauses

Attach credentials at the client, scope, or endpoint layer.

```rust
auth header "X-Upstream-Key" = upstream
auth query "api_key" = upstream
auth bearer session
auth basic login
auth certificate client_cert
```

Inherited auth applies to every endpoint below the layer where it is declared.

`auth certificate` is an advanced attachment form for client-certificate credential material. The DSL does not provide a `certificate(secret...)` credential constructor in v1; use endpoint-backed or runtime-provided credential material when certificate auth is needed.

OAuth2 client-credentials auth uses the `oauth2_client { ... }` credential declaration and is normally attached as bearer auth.

```rust
auth {
    secret client_id: String
    secret client_secret: String

    credential oauth = oauth2_client {
        token_url: "https://auth.example.com/oauth/token",
        client_id: secret.client_id,
        client_secret: secret.client_secret,
        scope: "read:users",
    }
}

defaults {
    auth bearer oauth
}
```

## Endpoint-Backed Credentials

An endpoint can produce a credential for later requests. Declare the credential as an endpoint path and map the auth endpoint response into the credential material.

```rust
client SessionApi {
    base "https://example.com"

    auth {
        secret upstream_key: String

        credential upstream = api_key(secret.upstream_key)
        credential session = endpoint auth_api::LoginForSession
    }

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
