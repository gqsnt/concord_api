# 18. Rate-Limit System Blueprint (Simple + Extensible)

This chapter records a deep audit of current rate limiting in Concord (DSL + core runtime), then proposes a cleaner target model that stays practical.

## 1) Design goals

1. Keep author intent explicit in the DSL.
2. Keep runtime behavior predictable across retry/cache/inflight/auth.
3. Prevent hidden quota amplification and unbounded runtime state growth.
4. Improve diagnostics where current errors are technically correct but hard to act on.
5. Add only high-value surface area; avoid a second mini-language.

## 2) Current model snapshot

Current model has three layers:

1. DSL declaration:
   - `rate_limit { profile ... default ... }`
   - endpoint/scope/profile application via `rate_limit ...`, `rate_limit only ...`, `rate_limit off`
   - custom response policy via `response custom Type`
2. Generated plan:
   - `RateLimitPlan` of `RateLimitBucketUse { id, key, windows, cost }`
3. Runtime enforcement:
   - `RateLimiter::acquire` before transport
   - `RateLimiter::on_response` after response classification hooks
   - governor default implementation with permit windows + cooldown memory

## 3) What already works well

1. DSL layering semantics (`add`/`replace`/`off`) are coherent and test-covered.
2. Response policy is extensible without DSL complexity explosion.
3. Integration with cache/retry/inflight is mostly correct:
   - fresh cache hits skip limiter
   - stale revalidation goes through limiter
   - inflight followers do not consume extra permits
4. Runtime extension point (`RateLimiter`) cleanly allows custom backends.

## 4) Key edge cases and pain points

### A. Runtime state growth risk (high impact)

`GovernorRateLimiter` stores per-window limiters in a `HashMap` keyed by bucket id + resolved key + window and does not evict old entries. High-cardinality keys can grow forever.

### B. Duplicate-plan amplification (high impact)

Plans are concatenated across defaults/profiles/layers (`extend` semantics) with no dedup or canonicalization. Accidental duplicate buckets consume permits multiple times for one request.

### C. Weak diagnostics for some key errors (medium)

Named rate-limit key failures often report with call-site spans, not the original DSL token span, which slows debugging.

### D. Runtime validation happens too late for some windows (medium)

Some invalid quota shapes are only rejected at runtime by governor (`window too small for max`) although all inputs are static DSL values and could fail at compile time.

### E. Retry-After parsing is narrow (medium)

`parse_retry_after` only supports delta-seconds. HTTP-date form is ignored.

### F. `rate_limit off` semantics can surprise (medium)

`off` clears proactive plan buckets, but response-policy fallback can still store endpoint cooldown and throttle later calls. This behavior is valid but not obvious from syntax alone.

### G. Key aliasing ergonomics are uneven (low/medium)

`rate_limit key` exists in `scope`, but not as a first-class endpoint-level alias declaration. Reusing shared profiles across differently named endpoint params is harder than necessary.

## 5) DX/UX target model

Keep the current mental model:

1. Profiles define reusable quota plans.
2. Endpoint/scope chooses apply mode (`add`, `only`, `off`).
3. Response policy interprets 429/headers.

Then add focused improvements.

### DSL improvements (small, high value)

1. Add optional endpoint-level key alias:
   - `rate_limit key region = platform` inside endpoint blocks too.
2. Add optional bucket cost in DSL:
   - `cost 2` in bucket body (maps to existing core `RateLimitBucketUse.cost`).
3. Clarify `off` in docs as:
   - "clear proactive bucket plan"
   - not "disable all reactive cooldown behavior."

### Core/runtime improvements (smallest clean refactor)

1. Canonicalize merged `RateLimitPlan` before request execution:
   - coalesce exact duplicate buckets (same id/key/windows/cost).
2. Add governor window state hygiene:
   - bounded size and/or idle TTL pruning for window limiter map.
3. Move static quota-shape validation to macro sema:
   - reject impossible `limit N every D` combinations at compile time.
4. Extend `Retry-After` parser:
   - support delta-seconds and HTTP-date.
5. Improve unresolved key diagnostics:
   - carry source span in resolved key IR.

## 6) Extensibility boundaries

Keep these stable:

1. `RateLimiter` remains the primary runtime extension point.
2. `RateLimitResponsePolicy` remains the DSL-driven response parser hook.
3. `RateLimitPlan` remains transport/runtime-neutral data.

Avoid:

1. embedding header parsing rules directly into DSL
2. introducing strategy sublanguages inside `rate_limit`
3. tying rate-limit state to cache/auth internals

## 7) Behavior contract (target)

1. Same request should not consume extra permits due to duplicate profile composition.
2. High-cardinality key usage should not cause unbounded memory growth in default limiter.
3. Invalid static quota shape should fail at compile time.
4. `rate_limit off` should continue to clear proactive plan, while docs explicitly explain reactive cooldown fallback behavior.
5. Response-policy delays should remain coordinated with retry to avoid double sleeping.
6. Cloned clients should continue sharing limiter state unless runtime state is explicitly replaced.

## 8) Dependency-ordered implementation plan

### Phase 1: correctness + safety

1. Add plan canonicalization step in request build/execute path.
2. Add governor window map pruning (idle TTL and hard cap).
3. Expand `Retry-After` parsing to HTTP-date.

### Phase 2: compile-time rigor

1. Add sema validation for impossible quota precision (`every/max` too small).
2. Replace saturating conversions in rate-limit sema with checked conversions and explicit diagnostics.
3. Preserve and report key-spec spans for unknown/unbound keys.

### Phase 3: DSL ergonomics

1. Add endpoint-level `rate_limit key` binding.
2. Add bucket `cost` field in DSL and codegen.
3. Document exact semantics for `off`, fallback targets, and custom policy interactions.

## 9) Implementation status

Implemented:

1. Request plans are canonicalized before send, so exact duplicate buckets are coalesced.
2. Governor limiter supports bounded window entries and idle TTL pruning.
3. `parse_retry_after` supports delta-seconds and HTTP-date.
4. Compile-time validation rejects impossible window precision (`limit` too high for `every`).
5. Rate-limit key diagnostics preserve source token spans.
6. Endpoint blocks now support `rate_limit key name = param`.
7. Bucket `cost` is parsed, validated, and emitted to runtime plans.
8. Docs now clarify `rate_limit off` as proactive-plan clear, not a global reactive cooldown disable.

## 10) Required tests

### New runtime tests

1. Duplicate profile composition does not multiply permit acquisition after canonicalization.
2. Governor limiter prunes stale window specs under high-cardinality key churn.
3. HTTP-date `Retry-After` is parsed and honored.

### New DSL/compile-fail tests

1. Invalid quota precision fails at compile time with actionable message.
2. Unknown named rate-limit key reports the key token span.
3. Endpoint-level `rate_limit key` alias works and rejects optional params.
4. Bucket `cost` rejects zero and non-integer literals.

### Regression tests (must keep passing)

1. Existing `rate_limit_dsl` behavior matrix.
2. Riot plan-shape tests.
3. Retry/rate-limit coordination tests.
4. Cache-hit/inflight follower no-extra-permit tests.

## 11) Why this balance is right

1. It fixes real operational risks (state growth, duplicate amplification).
2. It sharpens DX without adding heavy new concepts.
3. It keeps the current successful DSL mental model intact.
4. It keeps custom limiter implementations first-class.
