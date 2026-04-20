# 15. Authentication Evolution (Implemented Model)

This chapter records the auth model Concord now targets and the runtime/codegen shape behind it.

## Final mental model

Keep three separate concepts:

1. `secret` stores sensitive raw values.
2. `auth { credential ... }` defines credential source and lifecycle.
3. `use_auth ...` defines wire application on requests.

No collapse into a single auth abstraction.

## DSL credential sources

`credential` supports these source families:

1. Static secret-backed: `ApiKey(secret.x)`, `AccessToken(secret.y)`, `Basic(...)`
2. Provider-backed: `OAuth2ClientCredentials { ... }`, `Custom<T>(expr)`
3. Endpoint-backed manual: `Endpoint(auth::LoginEndpoint)`

Endpoint-backed credentials reuse endpoint output mapping; no auth-specific response mapping language exists.

```rust
scope auth {
    POST LoginForSession(body: Json<LoginRequest>)
    -> Json<LoginResponse> | AccessToken => {
        AccessToken::new(r.access_token)
    }
    {
        path["login"]
    }
}

client Api {
    scheme: https,
    host: "example.com",
    auth {
        credential session: Endpoint(auth::LoginForSession)
    }
}
```

## Endpoint-backed manual lifecycle

`credential x: Endpoint(scope::E)` is manual by default:

1. No implicit login call during protected requests.
2. Missing value yields `AuthErrorKind::MissingCredential`.
3. Acquisition is explicit user action.

Generated client API:

1. `pub async fn acquire_auth_<name>(&self, ep: endpoints::<scope>::<Endpoint>) -> Result<(), ApiClientError>`
2. `pub async fn set_auth_<name>_value(&self, value: Material)`
3. `pub async fn clear_auth_<name>(&self)`
4. `pub async fn has_auth_<name>(&self) -> bool`

Typical error before acquisition:

```text
missing credential `session`; call `client.acquire_auth_session(...)` first
```

## Core runtime representation

The current core model keeps `CredentialSlot` and extends it with manual lifecycle operations:

1. `set_manual(value)`
2. `clear_manual()`
3. `has_value()`
4. `get_cached()`

`ManualCredentialProvider<M>` is used for manual/endpoint-backed credentials:

1. `acquire` returns `MissingCredential` when empty.
2. Optional missing hint enriches diagnostics.
3. Slot state remains shared across client clones.

## Rejection handling: invalidation and retry are independent

`UseCredential::on_response` now evaluates:

1. response signal (`challenge`, `401`, `403`)
2. invalidation policy
3. retry policy

`AuthStepPolicy` exposes explicit controls:

1. `retry_on_challenge_rejection`
2. `invalidate_on_unauthorized`
3. `invalidate_on_forbidden`
4. `invalidate_on_challenge_rejection`

This allows manual credentials to invalidate on `401` without forcing automatic auth retry.

## Behavior matrix

1. Static secret credentials:
   - material available immediately
   - behavior unchanged by default
2. Provider credentials:
   - acquire/refresh automatically
   - rejection commonly invalidates and retries
3. Endpoint-backed manual credentials:
   - fail missing before explicit acquire
   - can be seeded/cleared explicitly
   - `401` invalidates by policy, no forced auto-retry by default

## Internal auth and recursion

Two distinct acquisition paths remain:

1. Provider-side internal HTTP (`AuthHttpExecutor`, `AuthMode`)
2. Endpoint-backed manual acquire via normal endpoint execution

Compile-time guard exists for direct self-dependency:

- if credential `c` uses `Endpoint(auth::E)` and endpoint `auth::E` uses `c`, compilation fails.

Runtime recursion protection for provider internal auth remains in place.

## Diagnostics

Compile-time diagnostics include:

1. unknown auth endpoint in credential source
2. direct endpoint/credential self-dependency
3. existing usage/credential fit checks
4. endpoint output that does not satisfy `CredentialMaterial` trait bounds (compile-time trait error)

Runtime diagnostics include:

1. clear `MissingCredential` message with `acquire_auth_*` hint for manual credentials

## Implementation plan (dependency order)

1. Core credential lifecycle and manual provider primitives
2. Core response handling split: invalidation vs retry
3. DSL AST/parser/sema support for `Endpoint(...)`
4. Codegen for manual endpoint credential provider + lifecycle helpers
5. Diagnostics hardening
6. Docs/examples/test refresh

The first four items are implemented; diagnostics hardening can continue incrementally.

## Required validation coverage

1. Static secret-backed API key flow unchanged
2. Custom provider flow unchanged
3. Endpoint-backed credential explicit acquire then use
4. Missing before acquire returns clear error
5. Clones observe acquire and clear state
6. Clear causes later protected call to fail missing
7. `401` invalidates manual credential without forced auto-retry
8. Endpoint-backed login can itself use explicit auth
9. Internal auth recursion protections remain correct
10. `one_of` semantics remain correct
