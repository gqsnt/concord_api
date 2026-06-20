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

Generated OAuth2 client-credentials support is a normal runtime credential flow. Before the first protected request, Concord sends a token request to `token_url` using `POST`, `Authorization: Basic base64(client_id:client_secret)`, `Content-Type: application/x-www-form-urlencoded`, and a body containing `grant_type=client_credentials` plus `scope` when configured. A successful token response becomes `AccessToken` material. Protected requests then materialize `Authorization: Bearer <access_token>` only at the transport boundary.

Valid OAuth tokens are reused through the credential slot. A protected `401` invalidates the applied token generation and reacquires a token within the runtime auth retry budget before retrying the protected request. Token endpoint failures stop the protected request before it is sent. `client_secret`, access tokens, and refresh tokens are redacted from debug output and errors.

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

Rejected credential refresh only applies after a credential exists and has been applied to the protected request. If the slot is empty, Concord reports the missing credential before sending the request.

Endpoint-backed material can be `AccessToken`, API-key-like secret material, `BasicCredential`, or `ClientCertificate` when attached to the matching auth placement. Endpoint-backed Basic credentials materialize as a Basic `Authorization` header at transport send time. Endpoint-backed client certificates materialize as certificate transport metadata.

## Auth State

Generated auth state accessors expose explicit checks and clearing.

```rust
if api.auth_state().session().is_set().await? {
    api.auth_state().session().clear().await?;
}
```

Auth state helpers that observe shared auth state are fallible. A poisoned auth-state lock returns `AuthError` instead of panicking.

Endpoint-backed credential slots track generations. If a stale response tries to invalidate an older generation after a newer credential was acquired, the newer credential is not cleared by that stale invalidation.

## Rejection And Refresh

By default, protected requests may refresh runtime-reacquirable credentials after `401 Unauthorized`.

`403 Forbidden` does not trigger credential refresh by default because it usually means the credential was accepted but lacks permission.

Credential refresh is bounded by the client runtime `max_auth_retries` setting. Concord will not refresh indefinitely.

Default rejection behavior:

| Status | Invalidate credential | Retry after refresh |
| --- | --- | --- |
| `401 Unauthorized` | yes | yes, for refreshable/reacquirable runtime credentials |
| `403 Forbidden` | no | no |

Endpoint-backed credentials are manual from the protected request's point of view. A protected `401` can invalidate the applied endpoint-backed generation, but it does not automatically call the auth endpoint again or retry the protected request into `MissingCredential`. Reacquire through the auth endpoint explicitly before sending another protected call.

Normal retry policy still runs separately. Auth rejection handling happens after response classification but before the normal retry decision, so a `401` refresh path is tried before any ordinary retry decision.

`AuthChallengePolicy::NeverRefresh` is available in the advanced core API for runtime integrations that must never refresh on a protected response. It is not a public DSL clause in v1.

## Redaction

Secret values are wrapped before storage. User-facing errors and diagnostics should identify the credential, header, query key, or auth usage by name, not render raw secret values.

Concord redacts secret values from debug and diagnostic output. Header values, bearer tokens, Basic auth usernames and passwords declared through `secret`, OAuth client secrets, and query-auth values are not rendered directly.

Basic auth cache/debug identities use opaque fingerprints by default. For Basic auth, the default fingerprint includes both the secret username and the secret password, without exposing either raw value. If an integration needs a readable non-secret partition label, provide an explicit identity hint from advanced credential material rather than relying on secret text.

The actual outbound request still contains the credential material required by the remote API. Redaction applies to debug/display output, diagnostics, cache/debug keys, and generated documentation, not to the request sent over transport.
