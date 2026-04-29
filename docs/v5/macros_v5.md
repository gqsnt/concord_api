# Concord v5 Macro Contract

The macro is a compiler pipeline:

```text
tokens
  -> RawAst
  -> NormApiTree
  -> ResolvedApi / ResolvedEndpoint
  -> codegen
```

Raw AST is parser-owned. Codegen consumes resolved data only and must not import parser AST modules.

## Codegen Inputs

Codegen works from:

- `ResolvedApi` for client-level data;
- `ResolvedEndpoint` for endpoint structs, facade methods, and `Endpoint::plan`;
- resolved route, policy, auth, body, response, and pagination specs.

## Required Contracts

- endpoint stanzas are canonical;
- `fmt[...]` is parsed and resolved as one atom;
- query shorthand normalizes to explicit query assignment;
- `max_attempts` is the retry count field;
- only one `default` block is allowed per node;
- facade, builders, auth acquire helpers, pagination, endpoint plans, and rustdoc are generated from resolved data;
- generated endpoints implement `Endpoint::plan` and produce `RequestPlan`.

## Boundary Rule

Shared syntax-neutral primitives belong in the resolved model layer. Parser-only structures stay behind the parse/sema boundary.
