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

- `client` is the root and owns base URL, variables, credentials, profiles, and the default attachment block.
- `scope` is a branch and can add host, path, auth, and policy context.
- An endpoint stanza is a leaf and describes one HTTP operation.

For larger clients, client configuration is usually grouped into `auth { ... }`,
`policies { ... }`, `profiles { ... }`, and `default { ... }`. Profiles give
semantic names to repeated auth and rate-limit combinations. Retry mode is
selected once while constructing the managed client, outside the endpoint DSL.

Profiles may extend other profiles; inheritance is resolved during semantic analysis before code generation.

## Planning And Execution

Generated endpoint code creates a request plan. The core runtime executes that plan with fixed ordering:

```text
logical call -> collision preflight -> provider preparation -> rate-limit -> sanitized hook -> secret materialization -> execution -> optional authentication recovery -> decode
```

Provider preparation may perform a separately bounded provider HTTP operation
through its own managed Reqwest client. That operation is outside the protected
endpoint's retry, hook, rate-limit, and pagination stages.

The runtime receives resolved semantic data. It does not need to know the DSL
syntax that produced the plan. Reqwest-internal resends occur inside an
execution and are not visible Concord executions.

Concord does not coalesce ordinary endpoint requests. Two identical requests
remain two request executions unless the application chooses a higher-level
reuse strategy outside Concord.

## Facade First

Application code normally uses generated facade methods:

```rust
let user = api.users().get_user(42).await?;
```

Advanced endpoint values under `endpoints::*` are available for focused tests and explicit request planning.
