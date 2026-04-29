# Concord v5 Core Contract

The core remains plan-based.

Generated endpoint code ends in:

```text
Endpoint::plan -> RequestPlan -> ApiClient::execute_plan
```

The core must not know DSL syntax such as endpoint stanzas, `fmt[...]`, query shorthand, or parser compatibility layers.

`EndpointPlan` carries:

```text
meta
route
policy
body
response
pagination
```

`RequestPlan` carries:

```text
endpoint
args
overrides
```

Retry semantics are expressed by v5 DSL as `max_attempts`; lower-level runtime field names may be internal implementation details until the core-alignment milestone.

Public API layers:

- `prelude`: normal generated-client users
- `advanced`: extension authors and tests
- `internal`: generated code and crate internals

Do not claim complete unless fmt, tests, clippy, docs, and grep audits are clean.
