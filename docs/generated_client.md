# Generated client

`api!` emits ordinary, nominal Rust types in your crate: a client, a builder,
one handle per scope, one named request and one unbound intent per endpoint,
and a credential facade. They own core bindings through private fields and
provide inherent, endpoint-specific methods. This page lists every generated
name; the value semantics are normative in the [DSL specification](spec.md).

## Names and modules

For `client CatalogApi` with `scope regional` and `GET item_summaries`
(§7.1):

| Declaration | Generated |
|---|---|
| `client CatalogApi` | `CatalogApi` (the client) and the support module `catalog_api` |
| builder | `catalog_api::CatalogApiBuilder` |
| unbound root | `catalog_api::Unbound` |
| credential facade | `catalog_api::Credentials` |
| `scope regional` | `catalog_api::regional::Regional` (handle) in module `catalog_api::regional` |
| `GET item_summaries` in `regional` | `catalog_api::regional::ItemSummariesRequest` (bound request) and `ItemSummariesEndpoint` (unbound intent) |

Nested scopes create nested modules. The `Request`/`Endpoint` suffixes denote
bound request versus unbound intent, never response DTOs. Snake case inserts
`_` before an uppercase letter that follows a lowercase letter or digit and
before the last uppercase letter of an acronym followed by lowercase; raw
markers are removed first. Ambiguous generated symbols are rejected with
`C3-GENERATED`, naming both origins; nothing is ever suffixed, truncated or
renamed. A request-local parameter named `timeout` becomes `set_timeout`, not a
bare `timeout` method.

## Building a client

`CatalogApi::builder()` returns the builder. Declaration defaults are evaluated
once, when the builder is created. The builder has:

- the root value controls (`set_x`, `set_x_if_some`, and for optional or
  initialized values `replace_x`, `omit_x`, `reset_x`, per the matrix below);
- one setter per secret, named after the secret, taking `SecretString`
  (`.bootstrap_key(secret)`);
- one setter per custom provider, named after the credential, taking the
  declared provider type (`.oauth(provider)`);
- `configure_http(|http| ...)` and `configure_runtime(|runtime| ...)` over the
  validated `HttpConfigBuilder` and `RuntimeConfigBuilder` facades (see
  [Runtime](runtime.md));
- `retry(policy)`, which installs a `RetryPolicy`;
- `build()`, which returns `Result<CatalogApi, ClientBuildError>`.

`build()` accumulates every missing value, secret and provider and every invalid
configuration into one `ClientBuildError` naming setters, never values. It
performs no network access, provider acquisition or remote registration. A
client with any reachable rate policy requires an explicit limiter
(`runtime.rate_limiter(...)`); there is no implicit no-op fallback.
`CatalogApi::new()` exists only when nothing is required, and delegates to the
same validation.

<!-- concord-source: crates/concord-examples/src/application/construction.rs#build_catalog -->
```rust
/// Builds the canonical Catalog client for production use.
///
/// # Errors
///
/// One [`ClientBuildError`] accumulating every invalid or missing input.
pub fn build_catalog(
    tenant: String,
    bootstrap: SecretString,
) -> Result<CatalogApi, ClientBuildError> {
    catalog_builder(tenant, bootstrap).build()
}

/// The configured builder behind [`build_catalog`].
///
/// A deployment (or a loopback test) can layer further trusted transport
/// configuration, such as a proxy or private trust root, with one more
/// `configure_http` call before `build`; the closures apply in call order.
#[must_use]
pub fn catalog_builder(tenant: String, bootstrap: SecretString) -> catalog_api::CatalogApiBuilder {
    CatalogApi::builder()
        .set_tenant(tenant)
        .bootstrap_key(bootstrap)
        .configure_http(|http| http.require_https().connect_timeout(Duration::from_secs(3)))
        .configure_runtime(|runtime| {
            runtime
                .rate_limiter(Arc::new(LocalRateLimiter::new()))
                .response_body_limit(RESPONSE_BODY_LIMIT)
                .pagination_max_pages(MAX_PAGES)
        })
}
```

## The client

A built client is immutable; `Clone` shares its services, credential stores and
snapshots.

| Method | Purpose |
|---|---|
| `CatalogApi::contract()` | `&'static ContractDescription`: the complete effective contract, without a client, secret or network |
| `CatalogApi::unbound()` | the unbound navigation root |
| `client.request(intent)` | binds an unbound intent to this client, returning its named request |
| `client.plan(&intent)` | plans an unbound intent against this client |
| `client.credentials()` | the local credential facade (`clear_x()`, `replace_x(material)`) |
| `client.bind(runtime_endpoint)` / `client.plan_runtime(&runtime_endpoint)` | runtime endpoints at the root owner ([Runtime endpoints](runtime_endpoints.md)) |
| `client.regional(region)` | selects a scope, evaluating its defaults once |
| `client.authenticate(input)` | constructs a root endpoint's named request |

Binding an intent to the wrong client is a type error; runtime binding is
fallible.

## Scope handles

`client.regional(region)` returns an owned `Regional` handle that does not
borrow the client expression. Required uninitialized scope parameters are
positional (`impl IntoValue<T>`); other scope values use controls. A scope value
control consumes the handle and returns a new snapshot, so requests created
earlier keep the old values. Handles are `Clone` and also have `bind` and
`plan_runtime` for runtime endpoints bound at the scope.

## Named requests

`regional.item_summaries(tags)` returns an owned
`catalog_api::regional::ItemSummariesRequest`. Required uninitialized endpoint
parameters and a required body are positional, in declaration order (a body
takes its exact type). Every named request has:

| Method | Purpose |
|---|---|
| `.await` (`IntoFuture`) | executes one operation, resolving to `Result<Output, ApiError>` |
| `response()` | `ResponseRequest<Output>`, resolving to `ApiResponse<Output>` |
| `plan()` | `Result<RequestPlan, ApiError>`, inert |
| `description()` | the static `&'static EndpointDescription` |
| `timeout(d)` | selects a finite timeout: `Result<Self, ControlError>` |
| `reset_timeout()` | restores the declared/default timeout |
| `cancel_with(token)` | adds a cancellation signal (at most 8): `Result<Self, ControlError>` |
| `with_options(&options)` | intersects `CallOptions` caps |
| `control_summary()` | the `ControlSummary` of declared/selected/effective controls |

Capability terminals exist only when declared (CALL-03):

| Declared by | Terminal |
|---|---|
| `expose accepted_bytes;` | `accepted_bytes()` returning `AcceptedBytesRequest` |
| `paginate via ...` | `pages(n)`, `collect_prefix(n)`, `collect_all(NonZeroU32)` |
| being the source of `credential x = endpoint path` | `store_as_x()` returning `StoreRequest` |

The named request implements the open `RequestControls` trait itself, so
generic helpers work across endpoints (§7.2). Implementing `RequestControls`
grants no native request, binding, credential or executor access. The terminal
wrappers `ResponseRequest`, `AcceptedBytesRequest`, `StoreRequest`,
`CollectPrefix` and `CollectAll` implement it too; an already-constructed
`Pages` iterator and acquired streams carry the controls captured from the
request instead.

<!-- concord-source: crates/concord-examples/src/application/execute.rs#execute_helper -->
```rust
use std::future::IntoFuture;

use concord::{ApiError, CallOptions, RequestControls};

/// Applies the caller's additional caps and awaits the request.
///
/// # Errors
///
/// Whatever the request's own terminal returns.
pub async fn execute<R, T>(request: R, options: &CallOptions) -> Result<T, ApiError>
where
    R: RequestControls + IntoFuture<Output = Result<T, ApiError>>,
{
    request.with_options(options).await
}
```

## Value controls

Each input's controls follow its declared shape, never its Rust spelling:

| Shape after construction | `set_x` | `replace_x(Option<T>)` | `set_x_if_some(Option<T>)` | `omit_x` | `reset_x` |
|---|:-:|:-:|:-:|:-:|:-:|
| Required, uninitialized | No | No | No | No | No |
| Initialized required (`x: T = V`, `#[default] x: T`) | Payload | No | Yes | No | Yes |
| Uninitialized optional (`x?: T`) | Payload | Yes | Yes | Yes | No |
| Initialized optional (`x?: T = V`, `#[default] x?: T`) | Payload | Yes | Yes | Yes | Yes |
| Required body | No | No | No | No | No |
| Optional body | Exact payload | Yes | Yes | Yes | No |

On the root builder, required values additionally have `set_x` and
`set_x_if_some`. Payload setters take `impl IntoValue<T>` (identity,
`&str -> String`, `&String -> String` only); `replace_x` and `set_x_if_some`
take exact `Option<T>`, so `None` has one target type. `replace_x(None)` is
exact absence; `set_x_if_some(None)` is "no new opinion"; `reset_x()` restores
the retained initializer without evaluating it again. Controls are grouped by
intent in rustdoc and repeat the author's value documentation.

*Construction-only:*

<!-- concord-source: crates/concord-examples/src/contracts/full_dsl.rs#configured_client -->
```rust
let client = ReferenceApi::builder()
    .set_tenant("tenant-a")
    .replace_locale(Some("fr".to_owned()))
    .omit_locale()
    .reset_locale()
    .set_page_size(50)
    .reset_page_size()
    .set_client_label("client-override")
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

<!-- concord-source: crates/concord-examples/src/contracts/full_dsl.rs#request_controls -->
```rust
let _: reference_api::StatusRequest = client
    .status()
    .set_verbose(true)
    .omit_verbose()
    .reset_verbose()
    .set_tags(vec!["stable", "documented"])
    .omit_tags()
    .reset_tags();
```

Saved state versus command-line opinions, over the canonical Catalog:

<!-- concord-source: crates/concord-examples/src/application/saved_state.rs#saved_state -->
```rust
use std::num::NonZeroU32;

use concord::{ApiError, CallOptions};

use super::execute::execute;
use crate::contracts::catalog_v3::{ItemSummary, catalog_api};

/// A list configuration the application saved earlier.
pub struct SavedList {
    /// Saved category; `None` is an explicit "no category" choice.
    pub category: Option<String>,
    /// Saved page size.
    pub count: NonZeroU32,
}

/// Command-line overrides; each `None` means "no new opinion".
#[derive(Default)]
pub struct CliListPatch {
    /// Category override.
    pub category: Option<String>,
    /// Page-size override.
    pub count: Option<NonZeroU32>,
}

/// Builds the inert list request: saved state first, then CLI opinions.
pub fn list_request(
    scope: &catalog_api::regional::Regional,
    saved: SavedList,
    cli: CliListPatch,
) -> catalog_api::regional::ItemSummariesRequest {
    scope
        .item_summaries(Vec::new())
        .replace_category(saved.category)
        .set_count(saved.count)
        .set_category_if_some(cli.category)
        .set_count_if_some(cli.count)
}

/// Executes one configured list page under the caller's limits.
///
/// # Errors
///
/// The single request's failure.
pub async fn configured_list(
    scope: &catalog_api::regional::Regional,
    saved: SavedList,
    cli: CliListPatch,
    limits: &CallOptions,
) -> Result<Vec<ItemSummary>, ApiError> {
    let request = list_request(scope, saved, cli);
    execute(request, limits).await
}
```

## Unbound intents

`CatalogApi::unbound()` offers the scope and endpoint navigation tree without
root-value setters. Intents capture scope and endpoint snapshots; the selected
client supplies its root values and services at binding, without re-evaluating
retained defaults. Direct and unbound+bind requests produce equivalent plans.

<!-- concord-source: crates/concord-examples/src/contracts/full_dsl.rs#direct_and_explicit -->
```rust
let _: reference_api::regions::FetchRequest = client
    .regions("eu-west")
    .set_language("fr")
    .set_scope_label("scope-override")
    .omit_scope_label()
    .reset_scope_label()
    .fetch("42", vec!["stable".to_owned()]);
let endpoint = ReferenceApi::unbound()
    .regions("eu-west")
    .set_language("fr")
    .set_scope_label("scope-override")
    .omit_scope_label()
    .reset_scope_label()
    .fetch("42", vec!["stable".to_owned()]);
let _: reference_api::regions::FetchRequest = client.request(endpoint);
```

## Cloning

Clients, scope handles, body-free requests and body-free intents are `Clone`
without requiring the output type to be `Clone`. Each executed clone is a new
operation and grants no retry authority. A request or intent whose declaration
has a body slot is not `Clone`, even when the payload is cloneable or an
optional body is absent: keep a scope and a request factory. In-flight futures,
pages, streams and storage futures are never `Clone`.

## Facade exports

The facade re-exports at its root (§7.2): `api!`, `ApiError`,
`ClientBuildError`, `ControlError`, `IntoValue`, `RequestControls`,
`CallOptions`, `ControlSummary`, `CancellationToken`, `SecretString`,
`StoreOutcome`, `DispatchKnowledge`, `OperationId`, `ExecutionMeta`, `Bytes`,
`ByteStream`, and the enabled first-party representations `Json` (feature
`json`), `Binary`, `Text`, `Empty` and `Discard`. Detailed roles live under
`concord::body`, `concord::response`, `concord::pagination`, `concord::auth`,
`concord::rate_limit` (including `rate_limit::LocalRateLimiter` with the
`rate-limit` feature), `concord::retry` and `concord::inspection`.
`concord::prelude` gathers the common declaration imports.

## Generated rustdoc

Generated documentation is real rustdoc on the methods you call (§2.3, §19.2).
An endpoint constructor and its named request show, in order:

1. the author's `///` description, and `#[deprecated]` where calling triggers
   it;
2. the required inputs (the positional arguments) and endpoint values with
   presence, sensitivity and their first doc line;
3. a contract card: the operation with its effective route template and
   endpoint ID (explicit or navigation-derived), the result and representation,
   accepted statuses, the expected-absence status (head-only, body dropped
   unread), effect and its source, the effective timeout, each effective
   authentication placement with its challenge and origin, each rate policy
   with its key labels and cost, and available pagination, accepted-bytes and
   `store_as_x()` terminals;
4. a link to the complete effective contract, `CatalogApi::contract()`.

Value controls carry the value's description and a one-line meaning of the
control (payload set, exact replacement, conditional set, omission or reset).
The builder names every required input. A macro unit test checks the generated
docs of the canonical Catalog's `find_item` against this list.

## The canonical Catalog signatures

The [modeling guide](modeling_apis.md)'s Catalog requires these signatures
(§22.3):

```text
CatalogApiBuilder::set_tenant(self, impl IntoValue<TenantId>) -> Self
CatalogApiBuilder::set_locale(self, impl IntoValue<Locale>) -> Self
CatalogApiBuilder::replace_locale(self, Option<Locale>) -> Self
CatalogApiBuilder::set_locale_if_some(self, Option<Locale>) -> Self
CatalogApiBuilder::omit_locale(self) -> Self
CatalogApiBuilder::set_api_version(self, impl IntoValue<ApiVersion>) -> Self
CatalogApiBuilder::reset_api_version(self) -> Self
CatalogApiBuilder::bootstrap_key(self, SecretString) -> Self
CatalogApi::authenticate(&self, LoginInput) -> AuthenticateRequest
AuthenticateRequest::store_as_session(self) -> StoreRequest
CatalogApi::regional(&self, impl IntoValue<Region>) -> Regional
Regional::set_language(self, impl IntoValue<Language>) -> Self
Regional::replace_language(self, Option<Language>) -> Self
Regional::item_summaries(&self, impl IntoValue<Vec<Tag>>) -> ItemSummariesRequest
ItemSummariesRequest::set_category(self, impl IntoValue<Category>) -> Self
ItemSummariesRequest::replace_category(self, Option<Category>) -> Self
ItemSummariesRequest::set_count_if_some(self, Option<NonZeroU32>) -> Self
ItemSummariesRequest::reset_offset(self) -> Self
Regional::item(&self, impl IntoValue<ItemId>) -> ItemRequest
Regional::find_item(&self, impl IntoValue<ItemId>) -> FindItemRequest
Regional::update_item(&self, impl IntoValue<ItemId>, impl IntoValue<CommandId>, UpdateInput)
    -> UpdateItemRequest
Regional::feed(&self) -> FeedRequest
Regional::export(&self) -> ExportRequest
```

The matrix does not generate `omit_count`, `reset_category`, `set_tags` on the
plain required `tags` input, or `Clone` for the body-bearing
`UpdateItemRequest`.
