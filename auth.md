# Auth Architecture

## Purpose

This document defines a stronger target design for real authentication in Concord.

The auth system must support:
- static API keys,
- static username/password credentials,
- access tokens,
- refresh tokens,
- first-request login or token acquisition,
- OAuth2 client credentials and refresh flows,
- custom login flows,
- request signing,
- mTLS/client certificate auth,
- multiple auth requirements on one endpoint,
- inherited auth from the DSL scope tree,
- user-defined auth providers and user-defined auth request application.

The core rule is:

Auth material is not auth usage.

Examples:
- Material: API key, username/password, access token, refresh token, certificate identity, signing key.
- Usage: Bearer auth header, Basic auth header, custom header, query parameter, mTLS transport identity, request signature.

A token is something we have. `BearerAuth` is one way to use it.

## Current ProblemsBearer

The current runtime has a single global hook:

```rust
pub trait AuthProvider: Send + Sync + 'static {
    fn prepare_request<'a>(
        &'a self,
        ctx: AuthPrepareContext<'a>,
    ) -> AuthFuture<'a, Result<(), ApiClientError>>;

    fn on_response<'a>(&'a self, ctx: AuthResponseContext<'a>) -> AuthFuture<'a, ()>;
}
```

This is useful as a low-level escape hatch, but it is not solid enough as the main model.

Problems:
- It does not know the endpoint auth contract generated from the DSL.
- It does not know which scope layer introduced auth.
- It cannot express deterministic `client -> scope -> endpoint` auth composition.
- It has no typed equivalent of `PaginationPart`.
- It cannot cleanly express multiple independent credentials.
- It cannot cleanly express alternatives such as `one_of`.
- `on_response` returns `()`, so it cannot request an auth retry after `401`.
- It does not store per-attempt auth state, so it cannot safely know which credential generation was used.
- It does not provide single-flight acquire/refresh.
- It encourages dynamic auth state to live in `vars` or `secret`, which is wrong for cloned clients.
- It does not define how internal login/refresh HTTP requests avoid recursive auth.
- It does not define safe cache/inflight key behavior for authenticated requests.
- It cannot model mTLS correctly, because certificates are transport configuration, not header mutation.

The first `auth.md` draft also had weak points:
- `AuthApplicator<Material = T>` with an associated material type is clean for one step, but awkward for heterogeneous lists.
- A generic `CredentialProvider` alone is not enough; the runtime also needs a single-flight slot around it.
- A single provider doing acquire, refresh, apply, response handling, and retry would become too large.
- A graph-like auth model must be constrained. Request application should remain deterministic and linear by layer. Credential dependencies can form a DAG, but request mutation should not become an arbitrary graph.

## Design Goals

## 1. Auth Must Be an Endpoint Part Like Pagination

Pagination has this shape:

```rust
pub trait PaginationPart<Cx: ClientContext, E: Endpoint<Cx>>: Send + Sync + 'static {
    type Ctrl: Controller<Cx, E>;
    fn controller(vars: &Cx::Vars, ep: &E) -> Result<Self::Ctrl, ApiClientError>;
}
```

Auth should follow the same idea.

Generated code declares the endpoint auth contract. Runtime controllers execute it.

Target shape:

```rust
pub trait AuthPart<Cx: ClientContext, E: Endpoint<Cx>>: Send + Sync + 'static {
    type Ctrl: AuthController<Cx, E>;

    fn controller(
        ctx: AuthBuildContext<'_, Cx>,
        ep: &E,
    ) -> Result<Self::Ctrl, ApiClientError>;
}
```

`AuthBuildContext` should contain:
- `vars`,
- `secret` or the current transitional `auth_vars`,
- shared runtime auth registry,
- static client metadata.

Do not pass a concrete transport type into `AuthPart`. Internal auth HTTP should go through a small object-safe `AuthHttpExecutor`, described later.

The `Endpoint` trait should eventually include:

```rust
pub trait Endpoint<Cx: ClientContext>: Send + Sync + Sized + 'static {
    const METHOD: Method;

    type Route: RoutePart<Cx, Self>;
    type Policy: PolicyPart<Cx, Self>;
    type Auth: AuthPart<Cx, Self>;
    type Pagination: PaginationPart<Cx, Self>;
    type Body: BodyPart<Self>;
    type Response: ResponseSpec;
}
```

This makes auth visible in the generated endpoint type, like route, policy, body, response, and pagination.

## 2. Auth Runtime Must Be a Controller, Not Only a Hook

Auth needs a lifecycle:
- initialize endpoint auth,
- prepare each attempt,
- remember what was applied,
- inspect the response,
- optionally invalidate credentials,
- optionally request one bounded retry.

Target shape:

```rust
pub type AuthFuture<'a, T> =
    Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait AuthController<Cx: ClientContext, E: Endpoint<Cx>>:
    Send + Sync + 'static
{
    type State: Send + Sync + 'static;

    fn init(&self, ep: &E) -> Result<Self::State, ApiClientError>;

    fn prepare<'a>(
        &'a self,
        state: &'a mut Self::State,
        ctx: AuthPrepareContext<'a, Cx, E>,
    ) -> AuthFuture<'a, Result<AuthAttempt, ApiClientError>>;

    fn on_response<'a>(
        &'a self,
        state: &'a mut Self::State,
        ctx: AuthResponseContext<'a, Cx, E>,
    ) -> AuthFuture<'a, Result<AuthResponseAction, ApiClientError>>;
}
```

`AuthPrepareContext` should include:
- endpoint,
- vars,
- secret or current transitional auth vars,
- runtime auth registry,
- auth HTTP executor,
- request metadata,
- mutable built request.

`AuthResponseContext` should include:
- endpoint,
- vars,
- secret,
- runtime auth registry,
- auth HTTP executor,
- response status,
- response headers,
- the `AuthAttempt` returned by `prepare`.

Response action:

```rust
pub enum AuthResponseAction {
    Continue,
    Retry {
        reason: AuthRetryReason,
    },
}
```

`on_response` must return an action. The current `on_response -> ()` cannot drive refresh-and-retry.

## 3. Request Auth Composition Is a Deterministic Chain

The DSL can feel graph-like, but request mutation must be deterministic.

Recommended request application order:
- client-level auth,
- outer scope auth,
- inner scope auth,
- endpoint auth,
- runtime override, if explicitly enabled.

Codegen should produce a type-level chain:

```rust
pub struct AuthChain<A, B>(PhantomData<(A, B)>);
```

`AuthChain<A, B>` applies `A` then `B`.

This matches the existing `Chain<A, B>` route/policy model and keeps tree readability from the DSL evolution.

For DSL merge behavior:
- `append`: generate `AuthChain<Inherited, New>`.
- `replace`: discard inherited auth and use only the replacement subtree.
- `remove(name)`: codegen filters the named inherited step before emitting the chain.
- `all_of`: generate a chain that applies all steps.
- `one_of`: generate an explicit alternative controller, not a normal chain.

The core should not try to infer merge behavior dynamically from names if the macro can resolve it statically.

## 4. Credential Dependencies Can Be a DAG, But Request Usage Should Not

OAuth2 refresh may need another credential, for example:
- resource request uses bearer access token,
- token refresh request uses client id/client secret as Basic auth,
- token endpoint response produces a new access token.

That is a credential acquisition dependency graph.

It should not make normal endpoint request mutation an arbitrary graph.

Use two models:
- Endpoint auth plan: deterministic chain or explicit alternative.
- Credential acquisition dependencies: DAG inside providers and internal auth HTTP.

This keeps endpoint execution easy to reason about while still supporting OAuth2.

## 5. Material Providers and Usage Applicators Are Separate

The public model should let users customize either side independently.

Credential material:

```rust
pub trait CredentialMaterial: Clone + Send + Sync + 'static {
    fn expires_at(&self) -> Option<Instant> {
        None
    }

    fn safe_identity(&self) -> AuthIdentity {
        AuthIdentity::Anonymous
    }
}
```

Examples:

```rust
pub struct ApiKey {
    pub value: SecretString,
    pub identity_hint: Option<String>,
}

pub struct AccessToken {
    pub token: SecretString,
    pub expires_at: Option<Instant>,
    pub refresh_token: Option<SecretString>,
    pub scope: Vec<String>,
    pub audience: Option<String>,
}

pub struct BasicCredential {
    pub username: String,
    pub password: SecretString,
}

pub struct ClientCertificate {
    pub identity_id: String,
}
```

Provider:

```rust
pub trait CredentialProvider<Cx: ClientContext>: Send + Sync + 'static {
    type Credential: CredentialMaterial;

    fn id(&self) -> CredentialId;

    fn acquire<'a>(
        &'a self,
        ctx: CredentialContext<'a, Cx>,
    ) -> AuthFuture<'a, Result<Self::Credential, AuthError>>;

    fn refresh<'a>(
        &'a self,
        ctx: CredentialContext<'a, Cx>,
        current: &'a Self::Credential,
    ) -> AuthFuture<'a, Result<Self::Credential, AuthError>> {
        Box::pin(async move {
            let _ = current;
            self.acquire(ctx).await
        })
    }

    fn invalidate<'a>(
        &'a self,
        ctx: CredentialContext<'a, Cx>,
        current: Option<&'a Self::Credential>,
        reason: InvalidateReason,
    ) -> AuthFuture<'a, Result<(), AuthError>> {
        Box::pin(async move {
            let _ = (ctx, current, reason);
            Ok(())
        })
    }
}
```

Usage:

```rust
pub trait AuthUsage<Cx: ClientContext, E: Endpoint<Cx>, M: CredentialMaterial>:
    Send + Sync + 'static
{
    fn name(&self) -> AuthUsageId;

    fn apply(
        &self,
        ctx: AuthApplyContext<'_, Cx, E>,
        material: &M,
    ) -> Result<AuthAppliedPart, ApiClientError>;

    fn challenge(
        &self,
        ctx: AuthChallengeContext<'_, Cx, E, M>,
    ) -> AuthChallengeDecision {
        let _ = ctx;
        AuthChallengeDecision::Ignore
    }
}
```

This generic form is the main extension path. It is typed like pagination, so a user can implement custom providers/usages without type-erasing everything.

Built-in usage adapters:
- `BearerAuth<P>` where `P::Credential = AccessToken`.
- `BasicAuth<P>` where `P::Credential = BasicCredential`.
- `HeaderAuth<P>` where `P::Credential` can expose a secret string.
- `QueryAuth<P>` where `P::Credential` can expose a secret string.
- `CertificateAuth<P>` where `P::Credential = ClientCertificate`.
- `RequestSigner<P, S>` for custom signing.

## 6. Provide a Step Adapter for Normal Cases

Most endpoint auth is one credential provider plus one usage applicator.

Core should provide:

```rust
pub struct UseCredential<P, U> {
    provider: P,
    usage: U,
    policy: AuthStepPolicy,
}
```

`UseCredential<P, U>` implements `AuthPart` or an internal `AuthStep` when:
- `P: CredentialProvider<Cx>`,
- `U: AuthUsage<Cx, E, P::Credential>`.

This gives the normal user a simple extension point:
- implement `CredentialProvider` for how to get/refresh material,
- implement `AuthUsage` for how to apply it,
- compose them with `UseCredential`.

Advanced users can implement `AuthController` directly.

## 7. Runtime Single-Flight Belongs in a Credential Slot

Providers should not each reinvent waiting behavior.

Core should provide a single-flight slot:

```rust
pub struct CredentialSlot<P: CredentialProvider<Cx>, Cx: ClientContext> {
    provider: P,
    state: Mutex<CredentialSlotState<P::Credential>>,
}
```

State:

```rust
enum CredentialSlotState<T> {
    Empty,
    Valid {
        value: T,
        generation: u64,
    },
    Refreshing {
        generation: u64,
        notify: Arc<Notify>,
    },
    Failed {
        generation: u64,
        error: SharedAuthError,
        retry_after: Option<Instant>,
    },
}
```

Rules:
- If the credential is valid and not expiring, return it.
- If it is missing, expired, or rejected, one caller becomes leader.
- Followers wait on `Notify`.
- The leader must not hold the mutex while doing network I/O.
- The leader commits `Valid` or `Failed` and wakes followers.
- Followers re-check state after wakeup.
- Failed acquisition must wake all followers with a cloned/redacted error.

Leader flow:

```text
lock slot
if valid: return value
if refreshing: clone notify, unlock, wait, loop
mark refreshing, unlock
perform acquire or refresh
lock slot
commit valid or failed
notify waiters
return result
```

Do not store dynamic tokens directly in `vars` or `secret`.

`vars` is normal client configuration. `secret` is static sensitive configuration. Runtime token state belongs in `ClientRuntimeState` through shared `Arc` slots, so cloned clients observe the same auth state.

## 8. Credential Registry

Multiple endpoints and cloned clients need to find the same slots.

Runtime should contain:

```rust
pub struct AuthRegistry {
    slots: DashMap<CredentialId, Arc<dyn DynCredentialSlot>>,
}
```

The typed path should avoid dynamic downcasts where possible. Codegen can store concrete slots in generated client state when the set is static.

Pragmatic recommendation:
- Start with typed `Arc<CredentialSlot<P, Cx>>` inside generated auth controllers.
- Add `AuthRegistry` for dynamic or named lookup later.
- Keep `CredentialId` in all applied auth metadata from day one.

`CredentialId` should be stable and redacted:

```rust
pub struct CredentialId {
    pub namespace: &'static str,
    pub name: &'static str,
}
```

Examples:
- `client.app_token`
- `client.riot_api_key`
- `scope.platform.user_token`

## 9. Auth Attempt State Is Required

Each request attempt must remember what auth was applied.

```rust
pub struct AuthAttempt {
    pub applied: Vec<AuthAppliedPart>,
    pub retry_budget: AuthRetryBudget,
}

pub struct AuthAppliedPart {
    pub credential_id: CredentialId,
    pub usage_id: AuthUsageId,
    pub generation: Option<u64>,
    pub identity: AuthIdentity,
    pub provenance: AuthProvenance,
}
```

This matters because after a `401`:
- we only invalidate credentials that were actually used,
- we avoid invalidating a newer token generated by another task,
- we retry only if the retry budget allows it,
- diagnostics can say exactly which auth step failed.

Generation-guarded invalidation:

```text
request used token generation 10
another task refreshes to generation 11
this request receives 401 for generation 10
invalidate only if current generation is still 10
do not discard generation 11
```

## 10. Refresh and 401 Retry Policy

Auth retry must be separate from general retry.

Default behavior:
- Missing credential before request: acquire once through the slot.
- Expired credential before request: refresh or acquire once through the slot.
- Credential expiring soon: refresh if policy says proactive refresh.
- `401` with a used bearer token: invalidate that token generation and retry once.
- `401` after the auth retry: return the HTTP status or a structured auth error.
- `403`: do not refresh by default, but allow usage/provider policy to opt in.
- `WWW-Authenticate: Bearer error="invalid_token"`: treat as token rejection.

Policy type:

```rust
pub struct AuthStepPolicy {
    pub refresh_skew: Duration,
    pub retry_on_unauthorized: bool,
    pub max_auth_retries: u8,
    pub retry_on_forbidden: bool,
}
```

No infinite loops:
- auth retry count is separate from transport retry count,
- the same auth step cannot trigger unbounded retries,
- internal token request failures are auth errors, not normal endpoint retries unless explicitly configured.

## 11. Internal Auth HTTP Must Be Explicit

Do not call the normal public request pipeline recursively from inside token refresh unless recursion is explicitly handled.

Core should expose a small internal executor:

```rust
pub trait AuthHttpExecutor: Send + Sync {
    fn send<'a>(
        &'a self,
        req: AuthHttpRequest,
    ) -> AuthFuture<'a, Result<AuthHttpResponse, AuthError>>;
}
```

Request:

```rust
pub struct AuthHttpRequest {
    pub method: Method,
    pub url: Url,
    pub headers: HeaderMap,
    pub body: Option<Bytes>,
    pub mode: AuthMode,
    pub policy: AuthInternalPolicy,
}
```

Mode:

```rust
pub enum AuthMode {
    SkipAuth,
    UseAuth(AuthRequirementId),
}
```

Default for login/refresh should be `SkipAuth`.

If an OAuth2 token endpoint needs Basic client auth, the provider should apply that explicitly inside the internal request. It should not accidentally use the resource endpoint bearer auth.

The executor should:
- use the same underlying transport,
- not run normal endpoint auth by default,
- support timeout,
- optionally support rate-limit/retry through an explicit internal policy,
- record a reentry stack to detect credential recursion.

Recursion example to reject:

```text
resource request needs app_token
app_token provider calls token endpoint
token endpoint is configured to need app_token
```

That must fail as `AuthErrorKind::RecursionDetected`.

## 12. Cache and Inflight Must Not Leak Secrets

Current inflight key includes all headers as text. That is not acceptable once `Authorization` or `X-Api-Key` exists.

Authenticated request identity must use redacted fingerprints, not raw tokens.

Recommended change:
- `AuthAppliedPart` contributes `AuthIdentity`.
- cache/inflight key builders can include `AuthIdentity`.
- raw sensitive headers are excluded or redacted from default keys.

Example:

```rust
pub enum AuthIdentity {
    Anonymous,
    Static(&'static str),
    User(String),
    Tenant(String),
    ScopeAudience {
        scope: Vec<String>,
        audience: Option<String>,
    },
    OpaqueHash([u8; 32]),
}
```

For API keys and tokens:
- never include the raw value,
- use a stable non-reversible hash if identity separation is needed,
- prefer semantic identity if known, such as tenant/user/audience/scope.

Recommended request flow:

```text
1. build route/policy/body
2. create auth controller
3. auth prepare: acquire/refresh and apply
4. compute safe cache key
5. compute safe inflight key
6. rate-limit
7. send
8. auth response handling
9. classify response
10. cache success
11. decode
12. general retry if needed
```

Do not cache a protected response under only `method + url` unless the endpoint explicitly declares the response is auth-independent.

## 13. mTLS and Certificate Auth

`CertificateAuth` is not a header/query applicator.

It requires transport support.

Core needs a request extension area:

```rust
pub struct BuiltRequest {
    pub meta: RequestMeta,
    pub url: Url,
    pub headers: HeaderMap,
    pub body: Option<Bytes>,
    pub timeout: Option<Duration>,
    pub extensions: RequestExtensions,
}
```

`CertificateAuth` should write a certificate identity into request extensions:

```rust
pub enum TransportAuth {
    ClientCertificate { identity_id: String },
}
```

`ReqwestTransport` may need:
- a client pool keyed by certificate identity,
- or a transport implementation that can choose a preconfigured client per request.

Do not pretend certificates are normal request headers.

## 14. One-Of Auth

`one_of` is not the same as `all_of`.

Possible semantics:
- Try first alternative that can prepare successfully.
- If the prepared request gets `401`, optionally try the next alternative once.
- Do not apply two alternatives at the same time.
- Diagnostics must show which alternatives were attempted.

Target controller:

```rust
pub struct OneOfAuth<A, B> {
    first: A,
    second: B,
}
```

`OneOfAuth` state should record the active branch for the current attempt.

Start with `all_of` and deterministic chains. Add `one_of` after single-flight and auth retry are solid.

## 15. DSL Direction

Credential declaration:

```rust
client Api {
    secret {
        client_id: String,
        client_secret: String,
        api_key: String,
    }

    auth {
        credential app_token: OAuth2ClientCredentials {
            token_url: "https://auth.example.com/oauth/token",
            client_id: secret.client_id,
            client_secret: secret.client_secret,
            scope: "read:items",
        }

        credential api_key: ApiKey(secret.api_key)
    }
}
```

Usage by scope:

```rust
scope api {
    use_auth BearerAuth(app_token)

    GET Items {
        path["items"]
        -> Json<Vec<Item>>;
    }
}
```

Same material, different usage:

```rust
scope legacy {
    use_auth HeaderAuth("X-Api-Key", api_key)
}

scope query_legacy {
    use_auth QueryAuth("api_key", api_key)
}
```

Multiple auth:

```rust
scope tenant {
    params {
        tenant_id: String,
    }

    use_auth [
        BearerAuth(app_token),
        HeaderValue("X-Tenant", tenant.tenant_id),
    ]
}
```

Alternative auth:

```rust
GET MaybeUser {
    use_auth one_of [
        BearerAuth(user_token),
        HeaderAuth("X-Api-Key", fallback_key),
    ]
}
```

Important syntax rule:
- credential declaration says how to acquire material,
- `use_auth` says how to apply material to this request.

Do not merge those into one concept.

## 16. Public Extension Levels

Users should be able to customize auth at four levels.

Level 1: Custom credential provider.

Use when the API has a custom login or refresh protocol.

```rust
impl CredentialProvider<MyCx> for MyLoginProvider {
    type Credential = AccessToken;

    fn id(&self) -> CredentialId {
        CredentialId::new("client", "my_token")
    }

    fn acquire<'a>(
        &'a self,
        ctx: CredentialContext<'a, MyCx>,
    ) -> AuthFuture<'a, Result<AccessToken, AuthError>> {
        Box::pin(async move {
            // Build internal login request through ctx.executor.
            Err(AuthError::new(AuthErrorKind::AcquireFailed, "example only"))
        })
    }
}
```

Level 2: Custom usage applicator.

Use when the API needs a special header/query/signature.

```rust
impl<Cx, E> AuthUsage<Cx, E, AccessToken> for MySignature
where
    Cx: ClientContext,
    E: Endpoint<Cx>,
{
    fn name(&self) -> AuthUsageId {
        AuthUsageId::new("my_signature")
    }

    fn apply(
        &self,
        ctx: AuthApplyContext<'_, Cx, E>,
        token: &AccessToken,
    ) -> Result<AuthAppliedPart, ApiClientError> {
        // Mutate ctx.request.
        Err(ApiClientError::Auth {
            ctx: ctx.error_context(),
            source: AuthError::new(AuthErrorKind::InvalidConfiguration, "example only"),
        })
    }
}
```

Level 3: Custom auth step.

Use when provider plus usage is not enough.

```rust
pub trait AuthStep<Cx: ClientContext, E: Endpoint<Cx>>:
    AuthController<Cx, E>
{
}
```

Level 4: Custom full controller.

Use for advanced behavior such as multi-step challenge/response.

This mirrors pagination:
- most users select built-ins,
- some users implement a provider,
- fewer implement usage,
- advanced users implement a controller.

## 17. Error Model

Add structured auth errors:

```rust
ApiClientError::Auth {
    ctx: ErrorContext,
    source: AuthError,
}
```

Kinds:

```rust
pub enum AuthErrorKind {
    MissingCredential,
    AcquireFailed,
    RefreshFailed,
    RejectedCredential,
    UnsupportedScheme,
    RecursionDetected,
    ProviderRejected,
    StateUnavailable,
    CertificateUnavailable,
    InvalidConfiguration,
}
```

Auth errors must redact:
- tokens,
- API keys,
- passwords,
- refresh tokens,
- client secrets,
- certificate private key material.

`Debug` output must also redact. Do not rely only on `Display`.

## 18. Implementation Plan

First PR: structural auth part.
- Add `AuthPart`, `AuthController`, `NoAuth`, `AuthChain`.
- Add `type Auth` to `Endpoint`.
- Wire auth controller creation into request execution.
- Keep current `AuthProvider` only as a temporary legacy hook or remove it if migration is acceptable.

Second PR: request attempt state.
- Add `AuthAttempt`.
- Make auth prepare return applied metadata.
- Pass `AuthAttempt` into response handling.
- Change response handling from `on_response -> ()` to `AuthResponseAction`.

Third PR: single-flight credentials.
- Add `CredentialMaterial`.
- Add `CredentialProvider`.
- Add `CredentialSlot`.
- Add generation-guarded invalidation.
- Add tests for concurrent acquire and refresh.

Fourth PR: built-in bearer token.
- Add `AccessToken`.
- Add `BearerAuth`.
- Add `StaticBearerProvider`.
- Add refreshable test provider.
- Add `401` invalidate and one auth retry.

Fifth PR: static key/basic/query/header.
- Add `ApiKey`.
- Add `BasicCredential`.
- Add `HeaderAuth`.
- Add `QueryAuth`.
- Add `BasicAuth`.
- Ensure cache/inflight keys do not leak raw headers.

Sixth PR: internal auth HTTP.
- Add `AuthHttpExecutor`.
- Add `AuthMode`.
- Add recursion detection.
- Add OAuth2 client credentials provider.
- Add refresh token provider.

Seventh PR: advanced composition.
- Add `one_of`.
- Add `replace/remove` codegen behavior.
- Add better diagnostics with auth provenance.

Eighth PR: certificate auth.
- Add request extensions.
- Add `TransportAuth`.
- Update `ReqwestTransport` or document a transport extension point.

## 19. Tests

Core tests:
- `no_auth_does_not_mutate_request`
- `auth_chain_applies_in_order`
- `bearer_auth_applies_authorization_header`
- `basic_auth_applies_authorization_header`
- `header_auth_applies_custom_header`
- `query_auth_updates_url_before_cache_key`
- `missing_token_single_flight_acquire`
- `expired_token_single_flight_refresh`
- `refresh_failure_wakes_all_waiters`
- `generation_guard_does_not_discard_newer_token`
- `response_401_invalidates_and_retries_once`
- `response_401_after_auth_retry_does_not_loop`
- `internal_auth_request_skips_resource_auth`
- `internal_auth_recursion_is_detected`
- `inflight_key_redacts_authorization_header`
- `cache_key_includes_safe_auth_identity`
- `one_of_uses_first_successful_branch`
- `one_of_can_try_next_branch_after_rejection`

DSL/codegen tests:
- client auth is inherited by scopes,
- scope auth is inherited by endpoints,
- endpoint auth append works,
- endpoint auth replace works,
- endpoint auth remove works,
- duplicate credential names fail,
- unknown credential names fail,
- bare route/query params do not resolve to auth credentials,
- `vars` and `secret` are respected as reserved namespaces,
- generated endpoint has `type Auth`,
- generated auth chain order follows tree order.

## Final Recommendation

Do not grow the current global `AuthProvider` into a giant auth framework.

The strong model is:
- generated `AuthPart` declares auth for each endpoint,
- `AuthController` executes auth with per-request/per-attempt state,
- `CredentialProvider` acquires or refreshes material,
- `CredentialSlot` provides shared single-flight state across cloned clients,
- `AuthUsage` applies material to requests,
- `AuthHttpExecutor` performs login/refresh requests without recursive endpoint auth,
- cache/inflight use redacted auth identity, never raw secrets.

This is close to the pagination architecture: static generated endpoint parts by default, controller lifecycle at runtime, and trait extension points for users who need custom behavior.
