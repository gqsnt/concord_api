# Contract

This chapter records the current Concord DSL contract. This is the only supported surface.

## Product Direction

Concord treats the DSL as a readable API contract:

1. The architecture is `client -> scope -> endpoint`.
2. Values have explicit owners: `vars`, `secret`, or endpoint/scope params exposed as `ep`.
3. Route and policy templates do not declare hidden parameters.
4. Generated clients expose typed endpoints with predictable constructors and setters.
5. Runtime lifecycle transitions stay explicit.

## Enforced DSL Rules

Endpoint-backed auth uses scoped endpoint identity:

```rust
credential session: Endpoint(auth::LoginForSession)
```

Policy blocks assign already-declared values. They do not declare parameters:

```rust
POST Create(idempotency_key: String) -> Json<()> {
    headers { "Idempotency-Key" = idempotency_key }
}
```

Route values are string literals, identifiers, scoped references, or `part[...]` composition:

```rust
scope platform(platform: PlatformRoute) {
    host[platform, "api"]
    path["lol"]

    GET Get(id: String) -> Json<Item> {
        path["items", id]
        query { "trace" = part["item-", id] }
    }
}
```

Removed forms are compile errors:

1. policy binds such as `"Header" as id: Type`
2. policy param declarations such as `id: Type`
3. route/template declarations such as `{id: Type}`
4. endpoint body/response/params block syntax from earlier internal drafts
5. scoped endpoint root aliases such as `endpoints::Ping` for `scope api { GET Ping ... }`

## Generated Endpoint Names

The public `endpoints` module mirrors the scope tree:

```rust
scope api {
    scope users {
        GET GetUser(id: u64) -> Json<User> { path["users", id] }
    }
}
```

Callers use:

```rust
api.request(endpoints::api::users::GetUser::new(42)).execute().await?;
```

Root endpoints remain available directly under `endpoints`. Scoped endpoints are not reexported at the root.

## Constructor Order

Required endpoint constructor args follow declaration order:

1. inherited scope params from outer to inner scope
2. endpoint signature params in written order
3. required body, when declared in the endpoint signature

Optional and defaulted params are set through builder methods.

## Compile-Time Contract

The macro validates static facts at compile time:

1. unknown variables, secrets, auth credentials, rate-limit keys, and auth endpoints
2. duplicate conflicting variable declarations
3. invalid retry/rate-limit shapes known from DSL literals
4. unsupported policy/route syntax
5. endpoint-backed auth recursion
6. endpoint-backed auth output that does not satisfy credential material bounds

Dynamic facts remain runtime errors with endpoint and method context.

## Runtime Contract

The generated wrapper builds typed endpoint values and delegates execution to `concord_core`.

The runtime order is:

1. build route, policy, body, retry, cache, and rate-limit plan
2. prepare auth
3. query cache
4. coordinate inflight requests
5. acquire rate-limit permits
6. send through transport
7. classify response
8. let auth inspect and possibly invalidate/retry
9. update cache
10. retry when policy allows
11. decode and map the response

A fresh cache hit returns before inflight, rate-limit, retry, and transport. Stale revalidation uses the normal send path.

## Validation

The current contract is covered by runtime tests and trybuild compile-fail tests in `concord_examples/tests`.

Expected full validation:

```powershell
cargo fmt --check
cargo check -p concord_core --no-default-features
cargo check -p concord_core --all-features
cargo check -p concord_examples --no-default-features --tests
cargo test --workspace
cargo test -p concord_examples --tests
cargo clippy --workspace --all-targets --all-features -- -D warnings
```
