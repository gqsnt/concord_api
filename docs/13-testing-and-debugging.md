# 13. Testing and Debugging

The examples use tests as the contract for the DSL. They verify generated request URLs, headers, query strings, body encoding, auth behavior, retry attempts, rate-limit plans, cache behavior, pagination, and compile-time diagnostics.

This chapter shows the test patterns used in `concord_examples/tests`.

## Mock transport tests

Most runtime tests use `concord_test_support::mock()` with scripted replies.

```rust
let (transport, h) = mock()
    .reply(MockReply::ok_json(json_bytes(&())))
    .build();

let api = ApiHeaders::new_with_transport(transport);
api.request(endpoints::One::new()).execute().await?;

let reqs = h.recorded();
assert_request(&reqs[0]).header("x-debug", "override");
h.finish();
```

The mock records `BuiltRequest` values so tests can assert exact policy output.

## Assert generated routes

Routing tests assert host and path behavior.

```rust
assert_request(&reqs[0])
    .host("euw1.api.riotgames.com")
    .path("/lol/matches/a%2Fb");
```

These tests catch bugs in host label validation, path segment encoding, optional segment omission, and scope composition.

## Assert headers and query

Policy tests assert set, override, remove, and append behavior.

```rust
assert_request(req)
    .header(USER_AGENT, "ua")
    .header("x-debug", "override")
    .header_absent("x-static")
    .query_values("dup", &["p:z", "s"]);
```

Use these tests when changing policy inheritance or key normalization.

## Assert body and content type

Body tests assert JSON encoding and content type behavior.

```rust
assert_request(&reqs[0])
    .header(CONTENT_TYPE, "application/json");

let body = std::str::from_utf8(reqs[0].body.as_ref().unwrap()).unwrap();
assert!(body.contains("id"));
```

The tests also verify that explicit `content-type` policy overrides the automatic JSON content type.

## Assert auth

Auth tests check both generated headers/query values and behavior across retries or secret updates.

```rust
assert_request(&reqs[0]).header("x-api-key", "tok1");
api.set_api_key("tok2");
assert_request(&reqs[1]).header("x-api-key", "tok2");
```

One-of auth tests script an unauthorized response first, then assert the fallback request uses the next auth usage.

OAuth2 and custom provider tests assert internal auth HTTP requests before the original API request.

Endpoint-backed manual credential tests should cover the full lifecycle:

- missing credential before acquire (`AuthErrorKind::MissingCredential`)
- explicit acquire through `acquire_auth_*`
- clone visibility for acquired/cleared state
- `401` invalidation without forced auth retry

## Assert retry

Retry tests inspect recorded request count and attempt metadata.

```rust
let reqs = h.recorded();
assert_eq!(reqs.len(), 2);
assert_eq!(reqs[0].meta.attempt, 0);
assert_eq!(reqs[1].meta.attempt, 1);
```

Tests also cover `retry off`, profile inheritance, `Retry-After`, transport-error retry, and idempotency requirements for `POST`.

## Assert rate-limit plans

Rate-limit tests install a recording limiter.

```rust
let limiter = RecordingLimiter::default();
let plans = limiter.plans.clone();
let api = RateLimitDslApi::new_with_transport(transport)
    .with_rate_limiter(Arc::new(limiter));

api.request(endpoints::Ping::new()).execute().await?;

let plans = plans.lock().unwrap().clone();
assert_eq!(plans[0].buckets().len(), 2);
```

This verifies generated `RateLimitPlan` without sleeping or depending on real quota state.

## Assert cache behavior

Cache tests verify transport count and policy interactions.

Fresh cache hit:

```rust
let first = api.request(endpoints::Cached::new()).execute().await?;
let second = api.request(endpoints::Cached::new()).execute().await?;

assert_eq!(first, second);
h.assert_recorded_len(1);
```

No-store:

```rust
h.assert_recorded_len(2);
```

Vary:

```rust
let en1 = api.request(endpoints::Localized::new("en-US".to_string())).execute().await?;
let fr = api.request(endpoints::Localized::new("fr-FR".to_string())).execute().await?;
let en2 = api.request(endpoints::Localized::new("en-US".to_string())).execute().await?;

assert_eq!(en1, en2);
assert_ne!(en1, fr);
h.assert_recorded_len(2);
```

Revalidation tests assert the second request contains `If-None-Match` and that `304 Not Modified` returns the cached body.

## Assert pagination

Pagination tests assert collected output, page metadata, query mutation, caps, and loop detection.

```rust
let out = api.request(endpoints::List::new())
    .paginate()
    .collect()
    .await?;

assert_eq!(out, vec!["a", "b", "c"]);

let reqs = h.recorded();
assert_request(&reqs[0]).page_index(0).query_has("start", "0");
assert_request(&reqs[1]).page_index(1).query_has("start", "2");
```

Use `max_pages`, `max_items`, and `detect_loops` tests for safety behavior.

## Compile-fail tests

The project uses `trybuild` for UI tests.

```rust
#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
```

Use compile-fail tests for macro diagnostics: missing placeholder types, duplicate variables, invalid references (including unknown auth endpoints or unknown rate-limit keys), recursive endpoint-backed credential dependencies, endpoint output types that do not satisfy credential material bounds, unsupported policy syntax, impossible rate-limit windows, invalid bucket cost, or cache feature requirements.

## Debugging generated behavior

For runtime request debugging, set debug level.

```rust
api.request(endpoints::GetPost::new(1))
    .debug_level(DebugLevel::VV)
    .execute()
    .await?;
```

Use `DebugLevel::V` for high-level request and response status logging. Use `DebugLevel::VV` when headers and body previews are needed.

For deterministic tests, prefer mock transport assertions over debug output.

## Useful validation commands

Run focused tests while developing one concept.

```powershell
cargo test -p concord_examples --test spec_policy_blocks
cargo test -p concord_examples --test auth_core
cargo test -p concord_examples --test auth_dsl
cargo test -p concord_examples --test ui
cargo test -p concord_examples --test retry_dsl
cargo test -p concord_examples --test rate_limit_dsl
cargo test -p concord_examples --test cache_dsl
cargo test -p concord_examples --test pagination
```

Run feature-specific cache tests when touching cache.

```powershell
cargo test -p concord_core --features cache-moka
cargo test -p concord_examples --features cache-moka --test cache_dsl
```

Run full validation before merging broad changes.

```powershell
cargo fmt --check
cargo check -p concord_core --no-default-features
cargo check -p concord_core --all-features
cargo check -p concord_examples --no-default-features --tests
cargo test --workspace
cargo test -p concord_examples --tests
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Cache DSL tests require the `cache-moka` feature:

```powershell
cargo test -p concord_examples --features cache-moka --test cache_dsl
```

## Test design guidance

Write behavior tests, not macro implementation tests. Assert the generated request and runtime behavior, not the internal token stream.

For a new DSL feature, add example/tests first, then core behavior, then macro parsing and codegen. That keeps DSL syntax aligned with runtime capability.
