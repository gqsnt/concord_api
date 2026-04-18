# 7. Authentication

Authentication is described in two steps:

1. Declare credential providers in `auth { ... }`.
2. Apply credentials with `use_auth ...` at the client, scope, or endpoint level.

Secrets are declared separately in `secret { ... }`.

```rust
api! {
    client ApiDslHeader {
        scheme: https,
        host: "example.com",

        secret {
            api_key: String
        }

        auth {
            credential api_key: ApiKey(secret.api_key)
        }
    }

    scope protected {
        use_auth HeaderAuth("X-Api-Key", api_key)
        path["api"]

        GET Ping {
            -> Json<()>;
        }
    }
}
```

The generated client receives the required secret and applies it to each protected request.

```rust
let mut api = api_dsl_header::ApiDslHeader::new("tok1".to_string());
api.request(api_dsl_header::endpoints::Ping::new()).execute().await?;

api.set_api_key("tok2");
api.request(api_dsl_header::endpoints::Ping::new()).execute().await?;
```

Secret setters rebuild auth state, and clones observe the updated state.

## Credential declarations

Credential declarations live inside the client `auth` block.

```rust
auth {
    credential api_key: ApiKey(secret.api_key)
    credential token: BearerToken(secret.access_token)
    credential basic: Basic(secret.username, secret.password)
}
```

Supported credential forms include:

- `ApiKey(secret.name)`
- `BearerToken(secret.name)`
- `AccessToken(secret.name)`
- `Basic(secret.username, secret.password)`
- `OAuth2ClientCredentials { ... }`
- `Endpoint(LoginEndpoint)`
- `Custom<ProviderType>(provider_expr)`

Credential names are local identifiers referenced by `use_auth`.

## Endpoint-backed manual credentials

Use `Endpoint(...)` when a credential must be acquired explicitly from an API endpoint response.

```rust
POST LoginForSession {
    path["login"]
    body Json<LoginRequest>
    -> Json<LoginResponse> | AccessToken => {
        AccessToken::new(r.access_token)
    };
}

client Api {
    scheme: https,
    host: "example.com",
    auth {
        credential session: Endpoint(LoginForSession)
    }
}

GET Me {
    use_auth BearerAuth(session)
    -> Json<User>;
}
```

Endpoint-backed credentials are manual by default:

1. Concord does not auto-call the login endpoint.
2. Using the credential before acquisition fails with `AuthErrorKind::MissingCredential`.
3. The generated client exposes async lifecycle helpers.

```rust
api.acquire_auth_session(endpoints::LoginForSession::new(...)).await?;
api.set_auth_session_value(AccessToken::new("seed")).await;
let has = api.has_auth_session().await;
api.clear_auth_session().await;
```

Typical runtime error before acquisition:

```text
missing credential `session`; call `client.acquire_auth_session(...)` first
```

The login endpoint output type (after optional response mapping) must implement `CredentialMaterial`.
The login endpoint can itself use `use_auth` when explicit upstream auth is required.
See `concord_examples/src/auth_session.rs` for a complete end-to-end example.

## Applying auth

Auth usage declares how a credential is applied to a request.

```rust
use_auth BearerAuth(token)
use_auth HeaderAuth("X-Api-Key", api_key)
use_auth QueryAuth("api_key", api_key)
use_auth BasicAuth(basic)
use_auth CertificateAuth(cert)
```

Auth can be applied at the client level, scope level, or endpoint level. Use scope-level auth for protected API families.

```rust
scope platform {
    use_auth HeaderAuth("X-Riot-Token", riot_api_key)
    path["lol"]

    GET GetPlatformData {
        path["status", "v4", "platform-data"]
        -> Json<PlatformDataDto>;
    }
}
```

## Header auth

Header auth writes a credential into a named header.

```rust
secret {
    api_key: String
}

auth {
    credential api_key: ApiKey(secret.api_key)
}

GET Ping {
    use_auth HeaderAuth("X-Api-Key", api_key)
    -> Json<()>;
}
```

The resulting request contains:

```text
x-api-key: <secret>
```

## Bearer auth

Bearer auth writes the credential into `Authorization`.

```rust
auth {
    credential token: AccessToken(secret.access_token)
}

GET Ping {
    use_auth BearerAuth(token)
    -> Json<()>;
}
```

The resulting request contains:

```text
authorization: Bearer <token>
```

## Query auth

Query auth writes a credential into the URL query string.

```rust
GET Ping {
    use_auth QueryAuth("api_key", api_key)
    -> Json<()>;
}
```

This sends `?api_key=<secret>`. Prefer header-based auth when the upstream API supports it, because query credentials are more likely to be logged by intermediaries.

## Multiple auth steps

Use a list to apply all steps in order.

```rust
GET Ping {
    use_auth [
        BearerAuth(token),
        HeaderAuth("X-Api-Key", api_key)
    ]
    -> Json<()>;
}
```

The request receives both auth artifacts.

## One-of auth fallback

Use `one_of` for fallback auth. Concord tries the first usage and, if the response challenges or rejects it, retries with the next usage.

```rust
GET Ping {
    use_auth one_of [
        BearerAuth(token),
        HeaderAuth("X-Fallback-Key", fallback)
    ]
    -> Json<()>;
}
```

In the tests, a `401 Unauthorized` response with a bearer challenge causes the second request to use the fallback header and omit the failed bearer token.

## OAuth2 client credentials

OAuth2 client credentials are declared as a credential provider.

```rust
secret {
    client_id: String,
    client_secret: String
}

auth {
    credential token: OAuth2ClientCredentials {
        token_url: "https://auth.example.com/token",
        client_id: secret.client_id,
        client_secret: secret.client_secret,
        scope: "read"
    }
}

GET Ping {
    use_auth BearerAuth(token)
    -> Json<()>;
}
```

The provider sends an internal token request, then applies the returned access token as bearer auth.

The tested token request uses:

```text
POST https://auth.example.com/token
Authorization: Basic <base64 client_id:client_secret>
Content-Type: application/x-www-form-urlencoded

grant_type=client_credentials&scope=read
```

## Custom credential providers

Use `Custom<T>(expr)` when built-in credential providers are not enough.

```rust
auth {
    credential token: Custom<DslStaticTokenProvider>(DslStaticTokenProvider)
}

GET Ping {
    use_auth BearerAuth(token)
    -> Json<()>;
}
```

The custom provider implements the core credential provider traits from `concord_core::prelude`.

A custom provider can perform internal HTTP requests through `CredentialContext.executor`. Tests use this to implement a login flow that posts a form to `/login`, receives an access token, then applies bearer auth to the original request.

## Custom auth usage

Use `Custom<UsageType>(usage_expr, credential)` when the credential is valid but the wire format is custom.

```rust
auth {
    credential token: Custom<DslStaticTokenProvider>(DslStaticTokenProvider)
}

GET Ping {
    use_auth Custom<DslFormattingBearerAuth>(DslFormattingBearerAuth::new("tenant-a:"), token)
    -> Json<()>;
}
```

The test formats the token before applying it:

```text
authorization: Bearer tenant-a:macro-token
```

## Certificate auth

Certificate auth is supported as a usage form.

```rust
auth {
    credential cert: Custom<DslCertificateProvider>(DslCertificateProvider)
}

GET Ping {
    use_auth CertificateAuth(cert)
    -> Json<()>;
}
```

The exact transport behavior depends on the transport and certificate integration.

## Auth response handling

Auth runs before cache lookup, so authenticated cache keys can include the auth identity.

Auth response handling happens before cache storage. This prevents Concord from storing a response that will trigger an auth retry.

Auth invalidation and auth retry are now independent decisions. This matters for manual endpoint-backed credentials: a `401` can invalidate local auth state without forcing an automatic retry.

Auth retries are capped by the runtime max auth retry budget in the core client. The generated wrapper uses the core default; expose or configure the lower-level `ApiClient` directly if an integration needs to tune that budget.

## Practical guidance

Use `secret` plus `auth` for credentials. Do not build auth headers manually unless the upstream API is unusual.

Put `use_auth` at the narrowest level that matches the API. Client-level auth is fine for APIs where every endpoint is protected. Scope-level auth is usually clearer for mixed public and private APIs.

Use `one_of` only when the API genuinely accepts multiple alternatives. It causes additional requests when fallback is needed.
