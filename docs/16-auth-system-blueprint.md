# 16. Authentication System Blueprint

This is the global auth design reference for Concord: DSL UX, runtime behavior, and extension boundaries.

## Product requirements (from scratch)

Auth must support all of these without changing mental model:

1. static secrets (`ApiKey`, bearer token, basic auth)
2. provider-managed credentials (OAuth2, custom provider logic)
3. manually seeded credentials
4. manually acquired endpoint-backed credentials
5. explicit missing-credential failures
6. composed auth application (`all` and `one_of`)
7. shared auth state across client clones
8. auth-aware cache identity behavior
9. internal auth HTTP with recursion protection

## UX contract

Author intent should stay explicit and visible:

1. `secret` answers "what sensitive inputs exist?"
2. `credential` answers "where does auth material come from and how is it managed?"
3. `use_auth` answers "how is credential material applied on the wire?"

Manual/session login must never be hidden behind request preparation. If login is needed, user code calls an explicit helper (`acquire_auth_*`).

## DSL blueprint

### 1) `secret`

- sensitive constructor/setter inputs only
- no request-application semantics

### 2) `credential`

Credential source taxonomy:

1. static secret-backed: `ApiKey(secret.x)`, `AccessToken(secret.y)`, `Basic(...)`
2. provider-backed: `OAuth2ClientCredentials { ... }`, `Custom<T>(expr)`
3. endpoint-backed manual: `Endpoint(LoginEndpoint)`

Endpoint-backed credentials reuse endpoint output mapping:

```rust
-> Json<LoginResponse> | AccessToken => { AccessToken::new(r.access_token) };
```

No second mapping language should be added inside `auth`.

### 3) `use_auth`

- wire-only concern (`BearerAuth`, `HeaderAuth`, `QueryAuth`, `BasicAuth`, `CertificateAuth`, custom usage)
- composable by inheritance and `one_of`

## Generated client contract

For endpoint-backed/manual credentials:

1. `acquire_auth_<name>(endpoint)` for network acquisition
2. `set_auth_<name>_value(value)` for manual local seeding
3. `clear_auth_<name>()` for local invalidation
4. `has_auth_<name>() -> bool` for local inspection

Naming rule:

1. `acquire_*` implies I/O
2. `set/clear/has_*` are local state operations

## Core runtime blueprint

### Credential state store

Each named credential has a shared slot storing:

1. current value presence
2. generation counter
3. refresh-in-flight coordination
4. failure cooldown metadata

Slots are shared across clones via `Arc`.

### Acquisition strategy

Credential providers define acquire/refresh/invalidate behavior:

1. static providers return immediately
2. provider-backed flows can auto-acquire/refresh
3. manual provider returns `MissingCredential` until value is seeded/acquired

### Application strategy

Auth usage implementations apply material to request and publish safe auth identity fragments for cache keys.

### Response decision model

Response handling must treat invalidation and retry as independent decisions.

Current policy surface supports that split through `AuthStepPolicy` flags, enabling:

1. invalidate + retry (typical provider case)
2. invalidate only (typical manual/session case)
3. continue (no auth action)

## Request pipeline contract

For each attempt:

1. prepare auth
2. run cache before-send
3. send transport
4. auth response processing (invalidation/retry decision)
5. cache after-response only when auth accepts the response

Auth retry loop is distinct from transport retry loop.

## Endpoint-backed manual acquisition contract

`acquire_auth_<name>(ep)` uses normal endpoint execution:

1. endpoint policies apply normally
2. endpoint can use `use_auth` explicitly
3. no implicit `SkipAuth` is forced

Direct self-dependency (`credential c: Endpoint(E)` where `E` uses `c`) must fail at compile-time.

## Behavior matrix

1. static secret-backed:
   - immediate availability
   - unchanged behavior by default
2. provider-backed:
   - auto acquire/refresh
   - rejection usually invalidates + retries
3. endpoint-backed manual:
   - missing until explicit acquire/seed
   - clear missing diagnostics
   - `401` can invalidate stored state without mandatory retry

## Diagnostics blueprint

Compile-time diagnostics should prioritize:

1. unknown credential reference in `use_auth`
2. unknown endpoint in `credential x: Endpoint(E)`
3. direct recursive credential/endpoint auth dependency
4. endpoint output not satisfying `CredentialMaterial` bounds
5. usage/material incompatibility

Runtime diagnostics should prioritize:

1. explicit missing-credential message pointing to `acquire_auth_*`
2. clear acquire/refresh failures from providers

## Extensibility boundaries

Stable extension points:

1. acquisition: `CredentialProvider`
2. wire formatting/challenge behavior: `AuthUsage`
3. material traits: `CredentialMaterial`, `SecretCredential`
4. provider-side internal HTTP: `AuthHttpExecutor`, `AuthMode`

Guardrails:

1. DSL expresses contract graph; core executes state machine.
2. wrapper API exposes ergonomic lifecycle calls, not internal policy internals.
3. avoid duplicating endpoint features inside auth.

## Security and operations notes

1. credentials use redacted secret wrappers for display/debug paths
2. cache identity uses safe material identity fragments, not raw secrets
3. manual credential invalidation should be cheap and immediate
4. auth retry budget should remain configurable at core level

## Migration plan to the target auth version

### Phase A (done)

1. manual credential provider + slot lifecycle operations
2. invalidation/retry decoupling in auth response handling

### Phase B (done)

1. DSL support for `credential x: Endpoint(E)`
2. generated `acquire/set/clear/has` helpers

### Phase C (in progress)

1. docs/examples alignment across all chapters
2. compile-fail diagnostics hardening and coverage expansion

### Phase D (optional future)

1. explicit refresh strategies for endpoint credentials
2. persistent credential stores
3. per-credential DSL policy overrides for response handling

## Test blueprint

Minimum system tests:

1. static secret-backed flow unchanged
2. custom/provider flow unchanged
3. endpoint manual acquire/use/clear flow
4. missing before acquire error clarity
5. clone-shared state for acquire/clear
6. unauthorized invalidates manual credential without forced retry
7. endpoint-backed login endpoint can itself require auth
8. internal auth recursion guard behavior remains correct
9. `one_of` behavior remains correct with mixed credential lifecycles
