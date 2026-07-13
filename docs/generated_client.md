# Generated Client

The generated client is facade-first. Normal callers construct the client, navigate scopes with methods, call an endpoint method, optionally set request parameters, and then await or execute the pending request.

See [Security Model](security_model.md) for the boundary between safe generated-client use, advanced extension points, and dangerous escape hatches.

## Construct A Client

Clients with no required variables use `new()`.

```rust
let api = minimal_api::MinimalApi::new();
```

Clients with required variables or secrets take those values as constructor arguments in source declaration order: ordinary `var` declarations first, then auth `secret` declarations.

```rust
let api = session_api::SessionApi::new("upstream-key".to_string());
```

For example, a client declaring `var tenant`, `var region`, then auth secrets `username` and `password` is constructed as `Example::new(tenant, region, username, password)`. For clients with several same-typed values, the builder API is often clearer.

Callers can use `new()`, `builder()`, or `new_with_safe_reqwest_builder(...)`. The safe managed constructors expose only `SafeReqwestBuilder`; they never expose raw Reqwest builders, clients, or proxies. Use the fallible variant when parsing trusted-root or client-identity PEM data. Generated clients also expose validated client-wide API-header methods. All feature profiles execute through the managed Reqwest client.

```rust
let api = minimal_api::MinimalApi::new();
```

Concord's managed reqwest transport disables redirects and Reqwest retries. That keeps auth material on the original request and leaves endpoint retries under Concord's runtime. `new_with_safe_reqwest_builder(...)` receives only `SafeReqwestBuilder` for infallible settings; `new_with_safe_reqwest_builder_fallible(...)` additionally permits fallible PEM parsing. Both support reviewed timeout, pool, protocol, TLS, and credential-free explicit-proxy settings while keeping Reqwest defaults, cookies, redirects, and retries unavailable.

When you configure runtime hooks or debug sinks, those callbacks receive sanitized metadata views. Sensitive request and response headers and sensitive query values are redacted before callback invocation, and neither surface receives request or response body bytes or raw secret material. High-volume debug can add measurable overhead.

Generated clients inherit Concord's runtime response-body limit. Endpoint responses are read under a finite 16 MiB default before decode. Advanced callers can adjust this with `configure(|cfg| cfg.max_response_body_bytes(bytes))`; `no_response_body_limit()` disables the endpoint read limit explicitly.

Generated Rustdoc describes defaulted setters as declared-default controls without rendering the source default expression value. The runtime behavior is unchanged: defaulted values still apply, and `Option` setters still reset to the declared default.

Use `builder()` when constructing a client is clearer with named setters.

```rust
let api = ExampleApi::builder()
    .tenant("acme".to_string())
    .api_key("secret".to_string())
    .build()?;
```

## Navigate The Facade

Scopes are methods. Endpoint aliases become the endpoint facade method names.

```rust
let user = api.users().get_user(42).await?;
```

Scope parameters are passed when entering the scope.

```rust
let summoner = riot
    .platform(PlatformRoute::EUW1)
    .summoner_v4()
    .by_puuid(puuid)
    .await?;
```

## Required Arguments

Required scope and endpoint parameters are direct method arguments.

```rust
api.users().get_user(user_id)
```

Dynamic path parameters are part of that rule too: required dynamic path values must stringify to a non-empty segment, optional dynamic path values reject `Some("")`, and optional `None` still omits the segment where supported. The empty-string rule does not apply to query parameters.

A request body is also a direct endpoint method argument.

```rust
api.posts().create(CreatePost {
    title: "hello".to_string(),
    body: "body".to_string(),
    user_id: 1,
})
```

## Optional And Defaulted Setters

Optional and defaulted endpoint parameters use fluent setters on the pending request.

```rust
let users = api
    .users()
    .search()
    .q("ada".to_string())
    .count(50)
    .await?;
```

Every optional or defaulted field has three setter forms:

```rust
request.field(value)
request.field_opt(optional_value)
request.clear_field()
```

Use `field_opt(None)` or `clear_field()` to clear an optional field. For defaulted fields, those forms reset the field to its declared default.

## Await

A pending request can be awaited directly. This returns the decoded endpoint value.

```rust
let user: User = api.users().get_user(42).await?;
```

## Execute

`.execute()` is the explicit equivalent of direct await.

```rust
let user = api.users().get_user(42).execute().await?;
```

## Response Metadata

`.response()` returns a decoded value plus response metadata for buffered endpoint requests.

```rust
let response = api
    .users()
    .get_user(42)
    .response()
    .await?;
let status = response.status();
let headers = response.headers();
let url = response.url();
let meta = response.meta();
let user = response.into_value();
```

## Dangerous Raw Response

`#[cfg(feature = "dangerous-raw-response")]` enables `concord_core::dangerous::BuiltResponse` and `.execute_raw_response()`, which returns the classified raw response before endpoint decoding. It still observes the configured response-body limit.

```rust
#[cfg(feature = "dangerous-raw-response")]
let raw = api.users().get_user(42).execute_raw_response().await?;
```

This is a dangerous escape hatch for diagnostics and protocol tests.

## Pagination

Paginated endpoints require an explicit `.paginate(...)` call with a termination policy.

```rust
use concord_core::prelude::PaginationTermination as PageUntil;

let items = api
    .items()
    .list()
    .paginate(PageUntil::hard_item_cap(1_000))
    .collect()
    .await?;
```

`collect()` is the supported high-level pagination surface.

## Auth Acquisition

Endpoint-backed credentials are acquired from the endpoint request that produces the credential value.

```rust
api.auth_api()
    .login_for_session(LoginRequest {
        username: "ada".to_string(),
        password: "secret".to_string(),
    })
    .acquire_as_session()
    .await?;
```

Protected calls that require that credential can then run normally.

```rust
let me = api.protected().me().await?;
```

Auth-state accessors expose explicit credential checks and clearing.

```rust
if api.auth_state().session().is_set().await? {
    api.auth_state().session().clear().await?;
}
```

Generated auth-state helpers are fallible when they observe runtime auth state. Lock and state failures return `AuthError` instead of panicking.

Cloned clients share auth state. Runtime configuration uses clone-on-write, but auth-state accessors, `set`, `clear`, `is_set`, and endpoint-backed acquisition operate on the shared auth-state handle. Clearing or replacing auth state on one clone affects other clones that share that handle. If credential isolation matters, create a separate client instance or install separate auth state instead of relying on `clone()`.

## Advanced Endpoints

The facade is the normal API. Advanced callers can construct endpoint values from `endpoints::*` and pass them to `request(...)`.

```rust
let endpoint = example_api::endpoints::GetUser::new(42);
let user = api.request(endpoint).execute().await?;
```

Use advanced endpoints for focused tests, reusable endpoint values, or explicit request planning.

## Public Name Stability

Generated public names are validated before codegen within their generated namespace. Client facade names are checked against generated client methods such as `new`, `builder`, `configure`, `request`, and `auth_state`. Endpoint-backed auth helper names, auth-state credential accessors, scope facade methods, endpoint methods, generated request-extension traits, endpoint marker types, and support types are also collision-validated in their own namespaces.

Raw Rust identifiers such as `r#type` are rejected for public generated names in v1. Use ordinary DSL names or aliases that generate stable public Rust names.

Endpoint-backed credentials expose deterministic acquisition helpers such as `.acquire_as_session()` on the endpoint request that returns credential material. Stored credential state is accessed through `api.auth_state().session().set(...)`, `.clear()`, and `.is_set()`.

## Rustdoc And Autocomplete

Generated endpoint methods and endpoint structs include rustdoc with:

- an effective contract summary derived from resolved semantics
- HTTP method, resolved path, and base identity
- resolved auth attachments after client default, profile, scope, and endpoint resolution
- response entity/output type and the applicable terminal method
- buffered metadata access when `.response().await` is available
- retry and rate-limit summaries with bounded resolved details
- pagination controller and collect-only usage when present
- request body summary and replayability when relevant
- names and metadata only; secret values and raw body bytes are not rendered

Generated setters document whether a field is a path, query, header, or request parameter and whether clearing removes an optional value or resets a default.
