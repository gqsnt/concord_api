# Concord v2.1 internals

This document assigns implementation responsibility. Public semantics remain owned by `docs/design_invariants.md` and `docs/spec.md`.

## Workspace roles

| Crate | Responsibility |
|---|---|
| `concord-macros` | parse, resolve, validate, derive surfaces, emit bridge/docs/descriptions/assertions |
| `concord-core` | semantic runtime types, one executor, native HTTP, bounds, auth/retry/rate/pagination/errors |
| `concord-extras` | optional curated semantic implementations |
| `concord` | stable ergonomic façade and feature re-exports |
| `concord-test-support` | deterministic fixtures using production semantics |
| `concord-examples` | compile-checked product examples and docs anchors |
| `xtask` | policy, fixtures, MSRV UI, semantic diff/migration/evidence tooling |
| `perf` | compile/expansion/runtime benchmark evidence |

## Compiler pipeline

```text
TokenStream
→ syntax AST with spans/docs
→ symbol and owner resolution
→ canonical declared semantic graph
→ family-specific effective contract with provenance
→ generated surface/capability model
→ Rust bridge/API/docs/descriptions/assertions
```

Emitters never infer semantics from syntax tokens or generated strings.

## Canonical semantic graph

Conceptually:

```rust
struct ApiContractIr {
    client: ClientIr,
    symbols: SymbolTable,
    descriptions: DescriptionIr,
}

struct ValueIr {
    id: ValueId,
    owner: OwnerId,
    name: Ident,
    payload_type: syn::Type,
    presence: Presence,
    initializer: InitializerIr,
    sensitivity: Sensitivity,
    docs: Docs,
    span: Span,
    controls: ControlSurfaceIr,
}

struct EndpointIr {
    id: OwnerId,
    source_name: Ident,
    contract_id: ContractIdIr,
    method: MethodIr,
    values: Vec<ValueIr>,
    body: Option<BodyInputIr>,
    output: OutputIr,
    local: LocalEndpointContract,
    effective: EffectiveEndpointContract,
    surface: EndpointSurface,
}
```

No `ValueShape::Option`, `ValueShape::Vec`, stream-token detection, or equivalent exists.

## Resolved references and atoms

```rust
struct ResolvedValueRef {
    value: ValueId,
    owner: ValueOwnerRef,
    presence: Presence,
    sensitivity: Sensitivity,
}

enum AtomIr {
    Literal(BoundedLiteral),
    Value(ResolvedValueRef),
    Adapted { source: Box<AtomIr>, adapter: syn::Type },
    Concat(Vec<AtomIr>),
}

enum EmissionIr {
    Scalar(AtomIr),
    Each { source: ResolvedValueRef, adapter: Option<syn::Type> },
}
```

The compiler resolves names into IDs before inheritance/emission. Runtime/generated bridge executes a pre-resolved shaping program.

## Family-specific attachments

Do not implement one generic public merge trait. Maintain dedicated IR/algebra:

- request/response representation nearest-owner resolution plus explicit fences;
- subdomain prepend/replace/off;
- path append/replace/off;
- query ordered patch;
- header case-insensitive patch;
- auth ordered append/replace/off and target ownership;
- timeout nearest finite selection;
- rate additive attachments;
- endpoint-local status/effect/pagination/exposed capabilities.

Every effective item retains source owner/span and inheritance operation.

## Surface model

One surface description generates direct, unbound, docs, and collision analysis:

```rust
struct EndpointSurface {
    direct_required_inputs: Vec<ValueId>,
    configurable_inputs: Vec<ValueControl>,
    body: Option<BodySurface>,
    output: OutputSurface,
    capabilities: EndpointCapabilities,
    names: GeneratedNames,
}
```

Direct and unbound generators must not implement independent argument/control algorithms.

## Rust assertions

Generate focused assertions for facts the macro cannot prove:

- effective request representation implements `RequestRepresentation<Input>`;
- effective response representation implements `ResponseRepresentation<Output>`;
- `RequestAtom` or selected atom adapter;
- `OrderedValues` and item atom adapter;
- pagination controller field/destination types and page capability;
- body page stability/replay capabilities;
- endpoint-backed output-to-credential material compatibility;
- accepted-byte support;
- rate dimension canonicalization.

Assertions must point at the endpoint/value and inherited representation owner.

## Generated bridge

Bridge code:

- reads explicit owner-qualified immutable values;
- executes shaping operations once per pass;
- creates real and sanitized atoms together;
- binds representations/status/effect/auth/rate/pagination metadata;
- preserves client/security-context identity;
- exposes no public fields/native types;
- cannot run a second executor.

## Core state

```text
Static contract
Built client/security context
Selected immutable snapshots
Logical operation/deadline
Visible attempts and dispatch knowledge
Prepared body and current attempt body
Credential source/slot/version
Rate plan/permit/feedback
Response status/driver/output
Pagination aggregate/controller/identity/items
Operation and stream evidence
```

Core owns all state transitions involving native transport, time, private material, attempts, and aggregate loops.

## Executor state machine

The normative order lives in `docs/execution_policy.md`. Implement one internal state machine shared by:

- direct await/response;
- accepted bytes;
- endpoint-backed store;
- generated unbound binding;
- runtime endpoint binding;
- retries and auth recovery;
- pagination pages;
- stream acquisition.

No terminal creates a private loop that bypasses the executor.

## Time and randomness

Core/testable dependencies own:

- operation deadlines;
- retry jitter;
- HTTP-date/server-delay parsing;
- credential expiry/skew;
- rate windows/cooldown;
- pagination/controller timestamps if required.

Extras do not call wall clock independently for a decision also owned by core.

## Error boundary

Core constructs stable `ApiErrorCategory`, `FailureStage`, `DispatchKnowledge`, IDs, status, counts, and bounded sanitized detail. Raw dependency strings are protected because they may include URLs, headers, payloads, or values.

## Test architecture

- parser/semantic unit tests;
- UI compile-pass/fail diagnostics;
- generated surface and negative-space tests;
- deterministic runtime event traces;
- systematic auth/concurrency tests;
- query/header/atom properties;
- pagination proof suite;
- information-flow sentinel matrix;
- large-contract expansion/compile benchmarks;
- feature/MSRV/docs/policy/deny/analyzer gates.

Test support uses the same public and core contracts. It must never add a production-inaccessible native mutation path.
