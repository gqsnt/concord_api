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
- resolved request-fact preparation
- policy application functions
- pagination helpers
- endpoint-backed credential acquisition methods

The generated client should preserve the facade-first API shape. Advanced endpoint structs exist for focused tests and request planning.

Generated public API names are validated after resolution and before codegen. The validator must use the same naming helpers as codegen for endpoint methods, scope facades, auth helpers, and generated public type names. Public DSL names that would collide with generated methods, generated types, reserved helper names, or raw Rust identifiers are semantic errors rather than Rust duplicate-definition failures.

Constructor shape is stable: generated `new(...)` and `builder()` keep ordinary vars before auth vars and secrets, each in source declaration order. Generated clients are concrete and own a core `ApiClient<Cx>`.

## Generated Preparation

Endpoint builders collect resolved field values, route pieces, public query and
header values, authentication requirement descriptors, body and response
adapter inputs, rate-limit descriptors, pagination bindings, and endpoint
metadata. They pass those facts through the narrow `__private` preparation
API. Core constructs the runtime policy and request plan and returns an opaque
`GeneratedPreparedCall<Cx, Output>`; generated code never constructs, mutates,
returns, or receives a Core runtime plan.

## Policies And Routes

Codegen emits resolved route, query, and header logic from semantic data. It should not inspect raw syntax such as whether a profile was declared flat or grouped.

Policy, route, and pagination emitters receive context-specific resolved value IR. Ordinary public policy values cannot contain auth or secret references, raw-identifier aliases of reserved roots, generated implementation-local roots, macro-token bypasses, or secret exposure method calls. Route values and pagination assignments are closed by the same sema validation before codegen runs. Codegen must not make public expressions depend on locals named `auth`, `secret`, `cx`, `ep`, `vars`, `self`, `request`, or similar implementation details. If generated code still needs safe resolved values, it should use internal-only bindings and context-specific IR rather than user-authored local capture.

Generated construction code must not panic because a prior phase "validated" a value. If an internal invariant is somehow violated while building retry status lists, rate-limit windows or costs, OAuth2 token URLs, or similar runtime config, generated code or core constructors must return typed errors rather than using `expect(...)` or `unreachable!()`.

When a codegen helper encounters an unexpected mismatch in resolved IR, it should emit a compile-time diagnostic instead of panicking.

Body codec encoding is emitted from the endpoint signature `body: Codec<T>`. Response codec decoding is emitted from `-> Codec<T>`.

## Pagination

For endpoints with a resolved `paginate` block, codegen implements the hidden
generated pagination marker. Core owns the runtime adapter. The request builder
exposes `.paginate(PaginationTermination::...)` only for endpoints with that
descriptor and a `PageItems` response.

## Endpoint-Backed Credentials

Auth endpoints that return credential material directly get acquisition helpers such as `.acquire_as_session()`. These helpers execute the endpoint and store material in the credential slot.

Endpoint-backed auth-state handles are exposed under `auth_state().credential_name()` with fallible `set`, `clear`, and `is_set` methods. Acquisition helpers are named from the real generated public credential name.

Generated auth preparation resolves credential leases against semantic `AuthRequirement` placement. Core validates and binds material, then inserts it only into the native Reqwest request immediately before managed-client execution.

Generated auth-var and helper paths must propagate lock and state failures as typed `AuthError` / `ApiClientError::Auth` values. They must not unwrap runtime auth locks or assume the state is available because the generated API owns the client.

Generated endpoint and auth-internal response handling must preserve the core body-size boundary. Endpoint decoding receives bytes only after the runtime bounded reader accepts the endpoint response, and auth acquisition receives token or credential responses only after auth-internal read limits have accepted them.

## Rustdoc

Rustdoc is generated from resolved endpoint metadata. Profile labels attached through defaults, scopes, and endpoints are emitted as a concise `Profile: ...` line. Do not render secrets or secret values in rustdoc.

Semantic logic should stay in sema where possible. Codegen mainly turns resolved model data into Rust.
