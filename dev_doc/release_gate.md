# Local v1 release gate

This gate is local workspace validation only. It does not package, publish,
tag, or run any crates.io step. The default gate does not require external
credentials or network access.

Run:

```bash
bash ./scripts/check_v1.sh
```

The commands below are the full local gate. `./scripts/check_v1.sh` requires
`cargo-nextest` and runs the scripted gate. The release gate is enforced
primarily by behavior tests, trybuild diagnostics, generated API compile
checks, clippy, and rustdoc. Manual review covers source and documentation
consistency.

## Core invariants

- `BuiltRequest` is logical request state and does not contain raw auth material.
- `TransportRequest` is the only Concord-owned request type that receives materialized auth values.
- `BuiltResponse`, `DecodedResponse<T>`, debug hooks, and cache keys operate on logical or redacted request data.
- Runtime request paths return typed errors for poisoned state, missing required rate-limit keys, body-limit violations, and semantic counter overflow.

## Macro/codegen invariants

- Raw parser AST may contain rejected forms, but semantic resolution lowers public policy, route, and pagination data into context-specific IR.
- Public policy, route, and pagination IR cannot carry auth-secret value variants.
- Optional policy values are represented directly in the value IR, not by independent conditional metadata.
- Codegen must not rely on validation-dependent `expect(...)` or `unreachable!()` for sema-invalid states.
- Generated construction of retry statuses, rate-limit numeric values, and OAuth token URLs is fallible if an internal invariant is violated.

## Runtime strictness invariants

- No synthetic rate-limit key fallback such as `"<unknown-host>"` is introduced.
- Semantic runtime/config values do not use silent saturating arithmetic.
- Public request execution and generated helper paths do not panic on runtime lock poisoning.
- Full response-body reads are bounded.

## Auth/redaction invariants

- Auth preparation receives an auth-only application request, not `BuiltRequest`.
- Endpoint auth and internal auth attach typed pending auth slots and carry raw material as sidecar data until transport materialization.
- Arbitrary auth header/query names are protected structurally, not only by sensitive-name guesses.
- OAuth client secrets, bearer tokens, Basic auth usernames and passwords declared as secrets, query auth values, and header auth values are absent from Concord debug/errors/hooks.

## Cache/retry/rate-limit invariants

- Auth resolution remains before cache identity.
- Cache identity remains separated by safe auth partitions without raw credential values.
- Ordinary endpoint requests are not deduplicated while in flight. Cache identity remains relevant only for completed cache entries.
- Concurrent cache misses are not runtime-coalesced; concurrent fresh cache hits bypass transport. Credential slots may single-flight acquisition/refresh for the same refreshable credential.
- Rate-limit `[host]` keys fail before permit acquisition and transport if the logical URL has no host.
- Retry/auth refresh semantics remain bounded and policy-driven.

## Body-limit invariants

- Endpoint response reads use the runtime response body limit.
- Auth HTTP/token response reads use the smaller auth body limit.
- Cache `max_body` remains a storage limit only and is not the decode/read limit.
- Too-large responses fail before decode and before cache write.

## Command list

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace --all-targets
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
bash ./scripts/check_v1.sh
```

Trybuild remains part of the full gate through
`cargo nextest run --workspace --all-targets`. It is serialized in
`.config/nextest.toml` with the `trybuild` test group because it drives many
rustc fixture compilations.

## Trybuild Snapshot Refresh

Trybuild fixtures are split by public UI-contract category under
`concord_macros/tests/trybuild/`.

The current trybuild test functions are:

- `trybuild_pass_contract_fixtures`
- `trybuild_auth_and_secret_diagnostics`
- `trybuild_route_and_fmt_diagnostics`
- `trybuild_policy_diagnostics`
- `trybuild_pagination_diagnostics`
- `trybuild_codegen_contract_diagnostics`

Run the full trybuild suite with:

```bash
cargo nextest run -p concord_macros --test trybuild_current
```

Snapshot refresh is not part of the default gate. Only use
`TRYBUILD=overwrite` when macro diagnostics intentionally change:

```bash
TRYBUILD=overwrite cargo test -p concord_macros --test trybuild_current -- --test-threads=1
```

Category-specific refresh examples:

```bash
TRYBUILD=overwrite cargo test -p concord_macros --test trybuild_current trybuild_auth_and_secret_diagnostics -- --test-threads=1
TRYBUILD=overwrite cargo test -p concord_macros --test trybuild_current trybuild_route_and_fmt_diagnostics -- --test-threads=1
TRYBUILD=overwrite cargo test -p concord_macros --test trybuild_current trybuild_policy_diagnostics -- --test-threads=1
TRYBUILD=overwrite cargo test -p concord_macros --test trybuild_current trybuild_pagination_diagnostics -- --test-threads=1
TRYBUILD=overwrite cargo test -p concord_macros --test trybuild_current trybuild_codegen_contract_diagnostics -- --test-threads=1
```

Inspect the git diff of `.stderr` files before accepting updates. Path-only
changes from fixture moves are acceptable; changed wording/spans must be
reviewed.

After refreshing snapshots, rerun the full local v1 gate.
