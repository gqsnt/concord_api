# Code Generation

Codegen translates the resolved semantic model into Rust tokens. It should not re-resolve names or duplicate inheritance logic.

## Generated Shape

Generated code includes:

- client type
- client builder
- vars and auth vars storage
- auth-state accessors
- facade methods
- scope facade structs
- endpoint structs under `endpoints::*`
- request plan construction
- policy application functions
- pagination helpers
- endpoint-backed credential acquisition methods

The generated client should preserve the facade-first API shape. Advanced endpoint structs exist for focused tests and request planning.

Generated public API names are validated after resolution and before codegen. The validator must use the same naming helpers as codegen for endpoint methods, scope facades, auth helpers, and generated public type names. Public DSL names that would collide with generated methods, generated types, reserved helper names, or raw Rust identifiers are semantic errors rather than Rust duplicate-definition failures.

Constructor shape is stable: generated `new(...)`, `builder()`, and `new_with_transport(...)` keep ordinary vars before auth vars and secrets, each in source declaration order. Normal users should not need to name endpoint marker structs; `endpoints::*` remains an advanced explicit-endpoint surface.

## Request Plans

Endpoint builders collect field values, route pieces, query and header policy, auth requirements, body codec information, response codec information, retry and rate-limit plans, pagination controller types and bindings, and endpoint metadata. They build `concord_core` request plans.

## Policies And Routes

Codegen emits resolved route, query, and header logic from semantic data. It should not inspect raw syntax such as whether a profile was declared flat or grouped.

Policy, route, and pagination emitters receive context-specific resolved value IR. Ordinary public policy values cannot contain auth or secret references, raw-identifier aliases of reserved roots, generated implementation-local roots, macro-token bypasses, or secret exposure method calls. Route values and pagination assignments are closed by the same sema validation before codegen runs. Codegen must not make public expressions depend on locals named `auth`, `secret`, `cx`, `ep`, `vars`, `self`, `request`, or similar implementation details. If generated code still needs safe resolved values, it should use internal-only bindings and context-specific IR rather than user-authored local capture.

Generated construction code must not panic because a prior phase "validated" a value. If an internal invariant is somehow violated while building retry status lists, rate-limit windows or costs, OAuth2 token URLs, or similar runtime config, generated code or core constructors must return typed errors rather than using `expect(...)` or `unreachable!()`.

When a codegen helper encounters an unexpected mismatch in resolved IR, it should emit a compile-time diagnostic instead of panicking.

Body codec encoding is emitted from the endpoint signature `body: Codec<T>`. Response codec decoding is emitted from `-> Codec<T>`.

## Pagination

For endpoints with a resolved `paginate` block, codegen implements the core `PaginatedEndpoint` marker. The runtime request builder exposes `.paginate(PaginationTermination::...)` only for endpoints with that marker and a `PageItems` response. Codegen must not mark non-paginated endpoints just because their response type can expose items.

## Endpoint-Backed Credentials

Auth endpoints that return credential material directly get acquisition helpers such as `.acquire_as_session()`. These helpers execute the endpoint and store material in the credential slot.

Endpoint-backed auth-state handles are exposed under `auth_state().credential_name()` with fallible `set`, `clear`, and `is_set` methods. Acquisition helpers are named from the real generated public credential name.

Generated auth preparation code resolves credential leases and receives an auth-only application request rather than `BuiltRequest`. Generated code calls core auth helpers that attach typed pending auth slots, then returns a prepared credential sidecar to the runtime so raw material can be inserted only when a `TransportRequest` is materialized immediately before send.

Generated auth-var and helper paths must propagate lock and state failures as typed `AuthError` / `ApiClientError::Auth` values. They must not unwrap runtime auth locks or assume the state is available because the generated API owns the client.

Generated endpoint and auth-internal response handling must preserve the core body-size boundary. Endpoint decoding receives bytes only after the runtime bounded reader accepts the endpoint response, and auth acquisition receives token or credential responses only after auth-internal read limits have accepted them.

## Rustdoc

Rustdoc is generated from resolved endpoint metadata. Behavior labels attached through defaults, scopes, and endpoints are emitted as a concise `Behavior: ...` line. Do not render secrets or secret values in rustdoc.

Semantic logic should stay in sema where possible. Codegen mainly turns resolved model data into Rust.

