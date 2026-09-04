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

<!-- concord-source: crates/concord-examples/src/contracts/full_dsl.rs#configured_client -->
```rust
    let client = ReferenceApi::builder()
        .set_tenant("tenant-a".to_owned())
        .set_locale(Some("fr".to_owned()))
        .omit_locale()
        .reset_locale()
        .set_page_size(50)
        .reset_page_size()
        .set_client_label(Some("client-override"))
        .omit_client_label()
        .reset_client_label()
        .api_key(
            concord::SecretString::try_new("example-api-key").expect("bounded example API key"),
        )
        // This construction-only example selects an explicit test limiter so a
        // rate-declaring client can never build through an implicit bypass.
        .configure_runtime(|runtime| runtime.rate_limiter(Arc::new(NoopRateLimiter::new())))
        .build()?;
```

`new()` is generated only when no required value, secret/provider, limiter, or other construction decision remains.

A client with effective rate policies cannot build without an explicit limiter choice such as a supplied limiter, explicit local limiter, or explicitly named test no-op.

## Scope snapshots

<!-- concord-source: crates/concord-examples/src/contracts/full_dsl.rs#direct_and_explicit -->
```rust
    let _: reference_api::regions::FetchRequest = client
        .regions("eu-west".to_owned())
        .set_language(Some("fr".to_owned()))
        .set_scope_label(Some("scope-override"))
        .omit_scope_label()
        .reset_scope_label()
        .fetch("42".to_owned(), vec!["stable".to_owned()]);
    let endpoint = ReferenceApi::unbound()
        .regions("eu-west".to_owned())
        .set_language(Some("fr".to_owned()))
        .set_scope_label(Some("scope-override"))
        .omit_scope_label()
        .reset_scope_label()
        .fetch("42".to_owned(), vec!["stable".to_owned()]);
    let _: reference_api::regions::FetchRequest = client.request(endpoint);
```

Scopes are inert immutable snapshots. They expose controls only for values they own and navigation only to their children.

## Endpoint construction

Required non-initialized ordinary values and required bodies are direct arguments in source order. Optional/defaulted values become controls.

<!-- concord-source: crates/concord-examples/src/contracts/full_dsl.rs#request_controls -->
```rust
    let _: reference_api::StatusRequest = client
        .status()
        .set_verbose(Some(true))
        .omit_verbose()
        .reset_verbose()
        .set_tags(Some(vec!["stable", "documented"]))
        .omit_tags()
        .reset_tags();
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

<!-- concord-source: crates/concord-examples/src/request_response/acquisition.rs#metadata_response -->
```rust
/// Returns the decoded value together with accepted response metadata.
pub async fn with_metadata(client: &ResponseApi) -> Result<ApiResponse<String>, ApiError> {
    client.message().response().await
}
```

`ApiResponse<T>` is a managed accepted response with bounded explicit metadata access. It is not a native response escape.

## Capability-specific terminals

Generated only when the contract proves the capability:

<!-- concord-source: crates/concord-examples/src/contracts/catalog_contract.rs#catalog_planning -->
```rust
    let _plan = request.plan()?;
    let _pagination = request.paginate(PageLimit::CompleteWithin(
        NonZeroU32::new(20).expect("positive"),
    ));
    let _accepted: concord::endpoint::AcceptedBytesRequest<_> =
        regional.item("item-42".to_owned()).accepted_bytes();

    let endpoint = CatalogApi::unbound()
        .regional("eu-west".to_owned())
        .item("item-42".to_owned());
    let _description = endpoint.description();
    let _unbound_plan = client.plan(&endpoint)?;
    let request = client.request(endpoint);
    let _response: concord::endpoint::ResponseRequest<_> = request.response();
    let _metadata_shape: Option<ApiResponse<Bytes>> = None;
```

- `accepted_bytes` is endpoint-declared and bypasses representation validation/conversion while retaining auth, status, negotiation, and finite acquisition.
- `paginate` exists only for a safe pageable complete response.
- `store_as_*` exists only on a declared endpoint-backed credential source and returns `Result<(), ApiError>`.

## Streams

<!-- concord-source: crates/concord-examples/src/request_response/records.rs#consume_records -->
```rust
pub async fn read_csv_rows(
    client: &RecordsApi,
) -> Result<Vec<(String, u32)>, Box<dyn std::error::Error>> {
    let mut stream = client.csv_stream().await?;
    let mut rows = Vec::new();
    while let Some(row) =
        std::future::poll_fn(|context| Pin::new(&mut stream).poll_next(context)).await
    {
        rows.push(row?);
    }
    Ok(rows)
}
```

The initial operation completes when the accepted stream handle is acquired. Later failures belong to the stream. Neither stream type is `Clone` without real replay/multicast semantics.

## Unbound generated intents

<!-- concord-source: crates/concord-examples/src/contracts/full_dsl.rs#direct_and_explicit -->
```rust
    let _: reference_api::regions::FetchRequest = client
        .regions("eu-west".to_owned())
        .set_language(Some("fr".to_owned()))
        .set_scope_label(Some("scope-override"))
        .omit_scope_label()
        .reset_scope_label()
        .fetch("42".to_owned(), vec!["stable".to_owned()]);
    let endpoint = ReferenceApi::unbound()
        .regions("eu-west".to_owned())
        .set_language(Some("fr".to_owned()))
        .set_scope_label(Some("scope-override"))
        .omit_scope_label()
        .reset_scope_label()
        .fetch("42".to_owned(), vec!["stable".to_owned()]);
    let _: reference_api::regions::FetchRequest = client.request(endpoint);
```

The unbound tree uses the same navigation, values, representation, and capabilities. It carries no client runtime, credentials, limiter, observer, or operation identity.

Binding a generated intent through another client type is a compile-time error.

## Runtime endpoints

Runtime endpoints use an explicit fallible path:

<!-- concord-source: crates/concord-examples/src/runtime_endpoints/generated_and_runtime.rs#runtime_endpoint -->
```rust
pub fn construct() -> Result<(), Box<dyn std::error::Error>> {
    let client = PreparedApi::builder().build()?;
    let endpoint = RuntimeEndpoint::builder(EndpointId::new("prepared_api.lookup")?, Method::POST)
        .path("records")
        .path("42")
        .query("view", "summary")
        .header(HeaderName::from_static("x-example"), "canonical")
        .timeout(Duration::from_secs(5))
        .body_codec::<crate::request_response::specifications::ExampleCodec>(
            "request body".to_owned(),
        )
        .response::<crate::request_response::specifications::ExampleCodec, String>()
        .build()?;
    let _: Request<_> = endpoint.try_bind(&client)?;
    Ok(())
}
```

Do not weaken generated binding merely to unify it with dynamic validation.

## Errors

Generated docs should teach callers to combine stable category and dispatch knowledge:

```text
match request.await {
    Ok(value) => use_value(value),
    Err(error) if error.dispatch_knowledge() == DispatchKnowledge::PossiblyDispatched => {
        reconcile(error.operation_id())
    }
    Err(error) => handle_by_category(error.category()),
}
```

No universal `is_retryable()` can answer application workflow, effect, body replay, deadline, budget, and remote uncertainty at once.
