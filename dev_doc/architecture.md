# Architecture

Concord is split across three main crates:

- `concord_macros` owns DSL syntax, parsing, semantic analysis, diagnostics, and Rust code generation.
- `concord_core` owns runtime execution: request plans, transport, auth, retry, rate-limit, pagination, and codecs.
- `concord_examples` owns compile-checked usage, public docs fixtures, integration-style examples, and the large Riot fixture.

The high-level flow is:

```text
api! input
-> raw parser AST
-> semantic model
-> generated Rust client and endpoints
-> opaque generated preparation
-> Core-owned request plan and execution
```

## Boundaries

Macros own syntax. Any new keyword, stanza, or DSL diagnostic belongs in `concord_macros`. The core crate must not depend on DSL syntax or macro AST types.

Core owns runtime execution. Auth application, Reqwest retry modes, rate-limit acquisition, transport, decode, and pagination execution live in `concord_core`.

Auth rejection is a bounded runtime substage: safe metadata is classified first, hooks and rate limiting observe it, then auth rejection handling runs before the normal retry decision. Raw auth is confined to the native request.

Pagination and page-request mutation belong in the logical-request phase before auth-collision validation, rate limiting, and native request materialization.

Code generation consumes resolved semantic data, not raw syntax. Sema resolves profile names, inheritance, and merge rules before codegen.

Pagination follows the same rule: codegen should work from controller types, endpoint-field bindings, and presence markers rather than classifier-specific controller metadata.

Endpoint I/O follows the same principle: the resolved semantic model keeps HTTP endpoint I/O separate from the rest of the request and response plumbing. HTTP endpoints carry HTTP request/response body shapes, and codegen dispatches on resolved HTTP shapes directly.

Raw parser AST may contain invalid forms long enough to produce good diagnostics. Resolved semantic IR should not. Sema lowers public policy, route, and pagination values into context-specific IR that cannot contain auth-secret references or other values rejected for that context. Codegen renders already-valid IR and must not rely on `expect("validated ...")`, `expect("valid ...")`, or `unreachable!()` for semantic invalid states.

Profiles are semantic declarations. They are lowered before runtime into ordinary auth and rate-limit data. `concord_core` does not need profile declarations.

The generated client is a typed facade over opaque prepared calls. Generated
source emits resolved facts and uses narrow route, policy, authentication,
rate-limit, body, response, and pagination adapters. Core alone constructs and
executes runtime plans. Hand-written calls use `PreparedEndpoint` or
`PreparedStreamEndpoint`; ordinary applications do not import `__private`.

Public docs are in `docs/`. Maintainer docs are in `dev_doc/`.

For endpoint I/O expansion work, see [endpoint_io.md](endpoint_io.md).
