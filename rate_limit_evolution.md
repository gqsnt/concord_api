# Rate Limit Evolution

## Purpose

This document defines the next rate-limit architecture for Concord. The goal is to support provider-specific limits such as Riot Games while keeping the core generic, extensible, and aligned with the DSL direction used for auth, pagination, retry, scope, params, headers, and query.

Rate limiting must not be a thin response hook only. It needs two complementary parts:

- Declaration: the API contract says which buckets apply to which request.
- Runtime coordination: the client waits, reserves, learns from responses, and coordinates concurrent requests.

A response-only limiter reacts after damage is already done. A config-only limiter drifts when the server replies with a 429 or changes its counters. The strong design is both: static DSL/config as the source of expected behavior, response feedback as correction and backpressure.

Riot context verified on 2026-04-14 from the Riot Developer Portal: Riot documents application, method, and service rate-limit categories; app and method limits are per region; 429 responses should be handled using the `Retry-After` header; some underlying-service 429s may not include `X-Rate-Limit-Type`. Source: https://developer.riotgames.com/docs/portal

## Current State

The current core already has a small runtime hook surface:

```rust
pub trait RateLimiter: Send + Sync + 'static {
    fn acquire<'a>(&'a self, ctx: RateLimitContext<'a>) -> RateLimitFuture<'a, Result<RateLimitPermit, ApiClientError>>;
    fn on_response<'a>(&'a self, ctx: RateLimitResponseContext<'a>) -> RateLimitFuture<'a, ()>;
}
```

`ClientRuntimeState` stores an `Arc<dyn RateLimiter>` and `ApiClient` exposes `set_rate_limiter` / `with_rate_limiter`.

The main request path already calls:

- `rate_limiter.acquire(...)` before sending.
- `rate_limiter.on_response(...)` after receiving response headers.

Internal auth requests can also opt into rate limiting through `AuthInternalPolicy { use_rate_limiter: bool }`.

What is missing:

- no declarative rate-limit config model,
- no request-level rate-limit plan in `Policy` / `BuiltRequest`,
- no standard in-memory limiter,
- no bucket identity model,
- no multi-window bucket support,
- no response parser/adaptive feedback model,
- no DSL,
- no integration with inflight/retry semantics,
- no provider-specific helpers such as Riot-style app/method/service limit parsing.

Important pipeline issue: rate limiting currently happens before inflight single-flight leader selection. For safe duplicated requests, only the actual HTTP request should consume permits. Waiters sharing the in-flight response should not burn tokens. The final pipeline should move limiter acquisition inside the in-flight leader send path or make the inflight policy expose a leader-only send section.

## Required Cases

### 1. Static Multi-Window Limits

A bucket can have several windows, and all must permit the request:

```text
500 requests every 10 seconds
30000 requests every 10 minutes
```

For a request to proceed, every window in every applicable bucket must have capacity. If any window is exhausted, acquisition waits until the earliest safe time or returns a structured error depending on policy.

### 2. Application, Method, And Service Buckets

Riot-like APIs have at least three useful bucket scopes:

- application: per API key and region/route host,
- method: per API key, region/route host, and endpoint/method identity,
- service: per service and region, sometimes shared across applications.

The core should not hard-code these names. It should model them as named buckets with key dimensions.

### 3. Region Or Route-Host Partitioning

Riot app and method limits are per region. In the current DSL, the region is usually visible through host composition:

```rust
scope platform {
    params { platform: PlatformRoute }
    host[platform, "api"]
}
```

A practical first key dimension is `route.host`, because `euw1.api.riotgames.com` and `na1.api.riotgames.com` naturally separate. For stronger type-level clarity, the DSL should also allow explicit key bindings from params:

```rust
rate_limit key region = platform
```

This should not create a new endpoint param. It should only define a rate-limit key component for the current scope and descendants.

### 4. Credential Partitioning Without Secret Exposure

Provider limits can be per API key. We should not use raw secret values as bucket keys by default.

The safe default for generated clients should be:

- client namespace,
- credential identity name, if auth is known,
- route host or explicit region,
- bucket name,
- endpoint identity where relevant.

This is correct for clones of the same generated client because they share runtime state. If users want multiple client instances or processes to share limits, they should configure a shared limiter namespace/store explicitly.

Do not hash secrets in the first implementation. It creates security and lifecycle questions. If we need cross-client same-key detection later, expose an explicit user-provided `rate_limit namespace = vars.key_id` or external store key, not secret introspection.

### 5. Response Feedback

Static scheduling prevents most 429s. Response feedback handles drift.

On any response, the limiter should be able to:

- observe status and headers,
- update bucket state,
- set a cooldown if the provider returned `Retry-After`,
- learn provider-specific bucket type if a header is present,
- handle 429s without a bucket-type header as an unknown/provider service cooldown.

For Riot-like behavior:

- `429` plus `Retry-After` must block future matching calls for that duration.
- If `X-Rate-Limit-Type` is present, use it to identify app/method/service if the adapter knows how.
- If it is absent, treat it as an unknown upstream service limit and apply a conservative cooldown to the concrete request key, not the entire client, unless configured otherwise.

The exact header parser must be provider-specific. Core should provide generic extension points and a Riot adapter later.

### 6. 429 And Retry Interaction

Rate limit and retry overlap but must not double-sleep.

Recommended rule:

- Rate limiter owns shared backpressure state and future scheduling.
- Retry owns whether the current failed request is replayed.
- When response is 429, rate limiter records cooldown and returns optional observed delay.
- Retry can use the observed delay if retry policy allows status 429.
- Future requests acquire through the limiter and wait even if the current request is not retried.

This avoids the bad case where retry sleeps for `Retry-After` and then the next limiter acquire sleeps again for the same cooldown.

The implementation probably needs a small coordinator value:

```rust
pub enum RateLimitResponseAction {
    Continue,
    Cooldown { delay: Duration, scope: RateLimitCooldownScope },
}
```

Then the request loop can feed `delay` into retry decision. If we keep `on_response -> ()`, the current request cannot know the learned delay, and only future requests benefit.

### 7. Pagination

Every page is a real HTTP request and must count. The existing `page_index` in `RateLimitContext` is useful for diagnostics but should not change bucket identity by default. All pages for one endpoint should normally share the same app/method buckets.

### 8. Cache And Inflight

Cache hits should not acquire a permit because no HTTP request is sent.

Inflight waiters should not acquire permits. Only the leader that sends the actual request should acquire and report response feedback. This is a pipeline correction compared to current ordering.

### 9. Internal Auth Requests

Internal auth requests are real HTTP requests, but they may belong to a different API or a different limit family.

Default should be conservative:

- Internal auth requests do not use API endpoint rate limits unless `AuthInternalPolicy.use_rate_limiter` is true.
- If enabled, internal requests should use their own explicit bucket, e.g. `auth.login`, or a user-provided custom plan.
- If the login endpoint is part of the same API and provider limit, the DSL should let users bind it to the same app bucket.

### 10. Distributed Limiters

The first standard implementation can be in-memory and clone-safe inside one process. The trait must allow external stores later:

- Redis,
- database lease table,
- local file lock,
- user-provided service.

The config model should not assume the state is in memory. Bucket keys should be serializable strings or structured components that can be encoded deterministically.

### 11. Costed Requests

Some APIs charge more than one unit per request. Add a `cost` field even if Riot examples use cost `1`.

```rust
cost 1
```

A later endpoint could declare `cost 10` without redesigning the core.

### 12. Fail-Open, Fail-Closed, And Max-Wait

Users need explicit behavior when acquiring would wait or when the limiter backend fails:

- `wait`: sleep until permit is available.
- `error`: return `ApiClientError::RateLimited` immediately.
- `fail_open`: allow the request when limiter backend is unavailable.
- `fail_closed`: fail the request when limiter backend is unavailable.
- `max_wait`: wait up to a duration, then error or fail-open depending on policy.

Default recommendation:

- for local in-memory limiter: `wait` with no max by default,
- for external store errors: `fail_closed` by default for provider compliance, configurable to `fail_open` for non-critical telemetry.

## Core Model

### RateLimitConfig

```rust
pub struct RateLimitConfig {
    pub profiles: Vec<RateLimitProfile>,
    pub default: RateLimitSetting,
    pub response: RateLimitResponsePolicy,
}
```

This is the client-level declaration generated from DSL or created by users manually.

### RateLimitSetting

Mirror retry's tri-state idea, but because rate limits are cumulative, it needs additive behavior:

```rust
pub enum RateLimitSetting {
    Inherit,
    Add(RateLimitPlan),
    Replace(RateLimitPlan),
    Off,
}
```

Recommended DSL semantics:

- no rate_limit block: inherit current plan,
- `rate_limit profile_name`: add/overlay that profile onto the inherited plan,
- `rate_limit only profile_name`: replace inherited plan with that profile,
- `rate_limit off`: clear all inherited limits for this subtree/endpoint.

This differs from retry because retry is one config, while rate limit is a set of constraints.

### RateLimitPlan

A resolved per-request plan:

```rust
pub struct RateLimitPlan {
    pub buckets: Vec<RateLimitBucketUse>,
    pub response_policy: RateLimitResponsePolicy,
    pub acquire_policy: RateLimitAcquirePolicy,
}
```

This should be stored in `Policy` and copied into `BuiltRequest`, like retry is now.

### RateLimitBucketUse

```rust
pub struct RateLimitBucketUse {
    pub id: RateLimitBucketId,
    pub key: RateLimitKeyTemplate,
    pub windows: Vec<RateLimitWindow>,
    pub cost: u32,
    pub algorithm: RateLimitAlgorithm,
}
```

The key template is resolved at request build time from known context:

- client namespace,
- endpoint name,
- method,
- route host,
- explicit scope key bindings,
- auth credential identities if declared,
- optional user-defined vars.

### RateLimitWindow

```rust
pub struct RateLimitWindow {
    pub max: u32,
    pub per: Duration,
}
```

Multiple windows on the same bucket all apply.

### Algorithm

Start with one standard algorithm:

```rust
pub enum RateLimitAlgorithm {
    FixedWindowFromFirstRequest,
}
```

This matches Riot's documented guidance that clients can assume a bucket starts when the first API call is made.

Later algorithms:

- `SlidingWindowLog`: precise, memory-heavy.
- `Gcra`: precise enough, efficient, good for distributed stores.
- `TokenBucket`: good smoothing, less exact for fixed-provider windows.
- `LeakyBucket`: useful when the user wants smoothing rather than strict provider mirroring.

For Riot, use `FixedWindowFromFirstRequest` first.

### StandardRateLimiter

Provide a default implementation:

```rust
pub struct StandardRateLimiter<S = InMemoryRateLimitStore> {
    store: S,
    clock: Arc<dyn Clock>,
    response_adapter: Arc<dyn RateLimitResponseAdapter>,
}
```

Store trait:

```rust
pub trait RateLimitStore: Send + Sync + 'static {
    fn acquire<'a>(&'a self, request: RateLimitAcquire<'a>) -> RateLimitFuture<'a, Result<RateLimitPermit, RateLimitError>>;
    fn observe<'a>(&'a self, response: RateLimitObservation<'a>) -> RateLimitFuture<'a, Result<RateLimitObservationResult, RateLimitError>>;
}
```

The in-memory store should use per-key locks and never hold a global mutex while sleeping.

### Acquire Semantics

Acquire must be atomic across all buckets in a plan.

For example, if a request uses `app` and `method` buckets, it cannot reserve method capacity and then discover app is full. The store should compute the maximum required wait across all involved windows and only reserve after the wait completes.

Pseudo-flow:

```text
loop:
  now = clock.now()
  decision = store.check_all(plan, now)
  if decision.allowed:
      store.reserve_all(plan, now)
      return permit
  if acquire_policy.error_on_wait:
      return RateLimited { retry_after: decision.wait }
  sleep(decision.wait)
```

### Permit Semantics

A permit represents a reserved counted request. Provider APIs usually count requests once sent, not when body decoding completes. The first implementation can reserve at acquire time and keep the permit only for diagnostics.

Later improvement:

- If transport fails before any bytes are sent, maybe release the reservation.
- This requires transport-level knowledge that core does not currently expose. Do not add it in the first pass.

## Response Handling

### Generic Response Adapter

```rust
pub trait RateLimitResponseAdapter: Send + Sync + 'static {
    fn observe(&self, ctx: &RateLimitResponseContext<'_>, plan: Option<&RateLimitPlan>) -> RateLimitObservation;
}
```

Default adapter:

- status 429 with `Retry-After` -> cooldown for request plan keys,
- otherwise no-op.

Riot adapter later:

- classify `X-Rate-Limit-Type` if present,
- parse app/method headers if exposed by the provider response,
- use `Retry-After` as the strongest signal,
- fall back to concrete request cooldown if bucket type is absent.

### Error Type

Add a dedicated error variant rather than overloading policy or transport errors:

```rust
ApiClientError::RateLimited {
    ctx: ErrorContext,
    bucket: Option<RateLimitBucketId>,
    retry_after: Option<Duration>,
}
```

This matters for users who want to catch rate-limit waits/errors distinctly from HTTP 429 responses.

## DSL Direction

The DSL should read like provider documentation while staying extensible. Use named profiles like retry, but make composition additive by default.

### Client-Level Profiles

```rust
client RiotClient {
    scheme: https,
    host: "riotgames.com",

    rate_limit {
        response RiotRateLimitHeaders {
            on status[429]
            retry_after honor
            classify header("X-Rate-Limit-Type")
        }

        profile app {
            bucket application by [route.host] {
                limit 500 every 10 seconds
                limit 30000 every 10 minutes
            }
        }

        profile summoner_by_puuid {
            bucket method by [route.host, endpoint] {
                limit 1600 every 1 minute
            }
        }

        profile league_entries {
            bucket method by [route.host, endpoint] {
                limit 50 every 10 seconds
            }
        }

        default app
    }
}
```

Notes:

- `default app` applies application limits to every request unless cleared.
- `rate_limit summoner_by_puuid` on an endpoint adds the method bucket while preserving inherited app limits.
- `route.host` is a built-in key component.
- `endpoint` is a generated stable endpoint identity, not a URL string.
- `RiotRateLimitHeaders` can be a built-in adapter type or user-provided type.

### Scope-Level Keys

When region is clearer as a scope param than as a host string, allow explicit key aliases:

```rust
scope platform {
    params { platform: PlatformRoute }
    host[platform, "api"]

    rate_limit key region = platform
}
```

Then profiles can use `region`:

```rust
profile app {
    bucket application by [region] {
        limit 500 every 10 seconds
        limit 30000 every 10 minutes
    }
}
```

The generated key should include client namespace and bucket id automatically, so users do not have to write those.

### Endpoint Method Buckets

```rust
GET GetSummonerByPuuid {
    params { puuid: String }
    path["by-puuid", puuid]
    rate_limit summoner_by_puuid
    -> Json<models::SummonerDto>;
}
```

### Inline Endpoint Bucket

For one-off cases:

```rust
GET GetLeagueById {
    params { league_id: String }
    path[league_id]
    rate_limit {
        bucket method by [route.host, endpoint] {
            limit 500 every 10 seconds
        }
    }
    -> Json<models::LeagueListDto>;
}
```

### Clearing And Replacing

```rust
GET StaticAsset {
    rate_limit off
    -> Json<models::StaticDto>;
}

GET SpecialCase {
    rate_limit only special_profile
    -> Json<models::SpecialDto>;
}
```

`off` is useful for APIs that do not count toward the app limit, such as static/CDN APIs in Riot's docs.

`only` is intentionally explicit because replacing inherited app limits accidentally would be dangerous.

### Service Buckets

If a provider documents a shared service bucket:

```rust
profile service_status extends app {
    bucket service by [route.host, service("lol-status-v4")] {
        limit 20000 every 10 seconds
        limit 1200000 every 10 minutes
    }
}
```

A `service("...")` key item is a literal component. It should not be a Rust expression.

## Riot Mapping Examples

The user's supplied Riot examples map naturally to app + method profiles.

### Production App Limit

```rust
profile riot_app {
    bucket application by [route.host] {
        limit 500 every 10 seconds
        limit 30000 every 10 minutes
    }
}
```

### summoner-v4 by PUUID

```rust
profile summoner_by_puuid {
    bucket method by [route.host, endpoint] {
        limit 1600 every 1 minute
    }
}
```

Usage:

```rust
GET GetSummonerByPuuid {
    params { puuid: String }
    path["by-puuid", puuid]
    rate_limit summoner_by_puuid
    -> Json<models::SummonerDto>;
}
```

### league-v4 challenger/master/grandmaster

```rust
profile league_queue_slow {
    bucket method by [route.host, endpoint] {
        limit 30 every 10 seconds
        limit 500 every 10 minutes
    }
}
```

Use it on:

- `GetChallengerLeagueByQueue`,
- `GetMasterLeagueByQueue`,
- `GetGrandmasterLeagueByQueue`.

### league-v4 league by ID

```rust
profile league_by_id {
    bucket method by [route.host, endpoint] {
        limit 500 every 10 seconds
    }
}
```

### league-v4 entries

```rust
profile league_entries {
    bucket method by [route.host, endpoint] {
        limit 50 every 10 seconds
    }
}
```

### high-limit Riot endpoints

Many supplied endpoints use:

```rust
profile riot_high_volume_method {
    bucket method by [route.host, endpoint] {
        limit 20000 every 10 seconds
        limit 1200000 every 10 minutes
    }
}
```

This profile can be reused for account-v1, champion-mastery-v4, tournament-stub-v5, spectator-v5, and challenges endpoints where the supplied limits match.

### match-v5 common endpoints

```rust
profile match_v5_method {
    bucket method by [route.host, endpoint] {
        limit 2000 every 10 seconds
    }
}
```

Use it on match by ID, match IDs by PUUID, and timeline if their supplied limits match.

## Macro Plan

### AST

Add:

```rust
RateLimitProfilesBlock
RateLimitProfileDef
RateLimitSpec
RateLimitPatch
RateLimitBucketDef
RateLimitWindowSpec
RateLimitKeySpec
RateLimitResponseSpec
```

Attach `rate_limit: Option<RateLimitSpec>` to client, layer, and endpoint, similar to retry.

### Sema

Resolve client profiles into `RateLimitPlanResolved`.

Validation:

- duplicate profile names rejected,
- duplicate bucket names in one profile rejected unless explicitly overriding,
- unknown profile rejected,
- `limit` value must be greater than 0,
- duration must be greater than 0,
- unsupported key component rejected,
- `rate_limit only` allowed only with a profile or inline plan,
- `rate_limit off` clears inherited plan,
- additive profile application de-duplicates bucket IDs by last writer only when explicit override syntax exists.

### Codegen

Emit policy operations like retry:

```rust
policy.add_rate_limit(...);
policy.replace_rate_limit(...);
policy.clear_rate_limit();
```

Then `Policy::into_parts` returns `RateLimitSetting` and `BuiltRequest` carries it.

## Runtime Integration Plan

Correct request pipeline should become:

```text
1. build route/policy/body/auth
2. cache lookup; return if hit
3. build inflight key
4. if request can be shared, elect inflight leader
5. leader acquires rate-limit permit
6. pre_send hook
7. send transport request
8. response hooks and rate-limit observation
9. auth response handling
10. retry decision; retry attempts go through acquire again
11. decode/map/cache store
12. wake inflight waiters
```

The important changes from the current pipeline are:

- inflight leader election before rate-limit acquire,
- rate-limit response observation should be able to return an action/delay,
- retry should reuse the observed rate-limit delay for 429 to avoid double sleeps.

## Extension Points

### Custom Limiter

Users can still replace everything:

```rust
api.with_rate_limiter(Arc::new(MyLimiter::new(...)))
```

### Custom Store

Users should be able to keep the standard behavior but swap state:

```rust
let limiter = StandardRateLimiter::new(config).with_store(RedisRateLimitStore::new(...));
api.with_rate_limiter(Arc::new(limiter));
```

### Custom Response Adapter

Provider-specific response parsing should be separate from bucket scheduling:

```rust
let limiter = StandardRateLimiter::new(config)
    .with_response_adapter(RiotRateLimitHeaders::default());
```

This mirrors auth's separation of credential material and usage, and pagination's separation of controller and endpoint declaration.

## Known Library Strategy

Use a known crate for the local scheduling algorithm, but do not make it the public Concord model.

The recommended first engine is `governor`, behind a feature such as `rate-limit-governor` or `rate-limit-standard`. It is a mature Rust rate-limiting crate built around quotas and keyed limiters, supports async waiting, and can model common quotas such as "500 requests every 10 seconds" by converting them into a quota with a burst size and replenishment interval. It is a good fit for Concord's first in-process limiter:

```rust
// Conceptual mapping only.
// N requests every P duration:
// quota = one cell every (P / N), burst N.
let max = NonZeroU32::new(max_requests).expect("non-zero limit");
let quota = governor::Quota::with_period(period / max.get())
    .expect("non-zero period")
    .allow_burst(max);
```

This should stay behind Concord's own types:

```rust
pub struct GovernorRateLimiter {
    // bucket/window key -> governor keyed or direct limiter
}

impl RateLimiter for GovernorRateLimiter {
    // Reads Concord's RateLimitPlan and uses governor internally.
}
```

Reasons to keep Concord's abstraction:

- Concord needs DSL-generated `RateLimitPlan` values, not crate-specific configuration.
- Concord needs several buckets per request: application, method, service, endpoint, auth request, etc.
- Concord needs several windows per bucket: for example `500/10s` and `30000/10m`.
- Concord needs response feedback: `Retry-After`, provider bucket type headers, and 429 cooldowns.
- Concord needs pipeline correctness: cache hits skip permits, inflight waiters skip permits, retries reacquire, pages reacquire.
- Concord should allow distributed stores later; `governor` is local in-process by default.

The main tradeoff is semantics. `governor` uses a GCRA/token-bucket-style model. That is safe and smooth, but it is not exactly the same as a provider's server-side fixed window if the provider uses hard reset windows. For Riot-like limits this is acceptable as the first standard limiter because it prevents bursts beyond the declared rate and works well locally. If we later need exact provider-window behavior, we can add another engine under the same `RateLimiter` / `RateLimitStore` traits.

Other libraries considered:

- `tower::limit::RateLimit`: not a good first fit. It is useful if the whole transport becomes a Tower `Service`, but Concord needs endpoint-aware, bucket-aware, response-aware limits over a reqwest-based pipeline.
- `tower-governor`: mainly useful as Tower middleware, more server/middleware shaped than Concord's current client-side endpoint model.
- `leaky-bucket`: useful and simple for async token/leaky-bucket behavior, but Concord would still need to build keyed bucket maps, multi-window composition, response feedback, and the DSL model around it.
- `async-rate-limiter`: simple token-bucket API and cloneable limiter, but too small as the main standard engine for Concord's planned bucket graph.

Reference docs checked:

- `governor`: https://docs.rs/governor/latest/governor/
- `tower::limit::RateLimit`: https://docs.rs/tower/latest/tower/limit/rate/struct.RateLimit.html
- `leaky-bucket`: https://docs.rs/leaky-bucket/latest/leaky_bucket/
- `async-rate-limiter`: https://docs.rs/async-rate-limiter/latest/async_rate_limiter/

Implementation recommendation:

1. Add Concord-owned `RateLimitPlan`, `RateLimitBucketUse`, `RateLimitWindow`, and `RateLimitKey` first.
2. Add a `GovernorRateLimiter` as the first optional standard runtime engine.
3. Keep the existing `RateLimiter` trait as the public extension point, but extend its context to include the resolved request plan.
4. Compose multi-window limits conservatively by acquiring every applicable bucket/window. If acquisition is sequential, partial early reservations can over-throttle but must never under-throttle.
5. Add a custom in-memory or store-backed engine later if strict atomic multi-window reservations become required.
6. Keep distributed rate limiting out of the first engine; expose a `RateLimitStore` trait so Redis or another external coordinator can be added without changing the DSL.

## Tests To Drive Implementation

Core tests:

- `acquire_allows_until_single_window_full`
- `acquire_waits_for_shortest_reset`
- `multi_window_requires_all_windows`
- `multi_bucket_acquire_is_atomic`
- `cooldown_from_429_retry_after_blocks_future_requests`
- `unknown_429_without_bucket_type_uses_concrete_request_scope`
- `inflight_waiters_do_not_consume_permits`
- `retry_429_uses_rate_limit_delay_without_double_sleep`
- `cache_hit_does_not_acquire`
- `auth_internal_request_uses_limiter_only_when_policy_enabled`

Macro tests:

- `client_default_app_rate_limit_applies_to_endpoint`
- `endpoint_rate_limit_profile_adds_method_bucket`
- `scope_rate_limit_key_can_bind_param_region`
- `rate_limit_off_clears_inherited_buckets`
- `rate_limit_only_replaces_inherited_buckets`
- `unknown_rate_limit_profile_is_compile_error`
- `zero_limit_or_zero_duration_is_compile_error`

Riot example tests:

- generated `RiotClient` endpoints carry app + method buckets,
- platform and regional hosts produce distinct bucket keys,
- high-volume profile can be reused across endpoints,
- static/CDN client can opt out or use a different limiter profile.

## Recommended First PR Sequence

1. Add core data model only: `RateLimitSetting`, `RateLimitPlan`, `RateLimitBucketUse`, `RateLimitWindow`, `RateLimitKey`, `RateLimitResponseAction`.
2. Add `Policy` and `BuiltRequest` plumbing, no macro yet.
3. Implement `StandardRateLimiter<InMemoryRateLimitStore>` with fixed-window-from-first-request and fake-clock tests.
4. Fix pipeline ordering so inflight leader acquisition happens before permit consumption.
5. Add 429 `Retry-After` feedback and avoid retry double-sleep.
6. Add macro AST/parser/sema/codegen for `rate_limit` profiles, inline buckets, `off`, and `only`.
7. Add Riot examples in `concord_examples/src/riot.rs` using app, method, and high-volume profiles.
8. Add provider adapter hook, then `RiotRateLimitHeaders` if we want built-in Riot support.

## Final Recommendation

Implement rate limiting as a configured plan plus a trait-backed runtime. The DSL should be the source of truth for expected buckets and windows, while response handling corrects drift and enforces cooldowns. The standard runtime should be in-memory, fixed-window, multi-bucket, and clone-safe first. Keep custom trait/store/response-adapter extension points from day one so users can implement Redis-backed or provider-specific behavior without forking core.

The most important architectural fix is not the DSL parser. It is pipeline correctness: cache should bypass rate limiting, inflight waiters should not consume permits, every real retry/page should acquire again, and 429 response feedback should feed the retry loop without double sleeping.
