# CONCORD_MACRO_ARCHITECTURE_0.1

**Version:** 0.1  
**Status:** Internal Release Approved
**Authority:** Intended DSL compiler architecture  
**Depends on:** DSL Specification, Generated API Specification, Feature and Compatibility Specification  
**Supersedes:** None

## 1. Purpose

This document defines the internal architecture of `concord_macros`.

The macro compiler must translate the DSL into typed generated Rust without implementing runtime behavior or exposing compiler internals to generated-client users.

## 2. Compiler pipeline

The macro uses four explicit stages:

```text
TokenStream
    ↓
Parse AST
    ↓
Resolve semantic symbols
    ↓
Normalize complete client/endpoint IR
    ↓
Generate Rust
```

Each stage has one responsibility and its own tests.

Code generation must not redo semantic resolution or inheritance.

## 3. Crate dependency policy

`concord_macros` should depend only on compile-time libraries such as:

- `proc-macro2`;
- `quote`;
- `syn`;
- `proc-macro-crate` or an equivalent package-aware crate-path resolver;
- a diagnostic helper if needed.

It must not depend on:

- Reqwest;
- Tokio;
- Serde JSON runtime integration;
- Governor;
- Concord core implementation modules.

The macro resolves the caller-visible `concord_core` package name and emits a collision-safe private alias for generated tokens. It does not link to or execute runtime code.

## 4. Parse stage

### 4.1 Responsibilities

The parser recognizes:

- token structure;
- punctuation;
- DSL keywords;
- declaration boundaries;
- source spans;
- syntactic ordering that can be determined without name resolution;
- bounded nesting.

It does not determine:

- whether references exist;
- whether a value is optional;
- whether credentials and placements are compatible;
- whether generated names collide;
- final inheritance behavior.

### 4.2 AST design

The AST should preserve source intent and spans rather than mirror runtime types.

Suggested root types:

```rust
struct ApiAst {
    clients: Vec<ClientAst>,
}

struct ClientAst {
    name: Ident,
    base: Spanned<LitStr>,
    values: Vec<ValueDeclAst>,
    credentials: Option<CredentialsAst>,
    retry: Option<RetryAst>,
    config: LayerConfigAst,
    rate_limit_observer: Option<syn::Path>,
    layer: LayerAst,
    children: Vec<ClientChildAst>,
    span: Span,
}
```

Scopes and endpoints similarly retain local layer declarations rather than inherited configuration.
An endpoint AST retains body and response types without semantic classification:

```rust
struct EndpointAst {
    // structural endpoint fields omitted
    body: Option<syn::Type>,
    response: syn::Type,
    pagination: Option<PaginationAst>,
}
```

The parser establishes only that the final parameter named `body`, when present, is last and that
the type following `->` is the response type. It does not inspect a type name, path, generic
argument, crate of origin, codec, media policy, response mode, or application semantics.

### 4.3 Spans

Every user declaration, reference, type path, expression atom, and punctuation-sensitive list item should retain a useful span.

Diagnostics should use:

- declaration span for duplicate declaration errors;
- reference span for unresolved reference errors;
- value expression span for incompatible type/use diagnostics;
- both original and conflicting spans for name collisions.

### 4.4 Ordering

The parser should use explicit state machines for client, scope, and endpoint body ordering rather than parsing every item then sorting or diagnosing later.

This yields earlier and more focused errors.

### 4.5 Restricted expressions

Public expressions are parsed into a dedicated AST:

```rust
enum PublicExprAst {
    String(LitStr),
    Integer(LitInt),
    ClientRef(Spanned<Ident>),
    ScopeRef(Spanned<Ident>),
    EndpointRef(Spanned<Ident>),
    Format(Vec<PublicExprAst>, Span),
}
```

Do not store arbitrary `syn::Expr` for public shaping.

Default expressions use a dedicated restricted AST containing only literals and constant paths. Arbitrary `syn::Expr` values are not retained for defaults.

## 5. Symbol resolution stage

### 5.1 Stable semantic IDs

Resolution assigns compact stable IDs within one expansion:

```rust
struct ClientId(u32);
struct ValueId(u32);
struct SecretId(u32);
struct CredentialId(u32);
struct ScopeId(u32);
struct EndpointId(u32);
struct LexicalRateLimitOrigin {
    client: Ident,
    scopes: Vec<Ident>,
    endpoint: Option<Ident>,
}
```

Generated Rust naming is not used as semantic identity.

### 5.2 Namespaces

Maintain distinct tables for:

- clients in the macro invocation;
- client values per client;
- secrets per client;
- credentials per client;
- child scopes/endpoints per parent;
- one optional direct rate-limit declaration per lexical layer;
- one optional client-only rate-limit observer.

A scope tree stores parent links and visible values.

### 5.3 Reference resolution

Resolve:

- `vars.NAME` against client values;
- `scope.NAME` against visible lexical scope values;
- `ep.NAME` against endpoint parameters;
- `secret.NAME` against earlier secret declarations;
- auth credential names;
- lexical rate-limit declaration paths;
- endpoint-backed endpoint paths.

Ambiguous visible scope names are rejected rather than implicitly choosing the nearest or outer declaration.

### 5.4 Endpoint paths

Endpoint-backed credential paths are resolved from a path syntax such as:

```text
auth::Login
```

Resolution walks scope names and finally an endpoint name.

Aliases do not change semantic endpoint identity for credential references unless the DSL explicitly names aliases as reference syntax. Concord 0.1 references declaration names.

### 5.5 Dependency graph

Build a credential dependency graph containing:

- endpoint-backed credential → producing endpoint;
- endpoint → effective auth credentials after normalization;

Reject direct and statically discoverable cycles.

## 6. Structural value-shape classification

The macro classifies only syntactically recognized value cardinality and query expansion shapes
needed by the DSL:

```rust
enum Cardinality {
    Required,
    Optional,
}
```

Recognize outer `Option<T>` syntactically.

Recognize `Vec<T>` and `Option<Vec<T>>` for query expansion.

Do not attempt semantic equivalence through type aliases. If a user aliases `Option<T>`, generated Rust behavior follows the explicit declared type path and may not receive DSL optional handling.

This restriction should be documented in generated diagnostics and examples.

## 7. Composition and normalization

### 7.1 Layer model

For each client/scope/endpoint layer, resolution produces local operations:

```rust
struct LayerOps {
    host: HostOp,
    path: PathOp,
    query: QueryOp,
    headers: HeaderOp,
    auth: AuthOp,
    timeout: TimeoutOp,
    rate_limit: Option<RateLimitDecl>,
}
```

### 7.2 Normalization traversal

Walk the client tree from outer to inner, carrying immutable normalized configuration.

For each layer:

1. apply host operation;
2. apply path operation;
3. apply query operation;
4. apply header operation;
5. apply auth operation;
6. apply timeout operation;
7. append the direct lexical rate-limit declaration, when present;
8. store the resulting normalized config for the scope/endpoint;
9. recurse to children with that result.

### 7.3 Endpoint IR

A normalized endpoint IR should contain no inheritance operations:

```rust
struct EndpointIr {
    id: EndpointId,
    rust_names: EndpointNames,
    method: MethodIr,
    parameters: Vec<ParameterIr>,
    values: ResolvedValueEnvironment,
    host: Vec<ResolvedExpr>,
    path: Vec<ResolvedExpr>,
    query: Vec<ResolvedQueryEntry>,
    headers: Vec<ResolvedHeaderEntry>,
    auth: Vec<ResolvedAuthUse>,
    timeout: ResolvedTimeout,
    rate_limits: Vec<NormalizedRateLimitRule>,
    body: Option<syn::Type>,
    response: syn::Type,
    pagination: Option<PaginationIr>,
    spans: EndpointSpans,
}
```

### 7.4 Lexical rate-rule normalization

The resolver carries the immutable normalized parent configuration while traversing the client tree.
It appends at most one direct declaration at each layer in client, outer-to-inner scope, endpoint
order. The normalized endpoint IR contains no inheritance operation, template symbol, dynamic value
reference, or key-part expression. Every rule contains only lexical origin, deterministic ID,
static cost, static windows, and a global key marker for code generation.

## 8. Semantic validation

Validation includes:

- exact client base count;
- duplicate DSL names;
- ambiguous references;
- generated public name collisions;
- secret confinement;
- credential source compatibility;
- auth placement compatibility;
- auth collision among statically known placements;
- query/header duplicate entries;
- reserved header names;
- valid rate-limit costs/windows/dimensions;
- endpoint body position and pagination declaration structure;
- duplicate pagination controller fields;
- duplicate resolved pagination binding destinations;
- pagination initializer and writable-destination ownership;
- endpoint-backed cycles;
- static route literal restrictions.

Some type-bound failures remain normal Rust errors:

- missing `Display`;
- missing codec implementation;
- missing `RequestBodySpec`, `ResponseBodySpec`, `PageResponseSpec`, or feature-gated core type;
- missing, unknown, inaccessible, or wrong controller fields;
- non-controller and non-`Send` pagination types;
- custom provider material mismatch;
- `IntoIterator` output mismatch.

Generated wrapper types should produce errors close to the user's declaration where possible.

## 9. Generated naming service

Use one naming module for every generated public and private identifier.

Inputs:

- declaration name;
- alias;
- namespace;
- reserved public names;
- Rust keyword status.

Outputs:

- client type;
- builder type;
- facade type;
- endpoint type;
- module names;
- method names;
- setter/reset/clear names;
- credential handle/accessor names;
- endpoint-backed acquisition names;
- private helper names.

Public names must be deterministic and collision-checked before code generation.

Private names may use stable hashes or IDs but must not appear in public Rustdoc or diagnostics.

## 10. Code generation stage

### 10.1 Responsibilities

Code generation maps normalized IR to:

- generated client builder;
- immutable value storage;
- credential storage and construction;
- generated client;
- facade types;
- endpoint values;
- inherent setters;
- generated-private endpoint trait implementations;
- generated-private pagination trait implementations;
- endpoint-backed acquisition methods;
- explicit endpoint modules;
- Rustdoc;
- generated-contract marker assertion.

### 10.2 Prohibited responsibilities

Code generation must not:

- resolve references;
- perform inheritance;
- infer auth composition;
- inspect or resolve runtime rate-limit values;
- reinterpret optionality;
- invent feature behavior;
- emit public runtime adapters absent from the Generated API Specification.

### 10.3 Typed endpoint generation

For each API, generate one public but sealed API-specific endpoint capability. For each endpoint,
generate one concrete endpoint value, implement that API-specific capability, and implement one
generated-private `EndpointSpec` with static request and response specification associated types.
For a declared body `B`, the endpoint stores `B::Input`; for a response `R`, direct output is
`R::Output` and metadata output is `ApiResponse<R::Output>`. The generated
client request method is constrained by the API-specific capability; the private seal and
`EndpointSpec` bound do not appear in public Rustdoc. Facade methods construct their endpoint value
and call that same request method.

Do not emit a generic runtime decoder function returning `Any`.

### 10.4 Value capture

Generated endpoint values store one `Arc` to the fixed client snapshot, `Arc` or owned enclosing-scope snapshots, and owned/`Arc` endpoint arguments. Explicit endpoint constructors capture every required enclosing scope value in lexical outer-to-inner order. Public constructor parameters retain sanitized DSL names; collision-resistant private storage bindings do not leak into the signature.

The macro should avoid unnecessary `Clone` bounds.

### 10.5 Body generation

For `body: B`, generation emits only the ordinary `B: RequestBodySpec` bound, a method and explicit
constructor argument of `B::Input`, typed input storage, and private endpoint dispatch to the one
core request-specification lifecycle. A bodyless endpoint exposes no body argument and selects one
core-private empty specification. Production macro source has no body semantic enum or branch for
`Json`, `Text`, `Bytes`, `NoContent`, `Stream`, `Multipart`, `Records`, `Csv`, `NdJson`, or any
application type.

### 10.6 Response generation

For `-> R`, generation emits only `R: ResponseBodySpec`, direct `R::Output`, and metadata
`ApiResponse<R::Output>` integration. Core trait implementations and sealed modes select behavior.
Raw-success eligibility uses a core-private capability bound. Production macro source has no
response semantic enum and no type-name, crate, codec, media, or mode branch.

### 10.7 Pagination generation

Generate:

- one arbitrary controller type from the normalized pagination IR;
- one complete ordinary controller struct literal;
- one generated-private associated controller type;
- one initial construction operation;
- one endpoint/scope bound-field operation into a fresh endpoint clone;
- compile-time `PaginationController<Page>` bounds;
- compile-time `R: PageResponseSpec` and `R::Output: IntoIterator` bounds;
- item associated type;
- configured page/item/identity ceiling plumbing.

The parser, IR, resolution, and generation contain no semantic controller-name or response-name,
crate-origin, built-in/custom, arithmetic, cursor, validation, mode, media, or short-page branch.
Ordinary Rust checks struct
field completeness, visibility, types, and controller bounds. Only declared `bind` fields enter
continuation-redaction classification. No factory, public patch, runtime adapter, callback set,
generated loop, erased registry, or `Any` state is generated.

## 11. Generated core-path resolution

Before code generation, the macro resolves the caller-visible dependency name for the `concord_core` package. Generated output introduces one expansion-local private alias and refers to public/generated-private items only through that alias.

The naming service ensures aliases do not collide across nested or repeated macro invocations. Trybuild fixtures cover normal names, renamed dependencies, workspace aliases, nested modules, and multiple expansions.

## 12. Feature validation strategy

The macro crate has no mirrored feature flags.

Body and response feature availability is owned entirely by core public type and trait availability.
Generated code emits the same structural bounds for built-in and downstream types, so selecting
`Json<T>`, `Multipart`, `Records<D>`, `Csv<T>`, or `NdJson<T>` without its feature fails at compile
time and cannot silently select another behavior. The macro may retain structural markers for
non-type DSL capabilities such as dangerous custom retry, but cannot recognize a body or response
name to choose a marker or mode.

Diagnostics should point to the DSL declaration rather than only a missing Rust item when practical.

## 13. Generated contract marker

Every generated client module emits a const-time assertion against a private semantic contract marker.

The marker version changes whenever generated code and core `__private` integration are incompatible.

Generic body/response specifications, universal metadata terminals, prepared selection, and
capability-based pagination changed that integration; the historical Phase 12 marker was `13`.
The synchronized Phase 13 marker is `14` in both macro output and core.

The marker must not rely solely on Cargo package version strings.

## 14. Diagnostics architecture

Use a structured diagnostic type:

```rust
struct Diagnostic {
    primary: SpannedMessage,
    secondary: Vec<SpannedMessage>,
    help: Option<String>,
    code: DiagnosticCode,
}
```

The final macro output converts diagnostics into `syn::Error` or equivalent stable compiler diagnostics.

Principles:

- one primary cause;
- deterministic ordering;
- declaration and conflicting spans shown together;
- canonical syntax in help text;
- no private generated trait names;
- no runtime value evaluation;
- no secret material.

## 15. Compile-time bounds

Bounded recursion must be enforced explicitly, not left to Rust stack behavior.

- increment/check scope depth before recursive parse/resolve;
- increment/check nested format depth;
- use iterative traversal where practical for large child lists;
- reject before allocating excessive recursive structures.

## 16. Performance considerations

Macro performance goals:

- linear parsing in token count;
- hash-map symbol lookup;
- one outer-to-inner normalization traversal;
- no repeated whole-tree scans per endpoint;
- quote generated code once per normalized item;
- avoid cloning large token streams in diagnostics;
- use IDs and references internally rather than copying AST nodes.

A large real-world fixture must be part of macro performance and stack-safety testing.

## 17. Testing architecture

### 17.1 Parse tests

Direct AST shape and focused syntax failures.

### 17.2 Semantic tests

Resolved IDs, normalized inheritance, cycle detection, and collision diagnostics.

### 17.3 Codegen unit tests

Only focused public-token properties that cannot be validated through compilation.

Avoid full token snapshots.

### 17.4 Trybuild

Primary evidence for generated API names, type bounds, feature gates, and diagnostics.

### 17.5 Generated runtime integration

Deterministic tests compile actual generated clients and exercise core runtime behavior.

## 18. Internal invariants

1. Every AST user reference is either resolved to a semantic ID or has a diagnostic.
2. Normalized endpoint IR contains no local inheritance operations.
3. Every generated public name is collision-checked.
4. Every endpoint body is `Option<syn::Type>` and every response is `syn::Type` in AST and IR.
5. Every generated endpoint has one static request specification and one static response specification.
6. Production parsing, resolution, IR, and codegen have no body/response semantic enum or built-in-name branch.
7. Pagination generation uses one type-neutral IR, `PageResponseSpec`, and no controller/response-name branch or runtime downcast.
8. Adding a downstream request, response in any sealed mode, page response, or record decoder requires no macro source change.
9. Codegen never imports macro-private symbols into generated public signatures.
10. Generated Rust requires only documented public APIs plus `__private` integration.

## 19. Rejected macro designs

### 19.1 Parse arbitrary Rust expressions everywhere

Rejected because it makes secret confinement, stable references, diagnostics, and normalization brittle.

### 19.2 Generate runtime profile/policy objects

Rejected because direct normalized endpoint facts are simpler and the product has no generic policy layer.

### 19.3 Codegen-time resolution

Rejected because it duplicates semantic logic and produces poorer diagnostics.

### 19.4 One giant generated client impl

Rejected because smaller generated types and modules improve compiler diagnostics, incremental compilation, and public organization.

### 19.5 Full token snapshots as architecture tests

Rejected because they freeze private generated plumbing and obstruct refactoring.
