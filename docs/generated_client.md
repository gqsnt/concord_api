# Generated client

The generated surface is public relative to the Rust module containing the
`api!` invocation.

## Names

| Declaration | Generated Rust |
|---|---|
| `client CatalogApi` | `CatalogApi`, `CatalogApiBuilder`, module `catalog_api` |
| root `GET health` | `catalog_api::Health`, `catalog_api::HealthRequest` |
| `scope product_groups` | method/module `product_groups`, type `ProductGroups` |
| scoped `GET by_id` | method `by_id`, types `ById` and `ByIdRequest` |

Scope modules follow declaration nesting. There is one generated endpoint tree
and no generated prelude.

## Construction

`CatalogApi::new()` exists when no required client values, secrets, or custom
providers are missing. It runs the same validation as
`CatalogApi::builder().build()`.

Required public values use `set_NAME`. Required secrets and providers use
their declared generated names.

The builder owns:

- value `set_*`, `clear_*`, and `reset_*` controls;
- secret and provider inputs;
- `retry(policy)`;
- `configure_http(|http| ...)`;
- `configure_runtime(|runtime| ...)`;
- `build()`.

Repeated transformations apply immediately in call order.

## Scopes

`client.scope_name(required_arguments)` returns an immutable scope snapshot.
The snapshot retains the shared client runtime and its public values.

Absence-capable or defaulted scope values use consuming `set_*`, `clear_*`,
and `reset_*` controls according to their capabilities.

## Endpoint leaves

Each endpoint leaf generates a direct method, an explicit endpoint value, and
a named request wrapper:

```text
client_or_scope.endpoint(required arguments) -> EndpointNameRequest
module::EndpointName::new(required arguments) -> EndpointName
client.request(endpoint_value) -> EndpointNameRequest
```

Direct and explicit paths resolve the same endpoint semantics. Explicit
endpoint values retain inherited scope controls. Named request wrappers retain
only their owned non-required endpoint controls.

The generated module contains a sealed `Endpoint` trait. Applications cannot
implement it.

## Descriptions and planning

`endpoint.description()` returns `EndpointDescription` with stable identity,
method, route/body/response categories, timeout selection, and capability
flags.

`client.plan(&endpoint)` returns `SafeRequestPlan` after real public
head-shaping validation.

Descriptions and plans exclude bound bodies, credentials, resolved rate-limit
values, native requests, and runtime objects.

## Request controls

Every named request wrapper supports:

- `.await` for the declared output;
- `.response().await` for accepted metadata and output;
- `.response_bytes().await` when the response mode is buffered or discard;
- `.timeout(duration)`;
- `.disable_timeout()`;
- `.inherit_timeout()`;
- `.debug(level)`.

A paginated endpoint also supports `.paginate(limit)`. An endpoint used as a
credential source also supports its generated `.store_as_NAME().await`
terminal.

Unavailable capabilities are absent or fail a sealed trait bound. They do not
fall back to another executor.

## Value controls

| Declaration property | Controls |
|---|---|
| required, no absence, no default | constructor argument or builder `set_*` |
| absence-capable | `set_*`, `clear_*` |
| defaulted | `set_*`, `reset_*` |
| absence-capable and defaulted | `set_*`, `clear_*`, `reset_*` |

All controls consume and return the builder or immutable snapshot. Sibling
snapshots remain isolated.

## Public namespaces

The façade exposes semantic modules for `auth`, `body`, `codec`, `endpoint`,
`error`, `hooks`, `http`, `pagination`, `rate_limit`, `records`, `response`,
`retry`, and `runtime`.

`concord::prelude` is a convenience import for handwritten code. Generated
inherent methods require no trait import.

## Related

- [DSL specification](spec.md)
- [Runtime model](runtime.md)
- [Advanced endpoints](advanced_endpoints.md)
