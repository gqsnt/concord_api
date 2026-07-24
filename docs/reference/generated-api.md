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
    .required_value(value)
    .required_secret(concord::SecretString::new(secret))
    .provider(provider)
    .configure_http(|http| /* safe transport configuration */ http)
    .configure_runtime(|runtime| /* limits/hooks/limiter */ runtime)
    .build()?;
```

Required value, secret, and provider setters are named. `CatalogApi::new()`
exists only when none are required. Missing inputs are runtime-validated at
`build()` using `ClientBuildError` and safe `ClientInputKind`.

Optional setters have `clear_NAME()`. Defaulted setters have `reset_NAME()`.
Replacement configuration is last-write-wins. An explicit limiter takes
precedence over generated façade composition.

## Scopes

A scope with required parameters is entered through:

```rust
client.scope_name(required_in_declaration_order)
```

It returns a named scope value with inherent optional/default setters and
child scope/endpoint methods. Scope values contain no client runtime and are
inert until an endpoint is bound.

## Endpoints and request wrappers

Each endpoint emits:

```text
client_or_scope.endpoint(required arguments) -> EndpointNameRequest
module::EndpointName::new(required arguments) -> EndpointName
client.request(endpoint_value) -> EndpointNameRequest
```

The direct and explicit paths use the same endpoint model. Optional/default
endpoint values are inherent setters on both the explicit value and named
request. `Debug` for generated request values is body/secret-free.

Every generated endpoint value exposes:

```rust
endpoint.description() -> &concord::endpoint::EndpointDescription
```

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
