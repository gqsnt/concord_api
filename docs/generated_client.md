# Generated Client

The generated client is facade-first. Normal callers construct the client, navigate scopes with methods, call an endpoint method, optionally set request parameters, and then await or execute the pending request.

## Construct A Client

Clients with no required variables use `new()`.

```rust
let api = minimal_api::MinimalApi::new();
```

Clients with required variables or secrets take those values as constructor arguments in declaration order.

```rust
let api = session_api::SessionApi::new("upstream-key".to_string());
```

Constructor order is stable: ordinary `var` inputs come first in source declaration order, followed by auth vars/secrets in source declaration order. Adding optional endpoint auth does not reorder existing constructor arguments.

Tests and custom transports can use `new_with_transport(...)`.

```rust
let api = minimal_api::MinimalApi::new_with_transport(transport);
```

Generated clients inherit Concord's runtime response-body limit. Endpoint responses are read under a finite 16 MiB default before decode. Advanced callers can adjust this with `configure(|cfg| cfg.max_response_body_bytes(bytes))`; `no_response_body_limit()` disables the endpoint read limit explicitly.

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

Every optional/defaulted field has three setter forms:

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

Use it when a method chain reads better with an explicit terminal operation.

## Execute Decoded

`.execute_decoded()` returns value plus response metadata.

```rust
let response = api.users().get_user(42).execute_decoded().await?;
let status = response.status();
let headers = response.headers();
let url = response.url();
let meta = response.meta();
let user = response.into_value();
```

Use this for status/header assertions, logging, or request metadata inspection.

## Raw Execution

`.execute_raw()` returns the classified raw response before endpoint decoding.

```rust
let raw = api.users().get_user(42).execute_raw().await?;
```

This is an advanced escape hatch for diagnostics and protocol tests.

## Pagination

Paginated endpoints require an explicit `.paginate()` call.

```rust
let items = api
    .items()
    .list()
    .paginate()
    .max_items(1_000)
    .collect()
    .await?;
```

Use `for_each_page` for bounded-memory processing.

```rust
api.items()
    .list()
    .paginate()
    .for_each_page(|page| async move {
        println!("status={} items={}", page.status(), page.value().len());
        Ok(())
    })
    .await?;
```

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

Auth state accessors expose explicit credential checks and clearing.

```rust
if api.auth_state().session().is_set().await? {
    api.auth_state().session().clear().await?;
}
```

Generated auth state helpers are fallible when they observe shared auth state. Lock/state failures return `AuthError` instead of panicking.

## Advanced Endpoints

The facade is the normal API. Advanced callers can construct endpoint values from `endpoints::*` and pass them to `request(...)`.

```rust
let endpoint = example_api::endpoints::GetUser::new(42);
let user = api.request(endpoint).execute().await?;
```

Use advanced endpoints for focused tests, reusable endpoint values, or explicit request planning. Keep normal application code on the facade where possible.

## Public Name Stability

Generated public names are validated before codegen within their generated namespace. Client facade names are checked against generated client methods such as `new`, `new_with_transport`, `builder`, `configure`, `request`, and `auth_state`. Endpoint-backed auth helper names, auth-state credential accessors, scope facade methods, endpoint methods, generated request-extension traits, endpoint marker types, and support types are also collision-validated in their own namespaces.

Raw Rust identifiers such as `r#type` are rejected for public generated names in v1. Use ordinary DSL names or aliases that generate stable public Rust names.

Endpoint-backed credentials expose deterministic acquisition helpers such as `.acquire_as_session()` on the endpoint request that returns credential material. Stored credential state is accessed through `api.auth_state().session().set(...)`, `.clear()`, and `.is_set()`.

## Rustdoc And Autocomplete

Generated endpoint methods and endpoint structs include rustdoc with:

- HTTP method and path pattern
- required parameters
- query and header names
- auth summary
- cache, retry, and rate-limit summary
- pagination controller
- body and response codecs

Generated setters document whether a field is a path, query, header, or request parameter and whether clearing removes an optional value or resets a default.
