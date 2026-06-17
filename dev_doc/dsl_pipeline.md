# DSL pipeline

The macro pipeline is staged so each layer has a narrow job. The parser output is the raw parser AST; sema turns it into the resolved semantic model.

```text
TokenStream
-> parser
-> raw AST
-> semantic resolution
-> resolved model
-> codegen
-> generated client
-> runtime request plan
```

## Parse phase

The parser turns tokens into a raw AST and emits syntax diagnostics. It should reject malformed syntax, unsupported stanzas, duplicate clauses inside one syntactic block, and bad list shapes such as empty or duplicate bracket lists.

The parser should not resolve profile names, merge inherited policy, or inspect runtime behavior.

## Raw AST

The raw AST preserves user-written structure closely enough for semantic analysis. It can normalize purely syntactic conveniences, but it should not become the final model consumed by codegen.

## Sema phase

Sema resolves names and turns raw declarations into the semantic model. Responsibilities include:

- client vars and secrets
- credentials and auth uses
- scope and endpoint arguments
- route atom references
- retry, cache, rate-limit, and behavior profile names
- `extends` inheritance and cycle detection
- policy inheritance and merge order
- behavior expansion
- behavior rustdoc metadata

Unknown profile diagnostics should generally be emitted here because sema has the profile maps and use sites.

## Codegen phase

Codegen translates the resolved model into Rust tokens: client structs, facade methods, endpoint builders, request plan construction, policy functions, auth state, pagination helpers, and rustdoc.

Codegen should not duplicate semantic merge rules. If generated output needs a resolved answer, compute that answer in sema.

## Runtime phase

The generated code builds `concord_core` request plans and calls the runtime. Runtime responsibilities begin after the generated client has constructed a plan. The core executes the plan without knowing DSL syntax.

## Diagnostics placement

- Syntax and span-local grammar errors: parser.
- Unknown names, inheritance cycles, invalid profile references: sema.
- Rust trait/type errors from generated user types: normal Rust compilation.
- Runtime failures such as missing credentials, transport errors, decode errors, and retry exhaustion: `concord_core`.

## Behavior profiles

Behavior profiles expand during sema. Their auth/cache/retry/rate-limit effects are merged into resolved policy data before codegen. Behavior names are also preserved as endpoint rustdoc metadata, deduped in stable first-seen order.
