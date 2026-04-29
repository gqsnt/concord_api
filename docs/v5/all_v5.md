# Concord v5 Master Contract

v5 is a strict, facade-first API client generation model.

## Identity

```text
DSL
  -> RawAst
  -> NormApiTree
  -> ResolvedApi / ResolvedEndpoint
  -> generated Endpoint::plan
  -> RequestPlan
  -> ApiClient::execute_plan
```

## User Model

```text
client = root
scope = branch
endpoint = leaf
policy = inherited branch behavior
request = walking the generated tree to a leaf
```

## DSL Laws

- The DSL describes the upstream API contract.
- Route structure is explicit before formatting.
- `fmt[...]` is structured formatting, not Rust `format!`.
- Required values are direct arguments.
- Optional/default values are setters.
- Complex behavior is explicit.
- Endpoint declarations are stanzas.

## Generated Usage

```rust
let me = api.protected().me().await?;

let ids = riot
    .regional(region)
    .match_v5_matches()
    .get_match_ids_by_puuid(puuid)
    .count(100)
    .paginate()
    .collect()
    .await?;
```

Session auth uses generated acquisition helpers:

```rust
api.auth_api()
    .login_for_session(LoginRequest { username, password })
    .acquire_as_session()
    .await?;
```

## Core Invariant

The core executes `RequestPlan`. Generated endpoints implement `Endpoint::plan`. `PendingRequest` uses `ep.plan(...)` and `ApiClient::execute_plan(...)`.

## Macro Invariant

Codegen consumes resolved endpoint data, not raw parser syntax. Policy inheritance is resolved before codegen.

## Release Gate

The release gate is clean formatting, tests, clippy, docs, and strict grep audits over production code, examples, and public docs.
