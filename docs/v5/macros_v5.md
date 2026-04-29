# Concord v5 Macro Contract

The macro is a compiler pipeline:

```text
tokens
  -> RawAst
  -> legacy diagnostics / v5 validation
  -> NormApiTree
  -> ResolvedApi / ResolvedEndpoint
  -> codegen
```

Raw AST may be syntax-shaped and may contain removed forms only to produce diagnostics.

Codegen must consume resolved data only. It must not import raw parser structures such as:

```text
ClientDef
LayerDef
EndpointDef
AuthBlock
RetryProfilesBlock
CacheProfilesBlock
RateLimitProfilesBlock
LegacySyntax
```

Shared syntax-neutral primitives belong in a model module, not in raw AST.

Required macro contracts:

- endpoint stanzas are canonical;
- `fmt[...]` is parsed and resolved as one atom;
- query shorthand normalizes to explicit query assignment;
- `max_attempts` is accepted and `attempts` is rejected;
- only one `default` block is allowed per node;
- old syntax emits migration diagnostics;
- facade, builders, auth acquire helpers, pagination, endpoint plans, and rustdoc are generated from resolved data.
