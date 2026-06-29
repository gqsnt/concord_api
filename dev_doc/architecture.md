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
-> concord_core request plan execution
```

## Boundaries

Macros own syntax. Any new keyword, stanza, or DSL diagnostic belongs in `concord_macros`. The core crate must not depend on DSL syntax or macro AST types.

Core owns runtime behavior. Auth application, retry, rate-limit acquisition, transport, decode, and pagination execution live in `concord_core`.

Auth rejection is a bounded runtime substage: transport metadata is classified first, hooks and rate-limit observe that transport metadata, then auth rejection handling runs before the normal retry decision. The accepted request-order invariants keep raw auth confined to `TransportRequest` and keep debug, hook, retry, and rate-limit metadata on the final sanitized logical request.

Pagination and page-request mutation belong in the logical-request phase before auth-collision validation, rate limiting, and transport materialization. Raw auth must remain confined to `TransportRequest`.

Code generation consumes resolved semantic data, not raw syntax. If codegen needs to know whether something came from `defaults { ... }` versus `default { ... }`, that is usually a design smell. Sema should resolve aliases, inheritance, profile names, and merge rules before codegen.

Endpoint I/O follows the same principle: the resolved semantic model keeps HTTP endpoint I/O separate from the rest of the request and response plumbing. HTTP endpoints carry HTTP request/response body shapes, and codegen should dispatch on resolved HTTP shapes directly instead of inferring behavior from raw syntax.

Raw parser AST may contain invalid forms long enough to produce good diagnostics. Resolved semantic IR should not. Sema lowers public policy, route, and pagination values into context-specific IR that cannot contain auth-secret references or other values rejected for that context. Codegen renders already-valid IR and must not rely on `expect("validated ...")`, `expect("valid ...")`, or `unreachable!()` for semantic invalid states.

Behavior profiles are semantic sugar. They are lowered before runtime into ordinary auth, retry, and rate-limit data. `concord_core` must not know behavior profiles exist.

The generated client is a typed facade over request plans. The public API should stay facade-first, while advanced endpoint structs remain available for tests and request planning.

Public docs are in `docs/`. Maintainer docs are in `dev_doc/`.

For endpoint I/O expansion work, see [endpoint_io.md](endpoint_io.md).
