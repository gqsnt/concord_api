# Generated client

- **Audience:** API callers and authors reviewing generated usage
- **Purpose:** guide and generated-surface reference
- **Normative references:** [DSL specification](spec.md), [runtime model](runtime.md)

The generated Rust API is a product surface, not an implementation detail. It
must mirror the domain tree and expose only actions supported by the resolved
contract and Rust trait obligations.

## Declaration-to-usage map

| Declaration | Generated Rust |
|---|---|
| `client CatalogApi` | `CatalogApi`, `CatalogApiBuilder`, module `catalog_api` |
| root `GET health` | method `health`, `catalog_api::Health`, `HealthRequest` |
| `scope regional` | method/module `regional`, immutable `Regional` snapshot |
| scoped `GET by_id` | method `by_id`, `ById` endpoint, `ByIdRequest` wrapper |

Scope modules follow declaration nesting. There is one generated endpoint tree
and no generated prelude requirement for ordinary inherent methods.

## Construction

`Client::new()` exists only when no required client value, secret, or custom
provider is missing. It performs the same validation as
`Client::builder().build()`.

The generated builder owns:

- required and non-required client value controls;
- named secret and provider inputs;
- `retry(policy)`;
- `configure_http(|http| ...)`;
- `configure_runtime(|runtime| ...)`;
- `build()`.

Repeated transformations apply immediately in call order. A successful build
freezes one immutable runtime snapshot.

## Scope snapshots

`client.scope_name(required_arguments)` returns an immutable scope snapshot.
It owns selected public values and retains the shared client runtime. Generated
controls consume one snapshot and return another; sibling snapshots remain
isolated.

Scopes provide generated navigation, but are also explicit values that may be
stored, cloned, configured through consuming controls, and used to construct
several related endpoint requests.

## Endpoint leaves have three useful forms

```text
direct:
client_or_scope.endpoint(required arguments) -> EndpointNameRequest

explicit:
module::EndpointName::new(required arguments) -> EndpointName
client.request(endpoint_value) -> EndpointNameRequest

runtime-defined:
RuntimeEndpoint::builder(...) -> RuntimeEndpoint
client.request(runtime_endpoint) -> Request<_>
```

Direct and explicit generated forms resolve the same endpoint semantics.
Explicit endpoint values retain inherited scope controls and expose
`description()`. Binding them to a client produces the same named request
wrapper as the direct method.

The generated module contains a sealed `Endpoint` trait. Applications cannot
forge generated endpoint ownership.

## Value controls

| Declaration property | Controls |
|---|---|
| required, no absence, no default | constructor argument or required client builder `set_*` |
| absence-capable | `set_*`, `clear_*` |
| defaulted | `set_*`, `reset_*` |
| absence-capable and defaulted | `set_*`, `clear_*`, `reset_*` |

For `Option<T>`, `set_*` accepts `T`. `clear_*` selects absence. `reset_*`
restores the declaration default retained by the current snapshot.

## Request controls and terminals

Every named request wrapper supports:

- `.await` for the declared output;
- `.response().await` for accepted metadata and output;
- `.response_bytes().await` when the response mode is buffered or discard;
- `.timeout(duration)`;
- `.disable_timeout()`;
- `.inherit_timeout()`;
- `.debug(level)`.

A paginated endpoint additionally supports `.paginate(limit)`. An endpoint used
as a credential source additionally supports its generated
`.store_as_NAME().await` terminal.

Unavailable capabilities are absent or fail a sealed trait bound. They do not
fall back to another executor.

## Descriptions and planning

`endpoint.description()` returns stable structural facts: identity, method,
source/route shape, body and response categories, timeout selection, and
capability flags.

`client.plan(&endpoint)` performs real public head shaping and returns a
`SafeRequestPlan`. Descriptions and plans exclude bound bodies, credentials,
resolved rate-key values, native requests, and runtime objects.

## Public namespaces

The façade exposes semantic modules for `auth`, `body`, `codec`, `endpoint`,
`error`, `hooks`, `http`, `pagination`, `rate_limit`, `records`, `response`,
`retry`, and `runtime`.

`concord::prelude` is a convenience import for handwritten code. Generated
inherent methods require no trait import.

## Design review checklist

Before freezing a declaration, inspect the generated surface:

- Does navigation follow the domain?
- Are values owned at the correct level?
- Are optional/default controls clear?
- Are names concise because the parent tree already supplies context?
- Are unsupported operations absent?
- Can explicit endpoints be planned and inspected without hidden effects?

## Related

- [Mental model](mental_model.md)
- [Modeling APIs](modeling_apis.md)
- [Runtime endpoints](runtime_endpoints.md)
- [Planning and observation](planning_and_observation.md)
