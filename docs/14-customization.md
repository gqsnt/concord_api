# 14. Customization and Extension Points

Concord has two kinds of customization.

DSL customization is part of the generated client. These extension points are intended to be used from `api!` definitions or generated client setters.

Core customization is available when working with `concord_core::ApiClient` directly or when injecting runtime objects into a generated client. Generated wrappers forward the common runtime knobs. Direct core usage remains useful for lower-level client contexts, manual endpoints, and advanced extension work.

This chapter lists the extension points that can be implemented by user code and the places where each implementation is used.

## Quick Reference

| What you customize | Trait or type | Used by generated wrapper | Used by DSL |
| --- | --- | --- | --- |
| Route/header/query value formatting | `Display` or `ToString` | Yes | Yes |
| Auth credential acquisition | `CredentialProvider<Cx>` | Yes | `credential x: Custom<T>(expr)` |
| Auth wire format | `AuthUsage<Cx, E, M>` | Yes | `use_auth Custom<T>(expr, credential)` |
| Cache backend | `CacheStore` | Yes, `with_cache_store` | Cache DSL creates default Moka backend |
| Rate limiter | `RateLimiter` | Yes, `with_rate_limiter` | Rate-limit DSL creates plans |
| Rate-limit response headers | `RateLimitResponsePolicy` | Yes, through limiter | `response custom Type` |
| HTTP transport | `Transport` and `TransportBody` | Yes, `new_with_transport` | No |
| Inflight deduplication key | `InflightPolicy` | Yes, `with_inflight_policy` | No |
| Pagination wrapper response | `PageItems`, `HasNextCursor` | Yes | `paginate CursorPagination`, etc. |
| Pagination controller | `concord_core::internal::Controller` | Advanced | `paginate MyController { ... }` |
| Runtime hooks | `RuntimeHooks` | Yes, `with_runtime_hooks` | No |
| Retry policy | `RetryPolicy` | Yes, `with_retry_policy` | Prefer retry DSL |
| Debug sink | `DebugSink` | Yes, `with_debug_sink` | No |
| Manual route/header/query policy | `RoutePart`, `PolicyPart` | Core only | Macro normally generates these |
| Manual client/endpoint | `ClientContext`, `Endpoint` | Core only | Macro normally generates these |
| Custom codecs | Internal codec traits | Not currently public | No stable external hook |

## Custom Value Types

Any value used in a route, header, query, rate-limit key, or pagination expression must format into a string. Implement `Display` for domain enums and newtypes.

```rust
#[derive(Clone, Copy)]
pub enum PlatformRoute {
    Euw1,
    Na1,
}

impl core::fmt::Display for PlatformRoute {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            PlatformRoute::Euw1 => f.write_str("euw1"),
            PlatformRoute::Na1 => f.write_str("na1"),
        }
    }
}
```

Then use it in the DSL.

```rust
scope platform(platform: PlatformRoute) {
    host[platform, "api"]

    GET Status
    -> Json<PlatformData>
    {
        path["lol", "status", "platform-data"]
    }
}
```

Host labels are validated after formatting. Path segments are percent-encoded after formatting.

## Custom Route and Policy Parts

The macro normally generates route and policy parts from `host[...]`, `path[...]`, `headers { ... }`, `query { ... }`, and policy blocks.

For manual core clients, you can implement route and policy parts directly through `concord_core::internal`.

```rust
pub struct ApiRoute;

impl<Cx, E> concord_core::internal::RoutePart<Cx, E> for ApiRoute
where
    Cx: ClientContext,
{
    fn apply(
        _ep: &E,
        _vars: &Cx::Vars,
        _auth: &Cx::AuthVars,
        route: &mut RouteParts,
    ) -> Result<(), ApiClientError> {
        route.path_mut().push_segment("api");
        Ok(())
    }
}

pub struct TenantPolicy;

impl<Cx, E> concord_core::internal::PolicyPart<Cx, E> for TenantPolicy
where
    Cx: ClientContext,
{
    fn apply(
        _ep: &E,
        _vars: &Cx::Vars,
        _auth: &Cx::AuthVars,
        policy: &mut Policy,
    ) -> Result<(), ApiClientError> {
        policy.insert_header(
            http::HeaderName::from_static("x-tenant"),
            http::HeaderValue::from_static("tenant-a"),
        );
        policy.set_query("sdk", "concord");
        Ok(())
    }
}
```

Use these only for low-level endpoints that implement `Endpoint` by hand. For macro-generated clients, prefer the DSL because it handles inheritance, validation, and generated constructors.

## Custom Auth Credential Provider

Implement `CredentialProvider<Cx>` when the client must obtain credentials from somewhere other than static secrets.

For simple login endpoints declared in the DSL, prefer `credential x: Endpoint(auth::LoginEndpoint)` first. It reuses endpoint response mapping and generates explicit lifecycle helpers (`acquire_auth_*`, `set_auth_*_value`, `has_auth_*`, `clear_auth_*`) without custom provider code.

```rust
#[derive(Clone)]
pub struct StaticTokenProvider;

impl<Cx: ClientContext> CredentialProvider<Cx> for StaticTokenProvider {
    type Credential = AccessToken;

    fn id(&self) -> CredentialId {
        CredentialId::new("example", "static-token")
    }

    fn acquire<'a>(
        &'a self,
        _ctx: CredentialContext<'a, Cx>,
    ) -> AuthFuture<'a, Result<Self::Credential, AuthError>> {
        Box::pin(async { Ok(AccessToken::new("token-value")) })
    }
}
```

Use it in the DSL with `Custom<T>(expr)`.

```rust
api! {
    client Api {
        scheme: https,
        host: "example.com",

        auth {
            credential token: Custom<StaticTokenProvider>(StaticTokenProvider)
        }
    }

    GET Ping
    -> Json<()>
    {
        use_auth BearerAuth(token)
    }
}
```

The provider can also implement `refresh` and `invalidate` when credentials expire or are rejected.

```rust
impl<Cx: ClientContext> CredentialProvider<Cx> for StaticTokenProvider {
    type Credential = AccessToken;

    fn id(&self) -> CredentialId {
        CredentialId::new("example", "rotating-token")
    }

    fn acquire<'a>(
        &'a self,
        ctx: CredentialContext<'a, Cx>,
    ) -> AuthFuture<'a, Result<Self::Credential, AuthError>> {
        Box::pin(async move {
            let _reason = ctx.reason;
            Ok(AccessToken::new("first-token"))
        })
    }

    fn refresh<'a>(
        &'a self,
        _ctx: CredentialContext<'a, Cx>,
        _current: &'a Self::Credential,
    ) -> AuthFuture<'a, Result<Self::Credential, AuthError>> {
        Box::pin(async { Ok(AccessToken::new("refreshed-token")) })
    }
}
```

`AccessToken`, `ApiKey`, `BasicCredential`, and `ClientCertificate` already implement the credential material traits needed by built-in auth usages.

## Custom Auth Provider With Internal HTTP

A provider can call `ctx.executor.send(...)` to perform login, refresh, or discovery requests with the same transport stack.

Use this pattern when auth acquisition cannot be modeled as a normal DSL endpoint, or when acquisition must use provider-level `AuthMode` controls (`SkipAuth` or explicit internal `UseAuth` requirements).

```rust
#[derive(Clone)]
pub struct LoginProvider {
    username: SecretString,
    password: SecretString,
}

impl LoginProvider {
    pub fn new(username: SecretString, password: SecretString) -> Self {
        Self { username, password }
    }
}

#[derive(serde::Deserialize)]
struct LoginResponse {
    access_token: String,
}

impl<Cx: ClientContext> CredentialProvider<Cx> for LoginProvider {
    type Credential = AccessToken;

    fn id(&self) -> CredentialId {
        CredentialId::new("example", "login")
    }

    fn acquire<'a>(
        &'a self,
        ctx: CredentialContext<'a, Cx>,
    ) -> AuthFuture<'a, Result<Self::Credential, AuthError>> {
        Box::pin(async move {
            let mut form = url::form_urlencoded::Serializer::new(String::new());
            form.append_pair("username", self.username.expose());
            form.append_pair("password", self.password.expose());

            let mut headers = http::HeaderMap::new();
            headers.insert(
                http::header::CONTENT_TYPE,
                http::HeaderValue::from_static("application/x-www-form-urlencoded"),
            );

            let url = format!("{}://{}/login", Cx::SCHEME, Cx::DOMAIN)
                .parse()
                .expect("valid login url");

            let response = ctx.executor.send(AuthHttpRequest {
                method: http::Method::POST,
                url,
                headers,
                body: Some(bytes::Bytes::from(form.finish())),
                mode: AuthMode::SkipAuth,
                policy: AuthInternalPolicy::default(),
            }).await?;

            if !response.status.is_success() {
                return Err(AuthError::new(
                    AuthErrorKind::AcquireFailed,
                    format!("login returned {}", response.status),
                ));
            }

            let parsed: LoginResponse = serde_json::from_slice(&response.body).map_err(|err| {
                AuthError::new(AuthErrorKind::AcquireFailed, format!("decode failed: {err}"))
            })?;

            Ok(AccessToken::new(parsed.access_token))
        })
    }
}
```

Use it from the DSL by passing secret values into the provider expression.

```rust
client Api {
    scheme: https,
    host: "example.com",

    secret {
        username: String,
        password: String
    }

    auth {
        credential session: Custom<LoginProvider>(
            LoginProvider::new(secret.username.clone(), secret.password.clone())
        )
    }
}
```

## Custom Auth Usage

Implement `AuthUsage<Cx, E, M>` when the credential material is right but the wire format is custom.

```rust
#[derive(Clone, Copy)]
pub struct PrefixBearer {
    prefix: &'static str,
}

impl PrefixBearer {
    pub fn new(prefix: &'static str) -> Self {
        Self { prefix }
    }
}

impl<Cx, E> AuthUsage<Cx, E, AccessToken> for PrefixBearer
where
    Cx: ClientContext,
    E: Endpoint<Cx>,
{
    fn name(&self) -> AuthUsageId {
        AuthUsageId::new("prefix-bearer")
    }

    fn apply(
        &self,
        ctx: AuthApplyContext<'_, Cx, E>,
        material: &AccessToken,
    ) -> Result<AuthIdentity, ApiClientError> {
        let raw = format!("Bearer {}{}", self.prefix, material.token.expose());
        let value = http::HeaderValue::from_str(&raw).map_err(|_| {
            ApiClientError::InvalidParam {
                ctx: ctx.error_context(),
                param: "authorization formatted bearer token",
            }
        })?;
        ctx.request.headers.insert(http::header::AUTHORIZATION, value);
        Ok(material.safe_identity())
    }
}
```

Use it in the DSL.

```rust
GET Ping
-> Json<()>
{
    use_auth Custom<PrefixBearer>(PrefixBearer::new("tenant-a:"), token)
}
```

Implement `challenge` when a response should invalidate the credential and retry.

```rust
fn challenge(&self, ctx: AuthChallengeContext<'_, Cx, E>) -> AuthChallengeDecision {
    if ctx.status == http::StatusCode::UNAUTHORIZED {
        AuthChallengeDecision::RejectCredential
    } else {
        AuthChallengeDecision::Ignore
    }
}
```

## Custom Credential Material

For most APIs, use the built-in `AccessToken`, `ApiKey`, `BasicCredential`, or `ClientCertificate`.

If a provider returns a different material, implement `CredentialMaterial`. If the material exposes a secret string and should work with `HeaderAuth` or `QueryAuth`, also implement `SecretCredential`.

```rust
#[derive(Clone)]
pub struct SessionKey {
    value: SecretString,
    tenant: String,
}

impl CredentialMaterial for SessionKey {
    fn safe_identity(&self) -> AuthIdentity {
        AuthIdentity::Tenant(self.tenant.clone())
    }
}

impl SecretCredential for SessionKey {
    fn secret_value(&self) -> &str {
        self.value.expose()
    }
}
```

This makes the material usable with built-in header and query auth usages.

## Custom Cache Store

Implement `CacheStore` to replace the cache backend.

The simplest store implements `key_for`, `get`, and `put`.

```rust
use concord_core::transport::{BuiltRequest, BuiltResponse};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;

#[derive(Default)]
pub struct MemoryCache {
    entries: Mutex<HashMap<CacheKey, BuiltResponse>>,
}

impl CacheStore for MemoryCache {
    fn key_for(&self, request: &BuiltRequest) -> Option<CacheKey> {
        if matches!(&request.cache, CacheSetting::Config(_)) {
            Some(default_cache_key(request))
        } else {
            None
        }
    }

    fn get<'a>(
        &'a self,
        key: &'a CacheKey,
    ) -> Pin<Box<dyn Future<Output = Option<BuiltResponse>> + Send + 'a>> {
        Box::pin(async move {
            self.entries.lock().expect("cache lock").get(key).cloned()
        })
    }

    fn put<'a>(
        &'a self,
        key: CacheKey,
        response: BuiltResponse,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            self.entries.lock().expect("cache lock").insert(key, response);
        })
    }
}
```

Install it on a generated client.

```rust
let api = cached_api::CachedApi::new()
    .with_cache_store(Arc::new(MemoryCache::default()));
```

`request.cache` is `CacheSetting::Config(_)` when the effective DSL policy enables caching. It is `CacheSetting::Off` or `CacheSetting::Inherit` when a store should normally bypass the request.

For HTTP semantics, override the lifecycle methods instead.

```rust
impl CacheStore for MyHttpCache {
    fn before_request<'a>(
        &'a self,
        request: &'a BuiltRequest,
    ) -> Pin<Box<dyn Future<Output = CacheBefore> + Send + 'a>> {
        Box::pin(async move {
            let _request = request;
            CacheBefore::Miss
        })
    }

    fn after_response<'a>(
        &'a self,
        request: &'a BuiltRequest,
        response: &'a BuiltResponse,
        revalidation: Option<CacheRevalidation>,
    ) -> Pin<Box<dyn Future<Output = CacheAfter> + Send + 'a>> {
        Box::pin(async move {
            let _ = (request, response, revalidation);
            CacheAfter::Stored
        })
    }

    fn after_error<'a>(
        &'a self,
        request: &'a BuiltRequest,
        error: &'a ApiClientError,
        revalidation: Option<CacheRevalidation>,
    ) -> Pin<Box<dyn Future<Output = Option<BuiltResponse>> + Send + 'a>> {
        Box::pin(async move {
            let _ = (request, error);
            revalidation.map(|cached| cached.cached_response)
        })
    }
}
```

Use lifecycle methods when you need stale revalidation, `304 Not Modified` merging, stale-on-error fallback, custom invalidation, or a distributed cache.

## Custom Rate Limiter

Implement `RateLimiter` when the generated rate-limit plan should be enforced by custom logic.

```rust
#[derive(Clone, Default)]
pub struct RecordingLimiter {
    plans: Arc<Mutex<Vec<RateLimitPlan>>>,
}

impl RateLimiter for RecordingLimiter {
    fn acquire<'a>(
        &'a self,
        ctx: RateLimitContext<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<RateLimitPermit, ApiClientError>> + Send + 'a>> {
        Box::pin(async move {
            self.plans.lock().expect("plan lock").push(ctx.plan.clone());
            Ok(RateLimitPermit)
        })
    }

    fn on_response<'a>(
        &'a self,
        ctx: RateLimitResponseContext<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<RateLimitResponseAction, ApiClientError>> + Send + 'a>> {
        Box::pin(async move {
            let _ = ctx;
            Ok(RateLimitResponseAction::Continue)
        })
    }
}
```

Install it on a generated client.

```rust
let api = rate_limit_dsl_api::RateLimitDslApi::new()
    .with_rate_limiter(Arc::new(RecordingLimiter::default()));
```

For the built-in governor limiter, window memory can be tuned:

```rust
let limiter = GovernorRateLimiter::new()
    .with_max_window_entries(4096)
    .with_window_idle_ttl(core::time::Duration::from_secs(15 * 60));
```

`acquire` runs before transport. Return a `RateLimitPermit` when the request is allowed. Return an `ApiClientError` to fail the request before transport.

`on_response` runs after a response and can store cooldown state or return a limited action.

## Custom Rate-Limit Response Policy

Use `RateLimitResponsePolicy` when the upstream API reports rate-limit scope or delay in custom headers.

```rust
#[derive(Default)]
pub struct HeaderScopePolicy;

impl RateLimitResponsePolicy for HeaderScopePolicy {
    fn observe(&self, ctx: &RateLimitResponseContext<'_>) -> RateLimitObservation {
        if ctx.status != http::StatusCode::TOO_MANY_REQUESTS {
            return RateLimitObservation::continue_();
        }

        let target = ctx.headers
            .get(http::HeaderName::from_static("x-limit-scope"))
            .and_then(|value| value.to_str().ok())
            .map(|value| match value.trim() {
                "application" => RateLimitTarget::bucket_kind("application", RateLimitTarget::Host),
                "method" => RateLimitTarget::bucket_kind("method", RateLimitTarget::Endpoint),
                _ => RateLimitTarget::current_plan_or_endpoint(),
            })
            .unwrap_or_else(RateLimitTarget::current_plan_or_endpoint);

        let delay = ctx.headers
            .get(http::HeaderName::from_static("x-delay-ms"))
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok())
            .map(core::time::Duration::from_millis)
            .or_else(|| parse_retry_after(ctx.headers));

        let mut observation = RateLimitObservation::limited().with_target(target);
        if let Some(delay) = delay {
            observation = observation.with_delay(delay);
        }
        observation
    }
}
```

`parse_retry_after` supports both delta-seconds and HTTP-date forms.

Reference it from the DSL.

```rust
rate_limit {
    response custom HeaderScopePolicy

    profile app {
        bucket application by [route.host] {
            limit 500 every 10 seconds
        }
    }
}
```

When a generated client sees a custom response policy, it configures the governor rate limiter with that policy.

## Custom Transport and Body

Implement `Transport` to replace the HTTP stack or to test without network I/O.

```rust
use concord_core::transport::{
    BuiltRequest, Transport, TransportBody, TransportError, TransportResponse,
};
use bytes::Bytes;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Clone, Default)]
pub struct CountingTransport {
    calls: Arc<AtomicUsize>,
}

impl Transport for CountingTransport {
    fn send(
        &self,
        req: BuiltRequest,
    ) -> Pin<Box<dyn Future<Output = Result<TransportResponse, TransportError>> + Send>> {
        let calls = self.calls.clone();
        Box::pin(async move {
            calls.fetch_add(1, Ordering::SeqCst);
            let body = Bytes::from_static(b"null");
            let mut headers = http::HeaderMap::new();
            headers.insert(
                http::header::CONTENT_TYPE,
                http::HeaderValue::from_static("application/json"),
            );
            Ok(TransportResponse {
                meta: req.meta,
                url: req.url,
                status: http::StatusCode::OK,
                headers,
                content_length: Some(body.len() as u64),
                rate_limit: req.rate_limit,
                body: Box::new(StaticBody { chunk: Some(body) }),
            })
        })
    }
}

pub struct StaticBody {
    chunk: Option<Bytes>,
}

impl TransportBody for StaticBody {
    fn next_chunk<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Bytes>, TransportError>> + Send + 'a>> {
        Box::pin(async move { Ok(self.chunk.take()) })
    }
}
```

Inject it with `new_with_transport`.

```rust
let api = client::Client::new_with_transport(CountingTransport::default());
```

A custom transport must honor `BuiltRequest` fields that apply to it: URL, headers, body, timeout, metadata, rate-limit plan, and auth transport extensions.

For client certificates, built-in `CertificateAuth` writes `BuiltRequest.extensions.transport_auth`. A transport that supports per-request certificates should inspect that field.

## Custom Inflight Policy

Inflight policy decides which requests are deduplicated while a matching request is already in progress.

```rust
pub struct GetOnlyInflight;

impl InflightPolicy for GetOnlyInflight {
    fn key_for(&self, req: &BuiltRequest) -> Option<RequestKey> {
        if req.meta.method == http::Method::GET && req.body.is_none() {
            Some(RequestKey::new(format!("GET {}", req.url)))
        } else {
            None
        }
    }
}
```

Install it on a generated client.

```rust
let api = client::Client::new()
    .with_inflight_policy(Arc::new(GetOnlyInflight));
```

Use `SafeMethodInflightPolicy` when the default safe-method behavior is enough.

A fresh cache hit returns before inflight coordination, so cached hits do not join or lead inflight requests.

## Custom Pagination Output

For a response wrapper, implement `PageItems`.

```rust
#[derive(serde::Deserialize)]
pub struct Page<T> {
    items: Vec<T>,
    next: Option<String>,
}

impl<T: Send + 'static> PageItems for Page<T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;

    fn len(&self) -> usize {
        self.items.len()
    }

    fn inner_into_iter(self) -> Self::IntoIter {
        self.items.into_iter()
    }
}
```

For cursor pagination, also implement `HasNextCursor`.

```rust
impl<T: Send + 'static> HasNextCursor for Page<T> {
    type Cursor = String;

    fn next_cursor(&self) -> Option<&Self::Cursor> {
        self.next.as_ref()
    }
}
```

Then use built-in cursor pagination.

```rust
GET List(page_cursor?: String, page_size: u64 = 50)
-> Json<Page<Item>>
{
    query {
        "cursor" = page_cursor,
        "limit" = page_size
    }
    paginate CursorPagination {
        cursor = page_cursor,
        per_page = page_size
    }
}
```

## Custom Pagination Controller

The built-in controllers are `OffsetLimitPagination`, `PagedPagination`, and `CursorPagination`.

Advanced users can provide a custom controller type. The generated code expects the controller type to implement `Default` and `concord_core::internal::Controller<Cx, E>`. The `paginate` block assigns directly to public fields on the controller.

```rust
#[derive(Default, Clone)]
pub struct HeaderCursorPagination {
    pub cursor_header: &'static str,
    pub query_key: &'static str,
}
```

A real controller implementation must define state, write request policy in `apply_policy`, inspect the decoded page in `on_page`, and optionally expose a progress key for loop detection.

```rust
impl<Cx, E> concord_core::internal::Controller<Cx, E> for HeaderCursorPagination
where
    Cx: ClientContext,
    E: Endpoint<Cx>,
    <E::Response as concord_core::internal::ResponseSpec>::Output: PageItems,
{
    type State = Option<String>;

    fn init(&self, _ep: &E) -> Result<Self::State, ApiClientError> {
        Ok(None)
    }

    fn apply_policy(
        &self,
        state: &Self::State,
        _ep: &E,
        policy: &mut PolicyPatch<'_>,
    ) -> Result<(), ApiClientError> {
        if let Some(cursor) = state {
            policy.set_query(self.query_key, cursor.clone());
        }
        Ok(())
    }

    fn on_page(
        &self,
        state: &mut Self::State,
        _ep_next: &mut E,
        _resp: &DecodedResponse<<E::Response as concord_core::internal::ResponseSpec>::Output>,
    ) -> Result<concord_core::internal::Control, ApiClientError> {
        let _ = self.cursor_header;
        *state = None;
        Ok(concord_core::internal::Control::Stop)
    }

    fn progress_key(&self, state: &Self::State, _ep: &E) -> Option<ProgressKey> {
        state.clone().map(ProgressKey::Str)
    }
}
```

Use it in the DSL.

```rust
GET List
-> Json<Vec<Item>>
{
    paginate HeaderCursorPagination {
        cursor_header = "x-next-cursor",
        query_key = "cursor"
    }
}
```

This is an advanced extension point because it uses `concord_core::internal` traits. Prefer built-in controllers unless the upstream API cannot be modeled with offset, page, or cursor pagination.

## Custom Runtime Hooks

`RuntimeHooks` can observe or block sends.

```rust
#[derive(Default)]
pub struct MetricsHooks;

impl RuntimeHooks for MetricsHooks {
    fn pre_send<'a>(
        &'a self,
        ctx: PreSendHookContext<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<(), ApiClientError>> + Send + 'a>> {
        Box::pin(async move {
            println!("sending {} {}", ctx.meta.method, ctx.meta.url);
            Ok(())
        })
    }

    fn post_response<'a>(
        &'a self,
        ctx: PostResponseHookContext<'a>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            println!("response {}", ctx.status);
        })
    }
}
```

Install hooks on a generated client.

```rust
let api = client::Client::new()
    .with_runtime_hooks(Arc::new(MetricsHooks));
```

Hooks are runtime instrumentation, so they stay out of the DSL. Generated wrappers forward the runtime hook setters to the underlying core client.

## Custom Retry Policy

The retry DSL covers most endpoint-specific retry behavior. For direct core clients, implement `RetryPolicy` to provide a fallback policy used when request retry setting is `Inherit`.

```rust
pub struct RetryTimeouts;

impl RetryPolicy for RetryTimeouts {
    fn max_retries(&self) -> u32 {
        2
    }

    fn should_retry(&self, ctx: &RetryContext<'_>) -> RetryDecision {
        match ctx.outcome {
            RetryOutcome::Transport(err)
                if err.kind() == concord_core::transport::TransportErrorKind::Timeout =>
            {
                RetryDecision::Retry
            }
            _ => RetryDecision::Stop,
        }
    }
}
```

Install it on a generated client when the retry DSL is not the right abstraction.

```rust
let api = client::Client::new()
    .with_retry_policy(Arc::new(RetryTimeouts));
```

Prefer the retry DSL for endpoint-specific policy. Use a runtime `RetryPolicy` for environment-wide fallback behavior.

## Manual Client Context and Endpoint

The macro normally generates `ClientContext`, `Endpoint`, route parts, policy parts, auth parts, body parts, and response specs.

For low-level tests or framework code, you can implement them manually with `concord_core::internal` helper types.

```rust
pub struct TestCx;

impl ClientContext for TestCx {
    type Vars = ();
    type AuthVars = ();
    type AuthState = ();

    const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTPS;
    const DOMAIN: &'static str = "example.com";

    fn init_auth_state(_vars: &Self::Vars, _auth: &Self::AuthVars) -> Self::AuthState {}
}

pub struct Ping;

impl Endpoint<TestCx> for Ping {
    const METHOD: http::Method = http::Method::GET;

    type Route = concord_core::internal::NoRoute;
    type Policy = concord_core::internal::NoPolicy;
    type Auth = concord_core::internal::NoAuth;
    type Pagination = concord_core::internal::NoPagination;
    type Body = concord_core::internal::NoBody;
    type Response = concord_core::internal::Decoded<Json, ()>;

    fn name(&self) -> &'static str {
        "Ping"
    }
}
```

Then use the core client directly.

```rust
let api = ApiClient::<TestCx, _>::with_transport((), (), transport);
api.request(Ping).execute().await?;
```

This is intentionally lower level than the DSL. Use it only when the macro is not the right abstraction.

## Custom Debug Sink

`DebugSink` is a core trait for replacing debug output. Generated wrappers forward debug-sink setters.

Install a custom sink on a generated client:

```rust
let api = client::Client::new()
    .with_debug_sink(Arc::new(MyDebugSink))
    .with_debug_level(DebugLevel::V);
```

or per request:

```rust
api.request(endpoints::Ping::new())
    .debug_level(DebugLevel::VV)
    .execute()
    .await?;
```

## Custom Codecs

The DSL supports the public codecs currently exported by `concord_core`, such as `Json<T>`, `Text<T>`, and `NoContent<()>`.

The lower-level codec traits exist inside `concord_core`, but they are not currently exported as a stable public extension surface. Do not document or rely on external custom codecs without first exposing those traits from the core crate.

If custom codec support is needed, the core crate should intentionally re-export the codec traits and the macro should have tests for an external codec crate.

## Choosing the Right Extension Point

Use the DSL first for API contract: routes, headers, query, auth placement, retry, rate-limit plan, cache policy, body, response, and pagination.

Use generated client setters for runtime behavior that varies by process: cache store, rate limiter, inflight policy, debug level, pagination caps, and transport.

Use custom auth providers and usages when the upstream authentication flow is non-standard.

Use direct `concord_core::ApiClient` only for lower-level integration work where hand-written client contexts or endpoints are the right abstraction.
