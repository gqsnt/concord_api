# Generated client

The generated API mirrors the source domain tree and exposes only contract-valid capabilities.

## Declaration-to-usage law

```text
client declaration     → client builder and built client
scope declaration      → immutable selected scope
endpoint declaration   → inert typed request
unbound root           → same tree without runtime/security attachment
terminal capability    → inherent method only on matching request
```

## Client construction

```rust
let client = CatalogApi::builder()
    .set_tenant(tenant)
    .set_locale(locale_from_config)
    .bootstrap_key(secret)
    .retry(retry_policy)
    .rate_limiter(shared_limiter)
    .configure_http(|http| {
        http.require_https();
    })
    .configure_provider_http(|provider| {
        provider.response_body_limit(provider_limit);
    })
    .configure_runtime(|runtime| {
        runtime.pagination_max_pages(max_pages);
    })
    .build()?;
```

`new()` is generated only when no required value, secret/provider, limiter, or other construction decision remains.

A client with effective rate policies cannot build without an explicit limiter choice such as a supplied limiter, explicit local limiter, or explicitly named test no-op.

## Scope snapshots

```rust
let regional = client
    .regional(region)
    .set_language(language_from_config)
    .set_language(Some(Language::Fr))
    .omit_language();
```

Scopes are inert immutable snapshots. They expose controls only for values they own and navigation only to their children.

## Endpoint construction

Required non-initialized ordinary values and required bodies are direct arguments in source order. Optional/defaulted values become controls.

```rust
let request = regional
    .item_summaries(tags)
    .set_category(category_from_config)
    .set_category(Some(category))
    .omit_category()
    .set_count(PageSize::new(100)?)
    .reset_count();
```

Do not generate generic path/query/header/body/auth/native mutation methods for declared endpoints.

## Value-control matrix

| Declaration | Construction | Controls |
|---|---|---|
| `x: T` | required argument/builder field | none after scope/endpoint construction |
| `x?: T` | initially absent | `set_x(Option<T>)`, `omit_x()` |
| `x: T = V` / `#[default] x: T` | retained initialized value | `set_x(T)`, `reset_x()` |
| initialized `x?: T` | retained present default | `set_x(Option<T>)`, `omit_x()`, `reset_x()` |

For `x?: Option<U>`, `set_x` accepts `Option<Option<U>>`, and `omit_x` selects the outer absence. The smaller exact surface avoids a second assignment operation that can obscure outer Concord presence.

## Ordinary terminals

Every endpoint request supports:

```rust
let output: Output = request.await?;
let response: ApiResponse<Output> = request.response().await?;
let plan = request.plan()?;
let description = request.description();
```

`ApiResponse<T>` is a managed accepted response with bounded explicit metadata access. It is not a native response escape.

## Capability-specific terminals

Generated only when the contract proves the capability:

```rust
request.accepted_bytes().await?;
request.paginate(PageLimit::CompleteWithin(limit)).await?;
request.store_as_session().await?;
```

- `accepted_bytes` is endpoint-declared and bypasses representation validation/conversion while retaining auth, status, negotiation, and finite acquisition.
- `paginate` exists only for a safe pageable complete response.
- `store_as_*` exists only on a declared endpoint-backed credential source and returns `Result<(), ApiError>`.

## Streams

```rust
let bytes: ByteStream = client.download(id).await?;
let items: ItemStream<Event> = client.watch().await?;
```

The initial operation completes when the accepted stream handle is acquired. Later failures belong to the stream. Neither stream type is `Clone` without real replay/multicast semantics.

## Unbound generated intents

```rust
let intent = CatalogApi::unbound()
    .regional(region)
    .item_summaries(tags)
    .set_category(Some(category));

let plan = client.plan(&intent)?;
let request = client.request(intent); // statically matching and infallible
```

The unbound tree uses the same navigation, values, representation, and capabilities. It carries no client runtime, credentials, limiter, observer, or operation identity.

Binding a generated intent through another client type is a compile-time error.

## Runtime endpoints

Runtime endpoints use an explicit fallible path:

```rust
let request = runtime_endpoint.try_bind(&client_or_scope)?;
```

Do not weaken generated binding merely to unify it with dynamic validation.

## Errors

Generated docs should teach callers to combine stable category and dispatch knowledge:

```rust
match request.await {
    Ok(value) => use_value(value),
    Err(error) if error.dispatch_knowledge() == DispatchKnowledge::PossiblyDispatched => {
        reconcile(error.operation_id())
    }
    Err(error) => handle_by_category(error.category()),
}
```

No universal `is_retryable()` can answer application workflow, effect, body replay, deadline, budget, and remote uncertainty at once.
