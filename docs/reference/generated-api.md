# Generated API reference

This page owns the stable declaration-to-Rust naming and capability contract.
Concrete signatures remain visible in generated Rustdoc.

## On this page

- [Names and locations](#names-and-locations)
- [Client construction](#client-construction)
- [Generated value controls](#generated-value-controls)
- [Scopes](#scopes)
- [Endpoints and request wrappers](#endpoints-and-request-wrappers)
- [Descriptions and planning](#descriptions-and-planning)
- [Request controls and terminals](#request-controls-and-terminals)
- [Runtime endpoints](#runtime-endpoints)
- [Public namespaces](#public-namespaces)

## Names and locations

For `client CatalogApi`, the generated client builder is
`CatalogApiBuilder` and the scope-oriented module root is `catalog_api`.
Every source scope/endpoint identifier must be `snake_case`; deterministic
PascalCase types are derived from it:

| Declaration | Method/module | Generated types and path |
|---|---|---|
| `client CatalogApi` | `catalog_api` | `CatalogApi`, `CatalogApiBuilder` |
| root `GET health` | `health` | `catalog_api::Health`, `catalog_api::HealthRequest` |
| `scope product_groups` | `product_groups` | `catalog_api::product_groups::ProductGroups` |
| scoped `GET by_id` | `by_id` | `catalog_api::product_groups::ById`, `ByIdRequest` |

There are no separate façade/endpoints public trees and no generated prelude.
Items are public relative to the Rust module containing the invocation; that
module's ordinary visibility remains authoritative.

## Client construction

```rust
let builder: CatalogApiBuilder = CatalogApi::builder();
let client: CatalogApi = builder
    .set_required_value(value)
    .required_secret(concord::SecretString::try_new(secret)?)
    .provider(provider)
    .retry(policy)
    .configure_http(|http| /* safe transport configuration */ http)
    .configure_runtime(|runtime| /* limits/hooks/limiter */ runtime)
    .build()?;
```

Required client values use `set_NAME`; secret and provider methods retain their
declared generated names. `CatalogApi::new()` exists only when none are
required. Missing inputs are runtime-validated at `build()` using
`ClientBuildError` and safe `ClientInputKind`.

Repeated `retry`, `configure_http`, and `configure_runtime` transformations
apply immediately in call order. `build()` validates the fixed base, required
values, protected material, HTTP configuration, runtime configuration, and
selected limiter before producing an immutable client. Client clones share
that runtime and its retry policy, credential slots, hooks, transport, and
limiter. Independent builds remain separate unless the application explicitly
shares a policy or another stateful extension object.

## Generated value controls

Construction and navigation keep declaration-domain names. Value mutation
uses `set_NAME(value)`; selecting absence uses `clear_NAME()`; restoring a DSL
default uses `reset_NAME()`. Absence capability and a declaration default are
independent properties.

| Generated owner | Plain required value | Absence-capable, no default | Has a default, not absence-capable | Absence-capable and has a default |
|---|---|---|---|---|
| Client builder | `set_name` | `set_name` + `clear_name` | `set_name` + `reset_name` | `set_name` + `clear_name` + `reset_name` |
| Scope façade | constructor argument only | `set_name` + `clear_name` | `set_name` + `reset_name` | `set_name` + `clear_name` + `reset_name` |
| Explicit endpoint snapshot | constructor argument only | `set_name` + `clear_name` | `set_name` + `reset_name` | `set_name` + `clear_name` + `reset_name` |
| Named request wrapper | construction only | owned endpoint control: `set_name` + `clear_name` | owned endpoint control: `set_name` + `reset_name` | owned endpoint control: `set_name` + `clear_name` + `reset_name` |

For `Option<T>`, `set_name` accepts `T`; for `Option<Vec<T>>`, it accepts
`Vec<T>`. Each setter selects a present value directly; `clear_name()` selects
absence.

Named request wrappers do not gain inherited scope controls they did not
already own. Required scope/endpoint values remain arguments to navigation or
construction. There are no generated bare `name(value)`, `with_*`, or mutable
setter aliases. Secret, credential, provider, timeout, runtime, and handwritten
builder APIs keep their existing names.

Collision validation reserves the exact emitted names. For example, a value
named `build` is legal when `set_build` is free, while a child method or
unchanged secret/provider method named `set_region` collides with the control
for value `region`. Absence-capable `clear_*` and declaration-default
`reset_*` helper collisions remain errors. When both capabilities exist, both
helper names are reserved and validated.

All generated value controls consume and return the builder or immutable
snapshot. For a defaulted `Option<T>`, a client builder retains three states:
no override, an explicitly present value, and explicit absence. `clear_name()`
selects explicit absence; `reset_name()` returns to no override and lets
`build()` choose and evaluate the DSL default. A scope, explicit endpoint, or
named request snapshot instead retains the evaluated default:
`clear_name()` selects absence and `reset_name()` restores that retained value
without re-evaluation. Repeated selection is last-write-wins. An explicit
limiter takes precedence over generated façade composition.

Built-in static credential material is validated during `build()` before the
immutable client snapshot and credential slots are created. Its generated
credential handles expose fallible replacement methods; validation failure
preserves the prior slot material and version. Construction and replacement
errors identify only safe generated input names and categories.

## Scopes

A scope with required parameters is entered through:

```rust
client.scope_name(required_in_declaration_order)
```

It returns a named scope value with inherent non-required `set_*` controls,
independently derived clear/reset helpers, and child scope/endpoint methods.
The scope snapshot retains the original client's shared runtime and its own
public value snapshot. Entering a scope performs no I/O, provider work, or
transport construction; it remains inert until a request terminal is polled.

## Endpoints and request wrappers

Each endpoint emits:

```text
client_or_scope.endpoint(required arguments) -> EndpointNameRequest
module::EndpointName::new(required arguments) -> EndpointName
client.request(endpoint_value) -> EndpointNameRequest
```

The direct and explicit paths use the same endpoint model. Optional/default
endpoint values are inherent `set_*` controls on both the explicit value and
named request. Existing non-required inherited scope controls remain on the
explicit endpoint snapshot, not the named request wrapper. `Debug` for
generated request values is body/secret-free.

The scope root defines a sealed generated `Endpoint` trait with associated
`Request` and `Response` types and a safe description. It supports bounded
generic composition; applications cannot implement it.

## Descriptions and planning

Every explicit endpoint value exposes:

```rust
endpoint.description() -> &concord::endpoint::EndpointDescription
```

The description contains stable structural identity, route/body/response
categories, timeout selection, and capability flags including whether
effective rate limits exist. It contains no bound runtime, body input,
credentials, resolved rate keys, detailed rule values, or native request.

Generated endpoint Rustdoc lists every applicable rate-limit rule in inherited
order. Each entry includes its lexical origin, deterministic declaration rule
ID, `Bucketed by` dimension names (or `declaration-global`), cost, and ordered
windows. Resolved key values never enter Rustdoc or `EndpointDescription`.

That declaration ID is deterministic within one generated client contract and
changes when its source identity changes. It is not a globally unique
namespace across unrelated `api!` expansions. Default-built clients have
separate limiter objects. Sharing one limiter across instances of the same
generated client intentionally shares matching rule state; sharing one across
unrelated generated contracts requires compatible rule namespaces.

Planning is a client operation on an unbound endpoint:

```rust
let plan = client.plan(&endpoint)?;
```

It executes the real public head-shaping path and returns
`SafeRequestPlan`. It does not run hooks, encode or poll a body, acquire
credentials or limiter authority, open transport, or dispatch completion.
There is no exact authenticated request preview.

## Request controls and terminals

All named request wrappers provide consuming request-local controls and the
ordinary await terminal:

| Capability | Generated surface | Result |
|---|---|
| Declared output | `.await` / `IntoFuture` | `Result<Response::Output, ApiError>` |
| Accepted metadata plus output | `.response().await` | `Result<ApiResponse<Response::Output>, ApiError>` |
| Accepted metadata plus bounded bytes | `.response_bytes().await` | `Result<ApiResponse<Bytes>, ApiError>` for `Buffered` or `Discard` declarations |
| Explicit timeout | `.timeout(Duration)` | replaces the generated selection |
| No deadline | `.disable_timeout()` | explicitly disables it for this request |
| Declared deadline | `.inherit_timeout()` | restores the endpoint selection |
| Built-in structural debug | `.debug(DebugLevel)` | request-local debug selection |
| Pagination | `.paginate(PageLimit)` | only for a declared paginated endpoint |
| Endpoint-backed credential storage | `.store_as_NAME().await` | only on the endpoint that supplies credential `NAME` |

Invalid terminals are absent instead of being hidden behind a broad extension
trait. `response_bytes()` is always emitted and carries a sealed
`ResponseBytesMode` bound, so streaming declarations fail during type checking.
It preserves the response specification's status acceptance and `Accept`
header, skips validation and decoding, and uses the ordinary response-body
limit. Body input, response output, pagination controller, and provider types
remain ordinary trait-driven Rust types.

Capability absence is part of the API:

| Endpoint shape | Deliberately absent |
|---|---|
| no `paginate` declaration | `.paginate(...)` |
| streaming, record, or otherwise non-page response | generated pagination |
| request body without `PageRequestBodySpec` | generated pagination |
| not used as an endpoint-backed credential source | `.store_as_NAME()` |
| declared response mode not `Buffered` or `Discard` | callable `.response_bytes()` terminal |

Named request wrappers retain controls only for their own non-required
endpoint values. They do not gain inherited scope controls. Every control and
terminal consumes the wrapper, so sibling request snapshots remain isolated.

A paginated request body is an ordinary opaque specification selected by the
declaration, but Rust additionally requires
`concord::body::PageRequestBodySpec`. Its input is cloned from the original
typed endpoint for each page and must independently prepare the same stable
body. The request body is not a pagination-controlled field and its bytes are
not part of cycle identity.

## Runtime endpoints

`concord::endpoint::RuntimeEndpoint::builder(EndpointId, Method)` builds a
typed runtime-defined endpoint. The builder owns public host/path/query/header
shape, body/response specifications, and timeout tri-state. Binding it with
`client.request(endpoint)` returns the ordinary managed `Request<E>`.

Builder mutations are incrementally bounded. Dynamic values use capped
formatted capture; repeated query values consume only through the first
withheld excess; and a latched structural error prevents later formatting or
iterator consumption before `build()` returns its safe category.

Runtime endpoints have the same safe description, planning shape, and
executor. Eligible runtime endpoint requests support both `.response()` and
`.response_bytes()` through the generic request type. They do not provide
partial runtime pagination.

## Public namespaces

Ordinary imports come from `concord`:

| Namespace | Purpose |
|---|---|
| `auth` | credentials, providers, recovery |
| `body` | body specifications/factories/multipart |
| `codec` | bytes, text, media codecs |
| `endpoint` | descriptions, IDs, runtime endpoints |
| `error` | stable safe errors and categories |
| `hooks` | sanitized lifecycle observation |
| `http` | safe HTTP value/configuration types |
| `pagination` | contracts, limits, controllers |
| `rate_limit` | plans, keys, limiters, observers |
| `records` | bounded record streams/decoders |
| `response` | modes, metadata, specifications |
| `retry` | managed retry policy and classifiers |
| `runtime` | finite runtime configuration |

`concord::prelude` is optional convenience for common handwritten code; it is
not required to make generated methods appear.

## Related

- [Complete DSL reference](dsl.md)
- [Build and share clients](../guide/client-construction.md)
- [Requests, planning, and errors](../guide/requests-planning-errors.md)
- [Feature flags](features.md)
- [Managed retries](../features/retries.md)
- Rustdoc: the generated client and its `catalog_api`-style module
