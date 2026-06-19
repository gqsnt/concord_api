# Code generation

Codegen translates the resolved semantic model into Rust tokens. It should not re-resolve names or duplicate inheritance logic.

## Generated shape

Generated code includes:

- client type
- client builder
- vars and auth vars storage
- auth state accessors
- facade methods
- scope facade structs
- endpoint structs under `endpoints::*`
- request plan construction
- policy application functions
- pagination helpers
- endpoint-backed credential acquisition methods

The generated client should preserve the facade-first API shape. Advanced endpoint structs exist for focused tests and request planning.

## Request plans

Endpoint builders collect field values, route pieces, query/header policy, auth requirements, body codec information, response codec information, retry/cache/rate-limit plans, pagination plans, and endpoint metadata. They build `concord_core` request plans.

## Policies and routes

Codegen emits resolved route/query/header logic from semantic data. It should not inspect raw syntax such as whether a profile was declared flat or grouped.

Body codec encoding is emitted from the endpoint signature `body: Codec<T>`. Response codec decoding is emitted from `-> Codec<T>`.

Resolved cache sizing fields are emitted through core cache config builders for capacity entries, max body bytes, and shared mode. Runtime cache order is unchanged by these fields.

## Mapping

`map Type { expr }` is generated after response decode. The decoded response value is available as `r` inside the map expression.

## Pagination

For paginated endpoints, codegen emits `.paginate()` and page-driving wrappers that connect the resolved `paginate` block to runtime `PaginationController` traits.

## Endpoint-backed credentials

Auth endpoints that map to credential material get acquisition helpers such as `.acquire_as_session()`. These helpers execute the endpoint and store material in the credential slot.

Generated auth preparation code resolves credential leases and receives an auth-only application request rather than `BuiltRequest`. Internal auth hooks use the same sealed request shape. Generated code calls core auth helpers that attach typed pending auth slots, then returns a prepared credential sidecar to the runtime so raw material can be inserted only when a `TransportRequest` is materialized immediately before send. Codegen must not emit raw auth values into ordinary query/header policy data or expose logical URL/header mutation during auth preparation.

## Rustdoc

Rustdoc is generated from resolved endpoint metadata. Behavior labels attached through defaults, scopes, and endpoints are emitted as a concise `Behavior: ...` line. Do not render secrets or secret values in rustdoc.

Semantic logic should stay in sema where possible. Codegen mainly turns resolved model data into Rust.
