# Rate Limit Implementation Plan

## Goal

Implement DSL-driven rate limiting end to end:

- Concord owns the public model: plans, buckets, windows, response adapters, and extension traits.
- `governor` is the default local runtime engine.
- Users can still replace the limiter completely by implementing `RateLimiter`.
- Users can customize response interpretation and, later, distributed storage without changing the DSL.
- The runtime ordering is correct: cache hits skip rate limits, inflight followers do not consume permits, retries/pages consume permits for real HTTP sends, and 429 feedback does not double-sleep with retry.

## High-Level Decision

Use `governor` as the default engine, not as Concord's public API.

Public surface:

```rust
pub trait RateLimiter: Send + Sync + 'static {
    fn acquire<'a>(
        &'a self,
        ctx: RateLimitContext<'a>,
    ) -> RateLimitFuture<'a, Result<RateLimitPermit, ApiClientError>>;

    fn on_response<'a>(
        &'a self,
        ctx: RateLimitResponseContext<'a>,
    ) -> RateLimitFuture<'a, Result<RateLimitResponseAction, ApiClientError>>;
}
```

Default implementation:

```rust
pub type DefaultRateLimiter = GovernorRateLimiter;
```

Custom implementation remains simple:

```rust
api.with_rate_limiter(Arc::new(MyCompanyLimiter::new(...)));
```

If `rate-limit-governor` is disabled, the generated DSL can still compile as long as the caller installs a custom limiter, but the ergonomic default should be `GovernorRateLimiter`.

## Cargo Plan

Add an optional dependency:

```toml
governor = { version = "0.10", optional = true }
```

Add features:

```toml
[features]
default = ["rate-limit-governor"]
rate-limit-governor = ["dep:governor"]
```

Reasoning:

- The user-facing default is governor.
- Users who want no default engine can opt out with `default-features = false`.
- `RateLimiter` remains available regardless of governor.
- `NoopRateLimiter` remains for explicit no-limit behavior and tests.

## Core Model

Add or expand `concord_core/src/rate_limit` into a module:

```text
rate_limit/
  mod.rs
  context.rs
  plan.rs
  limiter.rs
  response.rs
  governor.rs
```

### Plan Types

```rust
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RateLimitPlan {
    buckets: Vec<RateLimitBucketUse>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RateLimitBucketUse {
    pub bucket: RateLimitBucketId,
    pub key: RateLimitKey,
    pub windows: Vec<RateLimitWindow>,
    pub cost: NonZeroU32,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RateLimitBucketId {
    pub kind: Cow<'static, str>,
    pub name: Cow<'static, str>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RateLimitKey {
    pub parts: Vec<RateLimitKeyPart>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RateLimitKeyPart {
    pub name: Cow<'static, str>,
    pub value: Cow<'static, str>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RateLimitWindow {
    pub max: NonZeroU32,
    pub per: Duration,
}
```

Important rules:

- `RateLimitPlan::default()` means no limit.
- Keys must use safe derived values only: endpoint names, route host, region param, credential identity ID, not raw secrets.
- `cost` defaults to 1 but gives us future support for weighted requests.
- Window durations and max values must reject zero at DSL/sema and core constructors.

### Policy Setting

Add:

```rust
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum RateLimitSetting {
    #[default]
    Inherit,
    Add(RateLimitPlan),
    Replace(RateLimitPlan),
    Off,
}
```

Policy semantics:

- `Inherit`: keep parent/current plan.
- `Add`: append buckets to the current plan.
- `Replace`: replace inherited plan with this plan.
- `Off`: clear the current plan.

Expose helpers on `Policy` and `PolicyPatch`:

```rust
Policy::add_rate_limit(plan)
Policy::replace_rate_limit(plan)
Policy::clear_rate_limit()
PolicyPatch::add_rate_limit(plan)
PolicyPatch::replace_rate_limit(plan)
PolicyPatch::clear_rate_limit()
```

### Request Plumbing

Extend:

```rust
pub struct BuiltRequest {
    ...
    pub rate_limit: RateLimitPlan,
}
```

Extend contexts:

```rust
pub struct RateLimitContext<'a> {
    pub endpoint: &'static str,
    pub method: &'a Method,
    pub url: &'a str,
    pub attempt: u32,
    pub page_index: u32,
    pub idempotent: bool,
    pub plan: &'a RateLimitPlan,
}

pub struct RateLimitResponseContext<'a> {
    pub meta: RateLimitContext<'a>,
    pub status: StatusCode,
    pub headers: &'a HeaderMap,
}
```

## Default Governor Runtime

Add `GovernorRateLimiter` behind `rate-limit-governor`:

```rust
pub struct GovernorRateLimiter {
    windows: Mutex<HashMap<GovernorWindowSpec, Arc<GovernorWindowLimiter>>>,
    cooldowns: Mutex<HashMap<RateLimitCooldownKey, Instant>>,
    response_adapter: Arc<dyn RateLimitResponseAdapter>,
    mode: RateLimitAcquireMode,
}
```

Conceptual governor mapping:

```rust
// N requests every P duration:
// quota = one cell every P / N, burst N.
let max = NonZeroU32::new(max_requests).expect("non-zero limit");
let quota = governor::Quota::with_period(period / max.get())
    .expect("non-zero period")
    .allow_burst(max);
```

For a request with several buckets and windows:

1. Build all concrete `GovernorWindowSpec` values from the request plan.
2. Check provider/server cooldowns first.
3. Acquire every bucket/window with `until_key_n_ready` or equivalent.
4. Return `RateLimitPermit`.

Important limitation:

- `governor` cannot atomically reserve across multiple independent bucket/window limiters.
- The default engine should be conservative: it must never under-throttle.
- If one window is acquired and a later window waits, the request can be over-throttled.
- This is acceptable for the default local engine.
- The trait model should still allow a custom exact atomic store later.

Tests for `GovernorRateLimiter` should assert safety and ordering, not perfect multi-window atomicity.

## Response Feedback

Add:

```rust
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum RateLimitResponseAction {
    #[default]
    Continue,
    Cooldown {
        retry_after: Duration,
        scope: RateLimitCooldownScope,
    },
}

pub trait RateLimitResponseAdapter: Send + Sync + 'static {
    fn observe(&self, ctx: &RateLimitResponseContext<'_>) -> RateLimitResponseAction;
}
```

Default adapter behavior:

- If status is not 429, return `Continue`.
- If status is 429 and `Retry-After` is present, return a cooldown action.
- If status is 429 and no useful provider bucket header exists, cooldown the concrete request scope.

Riot adapter behavior:

- Parse `X-Rate-Limit-Type` when present.
- Map `application`, `method`, and `service` to bucket scopes.
- Honor `Retry-After` as the strongest signal.
- If Riot omits rate-limit type for an underlying service 429, fall back to concrete request scope.

## Retry Integration

Avoid double sleep.

Current retry can already honor `Retry-After`. Rate limit response handling needs to pass its cooldown decision to the retry loop.

Implementation options:

1. Add `rate_limit_action: Option<RateLimitResponseAction>` to the internal send error path.
2. Or add `rate_limit_retry_after: Option<Duration>` to `ApiClientError::HttpStatus`.

Prefer option 1 if it can stay internal. Use option 2 only if the public error needs to expose rate-limit context.

Retry delay selection:

```text
if rate_limit_action has cooldown:
    use that delay
else:
    use retry policy decision
```

The limiter should still store cooldown state in `on_response`, so future requests are blocked even if the current request is not retried.

## Runtime Pipeline Fix

Move rate-limit acquire into the real-send path.

Target order:

```text
1. build request, auth, policy, body
2. cache lookup; return if hit
3. compute inflight key
4. if shareable, join_or_lead
5. if follower, wait and return shared result; no rate-limit acquire
6. if leader or no inflight, acquire rate-limit permit
7. pre_send hook
8. send transport request
9. post_response hook
10. rate_limit on_response
11. classify HTTP status
12. retry/auth decisions
13. cache store
14. wake inflight followers
```

This fixes the current issue where rate limiting happens before inflight leader selection.

## Internal Auth Requests

Keep current policy:

```rust
AuthInternalPolicy {
    use_rate_limiter: bool,
    ...
}
```

When `use_rate_limiter` is true:

- build a synthetic `RateLimitPlan` for the auth request,
- use a bucket kind such as `auth`,
- key by auth provider identity and auth request name,
- never use raw tokens/secrets in the key.

Default remains false because login/token refresh endpoints often have different limits than the user API.

## DSL Plan

### Client-Level Profiles

```rust
client RiotClient {
    rate_limit {
        response RiotRateLimitHeaders

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

        default app
    }
}
```

### Scope Keys

```rust
scope platform {
    params { platform: PlatformRoute }
    host[platform, "api"]

    rate_limit key region = platform
}
```

This lets profiles use a semantic key:

```rust
bucket application by [region] {
    limit 500 every 10 seconds
}
```

### Endpoint Use

```rust
GET GetSummonerByPuuid {
    params { puuid: String }
    path["by-puuid", puuid]
    rate_limit summoner_by_puuid
    -> Json<models::SummonerDto>;
}
```

### Inline Endpoint Limit

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

### Override Semantics

```rust
rate_limit off
rate_limit only special_profile
rate_limit app
rate_limit app, summoner_by_puuid
```

Rules:

- `rate_limit profile` adds to inherited limits.
- `rate_limit profile_a, profile_b` adds several profiles.
- `rate_limit only profile` replaces inherited limits.
- `rate_limit off` clears inherited limits.
- Unknown profile is a compile error.
- Zero limits/durations are compile errors.
- Reserved key names should include `endpoint`, `route.host`, `method`, and later `auth.identity`.

## Macro Work

Update macro layers in this order:

1. AST:
   - profile definitions,
   - bucket definitions,
   - limits,
   - default profiles,
   - response adapter reference,
   - endpoint/scope `rate_limit` uses,
   - scope key bindings.
2. Parser:
   - parse client-level `rate_limit { ... }`,
   - parse endpoint/scope statements,
   - parse duration grammar: `10 seconds`, `1 minute`, `10 minutes`.
3. Sema:
   - validate profile existence,
   - validate no duplicate profile names,
   - validate bucket/window values are non-zero,
   - validate key names resolve,
   - validate `route.host`, `endpoint`, and scope keys,
   - validate `only` and `off` cannot be mixed with additive uses.
4. Codegen:
   - emit `RateLimitPlan` construction,
   - emit `Policy::add_rate_limit` / `replace_rate_limit` / `clear_rate_limit`,
   - emit default client runtime with `GovernorRateLimiter` when feature is enabled,
   - emit response adapter installation if configured.

## Test Plan

### Core Tests

- no plan means no acquire wait.
- single window allows exactly `max` immediately.
- single window waits after exhaustion.
- multi-window acquires every window.
- governor default never under-throttles with multiple windows.
- 429 plus `Retry-After` stores cooldown.
- requests during cooldown wait or error according to mode.
- cache hit does not acquire.
- inflight follower does not acquire.
- retry acquires again for each real attempt.
- paginated request acquires once per page.
- auth internal request only uses limiter when opted in.

### Custom Limiter Tests

- a custom limiter receives the resolved `RateLimitPlan`.
- a custom limiter can deny a request with `ApiClientError`.
- a custom limiter can ignore plans and behave as no-op.
- custom response adapter can map a 429 into a cooldown.

### Macro Tests

- client default profile applies to endpoints.
- endpoint profile adds method bucket on top of app bucket.
- `only` replaces inherited app profile.
- `off` clears inherited limits.
- inline bucket generates a plan.
- scope key binding works from descendant endpoints.
- unknown profile fails compile.
- unknown key dimension fails compile.
- zero max fails compile.
- zero duration fails compile.

### Riot Example Tests

- `champion-v3` endpoint gets app profile plus `30/10s` and `500/10m` method profile.
- `summoner-v4` by puuid gets `1600/1m`.
- high volume endpoints reuse `20000/10s` and `1200000/10m`.
- `match-v5` endpoints reuse `2000/10s`.
- platform and regional hosts produce distinct keys.

## PR Sequence

1. Core model PR:
   - split `concord_core/src/rate_limit.rs` into a module,
   - add `RateLimitPlan`, bucket, key, window, setting, response action,
   - keep `NoopRateLimiter`,
   - no runtime behavior change.
2. Policy/request plumbing PR:
   - add `RateLimitSetting` to `Policy`,
   - add plan to `BuiltRequest`,
   - pass plan into `RateLimitContext`,
   - update tests for existing behavior.
3. Pipeline PR:
   - move acquire into leader-only real-send path,
   - preserve cache-before-limit,
   - add inflight/cache/retry/page tests with a counting limiter.
4. Governor default PR:
   - add dependency and feature,
   - add `GovernorRateLimiter`,
   - make it the default rate limiter when the default feature is enabled,
   - keep explicit custom limiter override.
5. Response feedback PR:
   - add `RateLimitResponseAdapter`,
   - implement default `Retry-After` adapter,
   - wire cooldown into retry without double sleep.
6. DSL examples first PR:
   - add rate-limit DSL usage to `concord_examples/src/riot.rs`,
   - add compile/runtime tests describing desired generated plans,
   - allow project to fail until macro implementation catches up if using TDD.
7. Macro parser/sema/codegen PR:
   - implement client profiles, endpoint uses, inline buckets, `off`, `only`, scope keys,
   - pass macro tests.
8. Riot adapter PR:
   - add `RiotRateLimitHeaders`,
   - map `X-Rate-Limit-Type` and `Retry-After`,
   - wire it in Riot examples.
9. Cleanup PR:
   - docs,
   - public prelude exports,
   - examples,
   - feature docs,
   - final test pass.

## Implementation Guardrails

- Do not leak `governor` types into generated endpoint public APIs.
- Do not use raw secrets in keys.
- Do not make `RateLimiter` generic over the client type.
- Do not let inflight followers consume permits.
- Do not let cache hits consume permits.
- Do not sleep twice for the same 429.
- Do not hard-code Riot into core.
- Keep `NoopRateLimiter` for explicit opt-out and tests.
- Keep custom limiter replacement as a first-class path.

## Source Notes

- `governor` docs: https://docs.rs/governor/latest/governor/
- `governor` feature flags: https://docs.rs/crate/governor/latest/features
- `governor` keyed limiter docs: https://docs.rs/governor/latest/governor/state/keyed/index.html
- `tower::limit::RateLimit`: https://docs.rs/tower/latest/tower/limit/rate/struct.RateLimit.html
