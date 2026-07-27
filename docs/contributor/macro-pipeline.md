# Macro pipeline

The declaration compiler is a staged compiler:

```text
façade input
    -> parse source syntax
    -> resolve and validate semantics
    -> derive public surface and Rustdoc facts
    -> emit Rust
```

Each stage narrows responsibility. Parsing preserves what the author wrote;
semantic resolution decides what it means; surface derivation decides which
Rust artifacts exist; emission renders those decisions. A later stage must not
reconstruct an earlier stage's facts.

The concrete [compile-time to runtime trace](architecture/compile-runtime-flow.md)
uses `MinimalApi::health` as an end-to-end example.

## Façade input

Applications call `concord::api!`. The wrapper invokes the hidden proc macro
with:

- `@concord_facade($crate)`, the dependency-renaming-safe emission root;
- the original declaration tokens.

The hidden proc macro is the implementation crate's only proc-macro entry.
The generated inherent surface is feature-neutral. Cargo-selected
implementations remain ordinary Rust items and trait implementations rather
than a second private proc-macro protocol.

## Parsing

`crates/concord-macros/src/syntax` converts tokens into a source AST with
precise spans. Its responsibilities include:

- declaration grammar and delimiters;
- exactly one leading client declaration;
- source identifiers, Rust types, and literal forms;
- duplicate named-clause detection;
- preserving ordered lists such as path atoms, authentication placements,
  rate windows, and key dimensions;
- collecting order-independent named clauses without assigning semantics.

Parsing does not decide inheritance, type capability, generated names, or
runtime meaning. A parser error should point to the token that violates the
grammar. When a clause is duplicated, diagnostics retain both the first and
duplicate spans.

### Parser change test owners

- focused syntax unit tests live beside the grammar domains;
- accepted complete declarations live under
  `crates/concord/tests/ui/pass/language`;
- malformed public syntax and stable stderr live under
  `crates/concord/tests/ui/fail/syntax`.

## Semantic resolution

`crates/concord-macros/src/semantic` converts the source AST into a resolved
model that no longer depends on source grammar enums. It owns:

- lexical symbol tables for client, scope, and endpoint values;
- optional/default value capability and exact setter argument types;
- normalized public-head operations after inheritance;
- credential graph resolution and cycle checks;
- pagination bindings and compatible response requirements;
- rate-limit applicability, rule identity, ordered dimensions, and windows;
- final host-shaping and pagination/rate-key compatibility;
- endpoint contract IDs and description facts;
- structural first-excess checks whose bounds must align with core.

Validation occurs when enough information is available. Lexical references are
resolved against their declaring owner. Cross-layer rules are checked against
the final effective endpoint, after descendant shaping and pagination
bindings are known.

For example, an inherited rate rule keeps the dimensions resolved at its
lexical owner, but its prohibition on a pagination-controlled dimension is
checked against the final endpoint. A DNS base may accept host labels; an IPv4
or IPv6 base requires the final effective host-label list to be empty.

### Semantic change test owners

- domain unit tests cover resolution and exact boundary behavior;
- `semantic/tests.rs` covers cross-domain invariants and first excess;
- facade UI failures under authentication, names, pagination, and rate limits
  prove downstream diagnostics;
- runtime tests prove the accepted model has the documented behavior.

## Surface derivation

The surface layer converts resolved meaning into exact Rust API facts:

- client builder and support-module names;
- scope, endpoint, and named-request names and paths;
- required constructor arguments;
- consuming `set_*`, `clear_*`, and `reset_*` controls;
- clone eligibility;
- direct/response/response-bytes/pagination/credential-storage terminals;
- sealed endpoint associations;
- structured `EndpointDescription` and generated Rustdoc content.

Absence capability, declaration-default capability, and setter type are
independent facts. A defaulted `Option<T>` therefore receives all of
`set_name(T)`, `clear_name()`, and `reset_name()`. Collision validation uses
those exact derived identifiers.

The normal response-bytes terminal is always part of the surface. Its sealed
`ResponseBytesMode` bound admits buffered and discard response modes and makes
streaming use fail during Rust type checking.

### Surface change test owners

- `semantic/surface.rs` tests assert names, paths, terminal matrices, and
  documentation models;
- emission tests assert those decisions become matching Rust;
- facade Trybuild tests prove both present and deliberately absent methods;
- warning-denied generated Rustdoc catches broken surface documentation.

## Emission

`crates/concord-macros/src/emit` renders the resolved model through the supplied
façade path. It emits:

- client/builder storage and construction;
- scope snapshots and navigation;
- explicit endpoint values and named requests;
- semantic private trait implementations;
- credential handles, descriptions, and generated Rustdoc;
- contract-version and structural-limit assertions.

Emission must not:

- import source grammar;
- discover a new user error;
- silently select fallback semantics;
- emit direct `concord_core` or `concord_extras` paths;
- recognize first-party codec/provider/controller type names;
- decide capabilities from token spelling.

The Rust compiler remains responsible for user type and trait errors. If a
declared type does not implement `ResponseBodySpec`, preserve that focused Rust
diagnostic instead of inventing macro type recognition.

### Emitter change test owners

- module-local token tests check focused output;
- complete-generation tests check composition and forbidden paths;
- facade public API/runtime tests compile and execute emitted Rust;
- the renamed-façade fixture proves all paths resolve through the application
  dependency;
- policy validation scans emission sources for direct core paths.

## Diagnostic contract

Diagnostics use declaration and public API vocabulary:

- syntax errors point at grammar tokens;
- duplicate clauses show original and duplicate spans;
- semantic errors identify source names and relationships;
- Rust type/trait/feature errors remain Rust diagnostics at the declared type;
- generated privacy errors should reveal no stable hidden API to call;
- diagnostics never include runtime values or protected material.

Trybuild `.stderr` is reviewed output, not an automatically accepted snapshot.
Read the complete compiler message after every diagnostic change. Test
compiler-enforced absence with code that fails to compile rather than with a
source-text assertion.

## Change routing

| Desired change | First owner | Downstream work |
|---|---|---|
| accept a new token form | syntax | semantic meaning, UI pass/fail |
| change inheritance or cross-layer validity | semantic | surface/runtime proofs as affected |
| rename/add a generated method | surface | collision validation, emission, UI/Rustdoc |
| change only emitted implementation syntax | emission | generated behavior and bridge review |
| add a concrete codec/provider/policy | core trait or existing trait + extras | facade feature/reexport; usually no macro change |
| change hidden compiler/runtime call | bridge owners | atomic generated-contract bump and fixture |

Do not split modules only because they are large. Split when a stable semantic
domain gains an independent invariant and test owner.

## Generated contract decision

Current generated-contract version 30 adds the hidden
`PageRequestBodySpec` obligation for generated pagination and admits the new
repeatable-body endpoint shape. It prevents an older expansion, which assumed
all pagination was bodyless, from claiming compatibility with the new
façade/core protocol. Version 29 removed the old generated native-retry
baseline and added the managed `.retry(RetryPolicy)` builder surface.

The private contract does not change for:

- a source-language acceptance/diagnostic change;
- the public `set_*` rename and independently derived clear/reset controls;
- final semantic pagination validation;
- generated Rustdoc improvements;
- an internal refactor behind identical emitted calls.

It does change when emitted code and the façade/core bridge require a new or
incompatible private operation. Use the canonical
[Generated contract versioning](generated-contract.md) checklist rather than
copying a partial bump procedure into a compiler change.

## Verification

```bash
cargo test -p concord-macros --lib --locked
cargo test -p concord --test ui --all-features --locked
cargo run --locked -p concord-xtask -- fixtures --locked
cargo run --locked -p concord-xtask -- policy --locked
cargo rustdoc -p concord --all-features --locked -- -D warnings
rust-analyzer analysis-stats . --run-all-ide-things
```

Macro changes should also run the workspace doctests because generated
signatures and public trait bounds can invalidate otherwise distant examples.

## Related

- [Generated surface and contract](architecture/generated-surface.md)
- [Generated contract versioning](generated-contract.md)
- [Complete DSL reference](../reference/dsl.md)
- [Generated API reference](../reference/generated-api.md)
- [Testing](testing.md)
