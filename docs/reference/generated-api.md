# Generated API reference

This page owns the stable declaration-to-Rust naming and capability contract.
Concrete signatures remain visible in generated Rustdoc.

## Names

For `client CatalogApi`, the generated client builder is
`CatalogApiBuilder` and the scope-oriented module root is `catalog_api`.
Every source scope/endpoint identifier must be `snake_case`; deterministic
PascalCase types are derived from it:

| Declaration | Method/module | Generated types |
|---|---|---|
| `scope product_groups` | `product_groups` | `ProductGroups` |
| `GET by_id` | `by_id` | `ById`, `ByIdRequest` |

There are no separate façade/endpoints public trees and no generated prelude.

## Client construction

```rust
let builder: CatalogApiBuilder = CatalogApi::builder();
let client: CatalogApi = builder
    .set_required_value(value)
    .required_secret(concord::SecretString::try_new(secret)?)
    .provider(provider)
    .configure_http(|http| /* safe transport configuration */ http)
    .configure_runtime(|runtime| /* limits/hooks/limiter */ runtime)
    .build()?;
```

Required client values use `set_NAME`; secret and provider methods retain their
declared generated names. `CatalogApi::new()` exists only when none are
required. Missing inputs are runtime-validated at `build()` using
`ClientBuildError` and safe `ClientInputKind`.

## Generated value controls

Construction and navigation keep declaration-domain names. Value mutation
uses `set_NAME(value)`; selecting absence uses `clear_NAME()`; restoring a DSL
default uses `reset_NAME()`.

| Generated owner | Required value | Optional value | Defaulted value |
|---|---|---|---|
| Client builder | `set_name` | `set_name` + `clear_name` | `set_name` + `reset_name` |
| Scope façade | constructor argument only | `set_name` + `clear_name` | `set_name` + `reset_name` |
| Explicit endpoint snapshot | constructor argument only | `set_name` + `clear_name` | `set_name` + `reset_name` |
| Named request wrapper | construction only | owned endpoint control: `set_name` + `clear_name` | owned endpoint control: `set_name` + `reset_name` |

Named request wrappers do not gain inherited scope controls they did not
already own. Required scope/endpoint values remain arguments to navigation or
construction. There are no generated bare `name(value)`, `with_*`, or mutable
setter aliases. Secret, credential, provider, timeout, runtime, and handwritten
builder APIs keep their existing names.

Collision validation reserves the exact emitted names. For example, a value
named `build` is legal when `set_build` is free, while a child method or
unchanged secret/provider method named `set_region` collides with the control
for value `region`. Optional `clear_*` and defaulted `reset_*` helper
collisions remain errors.

All generated value controls consume and return the builder or immutable
snapshot. `clear_name()` selects `None`. On a client builder,
`reset_name()` discards the override and lets `build()` evaluate the DSL
default. On a scope, explicit endpoint, or named request snapshot,
`reset_name()` restores the retained evaluated default without re-evaluation.
Repeated selection is last-write-wins. An explicit limiter takes precedence
over generated façade composition.

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

It returns a named scope value with inherent optional/default `set_*` controls
and child scope/endpoint methods. Scope values contain no client runtime and
are inert until an endpoint is bound.

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

Every generated endpoint value exposes:

```rust
endpoint.description() -> &concord::endpoint::EndpointDescription
```

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

The scope root defines a sealed generated `Endpoint` trait with associated
`Request` and `Response` types and a safe description. It supports bounded
generic composition; applications cannot implement it.

## Request capabilities

All named requests can use the declared await terminal. Additional inherent
methods exist only where coherent:

| Capability | Generated method |
|---|---|
| Typed response metadata | `.response()` |
| Pagination | `.paginate(PageLimit)` |
| Request-local timeout | explicit duration/disable/restore methods documented in Rustdoc |
| Safe preview | `client.plan(&endpoint)` before binding |
| Bounded raw response | `.raw_response()` with the dangerous feature |

Invalid terminals are absent instead of being hidden behind a broad extension
trait. Body input, response output, pagination controller, and provider types
remain ordinary trait-driven Rust types.

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
executor. They do not provide partial runtime pagination.

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
| `runtime` | finite runtime configuration |
| `dangerous` | explicit feature-gated capabilities |

`concord::prelude` is optional convenience for common handwritten code; it is
not required to make generated methods appear.
