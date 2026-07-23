# Concord declaration compiler

> Audience: contributors changing `api!`, generated Rust, or the private façade bridge

The declaration compiler has one directional pipeline:

```text
source tokens
    ↓
syntax AST
    ↓
normalized contract
    ↓
generated surface
    ↓
token artifacts
    ↓
concord::__private façade bridge
    ↓
concord-core managed lifecycle
```

Each arrow is an ownership boundary. A change should enrich the existing
representation at the owning boundary instead of creating a parallel parser,
model, name table, or generated execution path.

## Source layout

```text
concord_macros/src/
├── syntax/
│   ├── declaration.rs   client, scope, and endpoint structure
│   ├── values.rs        values, parameters, defaults, and explicit bodies
│   ├── request.rs       host, path, query, header, and timeout syntax
│   ├── credentials.rs   credential and authentication syntax
│   ├── pagination.rs    controller bindings and initializer syntax
│   ├── rate_limits.rs   lexical quota declarations
│   ├── diagnostics.rs   shared parse diagnostics
│   ├── keyword.rs       grammar keywords
│   └── ast.rs           the only source AST
├── semantic/
│   ├── symbols.rs       value shapes, symbol tables, and reference resolution
│   ├── inheritance.rs   lexical operation normalization and public expressions
│   ├── auth.rs          credential and effective-auth validation
│   ├── pagination.rs    pagination binding normalization
│   ├── rate_limits.rs   rule composition and deterministic identities
│   ├── surface.rs       generated names, paths, docs identity, and capabilities
│   └── model.rs         the only normalized contract
├── emit/
│   ├── client.rs        clients and named builders
│   ├── scopes.rs        scope modules and facades
│   ├── endpoints.rs     explicit endpoint values
│   ├── requests.rs      named request wrappers and inherent terminals
│   ├── credentials.rs   credential facades and handles
│   ├── descriptions.rs  safe endpoint descriptions
│   ├── docs.rs          generated Rustdoc
│   ├── assertions.rs    specification aliases and semantic trait bounds
│   ├── bridge.rs        generated-contract assertion
│   ├── shaping.rs       normalized public-head construction
│   └── helpers.rs       small token and private-name helpers
└── crate_path.rs        renamed caller-visible `concord` resolution
```

`lib.rs` owns the only parse → resolve → emit entry. Production syntax and
semantic modules never emit tokens. Emitters receive the resolved model and do
not discover new user errors.

## Validation ownership

| Question | Owner | Proof |
|---|---|---|
| Is the token sequence legal and is a clause duplicated? | `syntax` domain module | colocated parser unit test and façade UI diagnostic |
| What declaration or value does a name reference? | `semantic::symbols` | normalized-model test |
| What is the final inherited request configuration? | `semantic::inheritance` | normalized-model test |
| Is authentication, pagination, or rate limiting structurally coherent? | matching semantic domain | normalized-model test plus façade UI case when user-facing |
| What public name, module, setter, or terminal exists? | `semantic::surface` | normalized-surface test and façade compile-pass/fail test |
| What Rust tokens implement a known artifact? | matching `emit` module | local artifact-signature test |
| Does a Rust type implement the required semantic contract? | generated trait bound and Rust compiler | façade compile-pass/fail test |
| Does execution preserve lifecycle and security invariants? | `concord-core` | deterministic runtime/security test |

Diagnostics are created while parsing or resolving, with the original DSL span.
An emitter may rely on normalized invariants and use `expect` only for an
internal impossibility. It must not turn a user error into a panic or invent
another validation vocabulary.

## Adding a clause

1. Add its syntax node to `syntax::ast`, then parse it in the matching domain
   module. Preserve the clause span and use the shared duplicate-clause helper
   so both declarations are reported.
2. Decide whether the clause changes the normalized contract. If it does, add
   one model field and normalize it in its semantic owner. Do not carry a
   replace/off/append operation into emission.
3. If public names or capabilities change, update `semantic::surface`. Emitters
   must not infer capability availability independently.
4. Update only the emitters for affected artifacts.
5. Add the lowest-layer unit proof, a façade compile-pass or compile-fail proof
   for public behavior, and a deterministic runtime proof if execution changes.
6. Run the macro unit suite, macro UI suite, façade generated-surface suite, and
   expansion comparison.

If the fact can be expressed by an ordinary Rust type implementing a Concord
trait, it normally does not belong in the grammar. Body, response, provider,
limiter, observer, and pagination-controller types remain opaque to the macro.

## Adding a generated capability

Capability availability has one source: `semantic::surface`. Add the capability
there after semantic normalization, then emit the inherent method only from the
matching artifact module. Do not use `Deref`, a generated extension trait, or a
per-client prelude to make the method appear.

Prove all three states where applicable:

- a compile-pass case uses the inherent method without imports;
- a compile-fail case shows the method is absent for an incompatible endpoint;
- the generated-surface test checks the predictable named signature.

The direct client call and explicit endpoint binding must consume the same
`EndpointSurface` and endpoint model.

## Bridge and path rules

`crate_path.rs` resolves the dependency named by Cargo package `concord`,
including a downstream rename. Emission aliases that caller-visible façade and
uses its doc-hidden `__private` bridge. Final user expansions must not require a
direct `concord_core` dependency.

The generated contract version is isolated in
`concord_macros/src/emit/bridge.rs`; the core owner is
`concord_core/src/generated_contract.rs`, reexported by the façade bridge.
Change both only when their private call contract becomes incompatible, then
update the mismatch fixture and exact-version tests.

## Review checklist

- One syntax AST and one normalized model remain.
- `semantic::surface` remains the sole generated-name/capability owner.
- No semantic module imports `quote` outside tests.
- No emitter returns `syn::Error` or validates user syntax.
- No production macro branch names an extras implementation.
- Generated paths use the resolved façade.
- Generated descriptions and docs contain only safe semantic projections.
- Existing UI text/spans and expanded tokens are unchanged unless the public
  contract intentionally changed and its snapshots were individually reviewed.

The manual rust-analyzer protocol remains necessary at release gates because
compiler tests do not measure completion ranking or navigation quality.
