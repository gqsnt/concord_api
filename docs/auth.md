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

        credential upstream = api_key(secret.upstream_key)
        credential session = bearer(secret.bearer_token)
    }
}
```

For compact examples, `secret` and `credential` may still be written directly in the client block. For larger clients, prefer grouping them under `auth { ... }`.

Secret references belong only in credential declarations. Public request-shaping expressions for headers, query parameters, routes, timeouts, rate-limit keys, and pagination assignments cannot read secrets, auth material, generated implementation locals, or secret exposure methods. Basic and OAuth2 credential declarations follow the same boundary: their secret inputs are declared as client secrets and consumed only by the credential declaration.

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

`auth certificate` is an advanced attachment form for client-certificate material. The DSL does not provide a public certificate credential constructor in v1; use endpoint-backed or runtime-provided credential material when certificate auth is needed.

OAuth2 client-credentials auth uses the `oauth2_client { ... }` credential declaration and is normally attached as bearer auth.

Declare an OAuth2 client-credentials provider as a named credential, then attach that credential with `auth bearer oauth` at the default, scope, or endpoint layer. The credential declaration owns the token URL, client id, client secret, and optional scope; those inputs stay inside the credential declaration and are not available to public request-shaping expressions.

Before the first protected request, Concord sends a token request to `token_url` using `POST`, `Authorization: Basic base64(client_id:client_secret)`, `Content-Type: application/x-www-form-urlencoded`, and a body containing `grant_type=client_credentials` plus `scope` when configured. A successful token response becomes `AccessToken` material. Protected requests then materialize `Authorization: Bearer <access_token>` only at the transport boundary.

Valid OAuth tokens are reused through the credential slot. A protected `401` invalidates the applied token generation and reacquires a token within the runtime auth retry budget before retrying the protected request. Token endpoint failures stop the protected request before it is sent. OAuth client secrets, access tokens, and refresh tokens are redacted from debug output and errors.

## Endpoint-Backed Credentials

An endpoint can produce credential material for later requests. Declare the credential as an endpoint path and return the credential material directly.

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
        -> Json<AccessToken>
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

Endpoint-backed material can be `AccessToken`, `BasicCredential`, or `ClientCertificate` when attached to the matching auth placement. For bearer auth, the endpoint should return `AccessToken` directly.

## Auth State

Generated auth-state accessors expose explicit checks and clearing.

```rust
if api.auth_state().session().is_set().await? {
    api.auth_state().session().clear().await?;
}
```

Auth-state helpers that observe runtime state are fallible. A poisoned auth-state lock returns `AuthError` instead of panicking.

Credential slots track monotonic generations, including when a slot is empty. If an older response tries to invalidate an earlier generation after newer material was acquired, the newer material is kept. An older credential acquisition completion is ignored, and a cancelled acquisition wakes waiters instead of leaving the slot permanently in flight.

## Rejection And Refresh

By default, protected requests may refresh runtime-reacquirable credentials after `401 Unauthorized` or `403 Forbidden`.

Credential refresh is bounded by the client runtime `max_auth_retries` setting.

Default rejection behavior:

| Status | Invalidate credential | Retry after refresh |
| --- | --- | --- |
| `401 Unauthorized` | yes | yes, for refreshable or runtime-reacquirable credentials |
| `403 Forbidden` | yes | yes, for refreshable or runtime-reacquirable credentials |

`AuthStepPolicy` remains a bool matrix in v1. The supported combinations are:

| retry | invalidate | Observed behavior |
| --- | --- | --- |
| `true` | `true` | Default refresh path. Concord invalidates the applied generation and retries the protected request when the credential can be reacquired. |
| `true` | `false` | Concord retries the protected request without first clearing the applied generation. |
| `false` | `true` | Concord invalidates the applied generation and returns a terminal auth rejection for the current request. |
| `false` | `false` | Concord returns a terminal auth rejection and leaves the applied generation untouched. |

Endpoint-backed credentials are manual from the protected request's point of view. A protected `401` or `403` can invalidate the applied endpoint-backed generation, but it does not automatically call the auth endpoint again. Reacquire through the auth endpoint explicitly before sending another protected call.

Normal retry policy still runs separately. Auth rejection handling happens after response classification but before the normal retry decision, so a protected `401` or `403` refresh path is tried before any ordinary retry decision.

`AuthChallengePolicy::NeverRefresh` is available in the advanced core API for runtime integrations that must never refresh on a protected response. It is not a public DSL clause in v1. With `NeverRefresh`, protected `401` and `403` responses do not invalidate, refresh, or retry.

## Redaction

Secret values are wrapped before storage. User-facing errors and diagnostics should identify the credential, header, query key, or auth usage by name, not render raw secret values.

Concord redacts secret values from debug and diagnostic output. Header values, bearer tokens, Basic auth usernames and passwords declared through `secret`, OAuth client secrets, and query-auth values are not rendered directly.

If a public query parameter already uses the same key as a query-auth credential, Concord rejects the request before transport with a typed auth configuration error. It does not append a duplicate credential query key or materialize the raw query-auth secret before reporting the collision.

Header-auth placements reserve their header name as well. A public header that collides with a bearer, Basic, or custom header-auth header is rejected after auth inheritance has been applied to the final endpoint and before transport, and header-name matching is case-insensitive.

The actual outbound request still contains the credential material required by the remote API. Redaction applies to debug output, diagnostics, and generated documentation, not to the request sent over transport.

Concord's default reqwest transport disables redirects, so bearer, basic, header, and query auth material stays on the original request instead of being forwarded to a remote-selected redirect target. Advanced callers who supply their own reqwest client own that redirect policy themselves.
