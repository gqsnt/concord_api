# Mental Model

Concord has four layers:

- DSL: a structured upstream API contract.
- Generated client: typed Rust navigation through that contract.
- Core runtime: syntax-neutral request plan execution.
- Macro compiler: transforms the contract into generated client code and request plans.

## API Tree

```text
client root
  scope branch
    scope branch
      endpoint leaf
```

- `client` is the root and owns base URL, variables, credentials, profiles, and defaults.
- `scope` is a branch and can add host/path/auth/policy context.
- An endpoint stanza is a leaf and describes one HTTP operation.

## Planning And Execution

Generated endpoint code creates a request plan. The core runtime executes that plan with fixed ordering:

```text
plan -> auth -> cache -> inflight -> rate-limit -> transport -> classify -> retry/fallback -> decode
```

The runtime receives resolved semantic data. It does not need to know the DSL syntax that produced the plan.

## Facade First

Application code normally uses generated facade methods:

```rust
let user = api.users().get_user(42).await?;
```

Advanced endpoint values under `endpoints::*` are available for focused tests and explicit request planning.
