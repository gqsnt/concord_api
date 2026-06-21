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

Generated public API names are validated after resolution and before codegen. The validator must use the same naming helpers as codegen for endpoint methods, scope facades, auth helpers, and generated public type names. Public DSL names that would collide with generated methods/types, reserved helper names, or raw Rust identifiers are semantic errors rather than Rust duplicate-definition failures.

Constructor shape is stable: generated `new(...)`, `builder()`, and `new_with_transport(...)` keep ordinary vars before auth vars/secrets, each in source declaration order. Normal users should not need to name endpoint marker structs; `endpoints::*` remains an advanced explicit-endpoint surface.

## Request plans

Endpoint builders collect field values, route pieces, query/header policy, auth requirements, body codec information, response codec information, retry/cache/rate-limit plans, pagination plans, and endpoint metadata. They build `concord_core` request plans.

## Policies and routes

Codegen emits resolved route/query/header logic from semantic data. It should not inspect raw syntax such as whether a profile was declared flat or grouped.

Policy, route, and pagination emitters receive context-specific resolved value IR. Ordinary public policy values cannot contain auth or secret references, raw-identifier aliases of reserved roots, generated implementation-local roots, macro-token bypasses, or secret exposure method calls. Route values and pagination assignments are closed by the same sema validation before codegen runs. Codegen must not make public expressions depend on locals named `auth`, `secret`, `cx`, `ep`, `vars`, `self`, `request`, or similar implementation details. If generated code still needs safe resolved values, it should use internal-only bindings and context-specific IR rather than user-authored local capture.

Generated construction code must not panic because a prior phase "validated" a value. If an internal invariant is somehow violated while building retry status lists, rate-limit windows/costs, OAuth2 token URLs, or similar runtime config, generated code or core constructors must return typed errors rather than using `expect(...)` or `unreachable!()`.

Body codec encoding is emitted from the endpoint signature `body: Codec<T>`. Response codec decoding is emitted from `-> Codec<T>`.

Resolved cache sizing fields are emitted through core cache config builders for capacity entries, max body bytes, and shared mode. Runtime cache order is unchanged by these fields.

## Mapping

`map Type { expr }` is generated after response decode. The decoded response value is available as `r` inside the map expression.

## Pagination

For paginated endpoints, codegen emits `.paginate()` and page-driving wrappers that connect the resolved `paginate` block to runtime `PaginationController` traits.

## Endpoint-backed credentials

Auth endpoints that map to credential material get acquisition helpers such as `.acquire_as_session()`. These helpers execute the endpoint and store material in the credential slot.

Endpoint-backed auth state handles are exposed under `auth_state().credential_name()` with fallible `set`, `clear`, and `is_set` methods. Acquisition helpers are named from the real generated public credential name, for example `.acquire_as_session()`.

Generated auth preparation code resolves credential leases and receives an auth-only application request rather than `BuiltRequest`. Internal auth hooks use the same sealed request shape. Generated code calls core auth helpers that attach typed pending auth slots, then returns a prepared credential sidecar to the runtime so raw material can be inserted only when a `TransportRequest` is materialized immediately before send. Codegen must not emit raw auth values into ordinary query/header policy data or expose logical URL/header mutation during auth preparation.

Generated auth-var setters and endpoint-backed credential state helpers must not unwrap shared locks. Setters that update generated auth vars return `Result<..., AuthError>` when lock state is unavailable. Endpoint-backed credential `set`, `clear`, and `is_set` helpers are fallible for the same reason.

Generated clients do not change codec traits for response-size enforcement. They expose the shared runtime configuration surface, and `concord_core` enforces endpoint response body limits before generated response decoding runs.

## Rustdoc

Rustdoc is generated from resolved endpoint metadata. Behavior labels attached through defaults, scopes, and endpoints are emitted as a concise `Behavior: ...` line. Do not render secrets or secret values in rustdoc.

Semantic logic should stay in sema where possible. Codegen mainly turns resolved model data into Rust.
