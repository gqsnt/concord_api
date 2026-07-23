# Concord

Concord compiles a remote HTTP contract into an ordinary typed Rust API and
executes every request through one bounded, redaction-aware managed lifecycle.
Applications depend on one package:

```toml
[dependencies]
concord = { version = "0.1.0", features = ["json"] }
```

```rust
use concord::{Json, api};
use serde::Deserialize;

#[derive(Deserialize)]
struct User {
    id: u64,
}

api! {
    client UsersApi {
        base "https://api.example.test/";
    }

    GET user(id: u64) -> Json<User> {
        path ["users", ep.id];
    }
}

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let client = UsersApi::builder().build()?;
let user = client.user(42).await?;

let endpoint = users_api::User::new(42);
let response = client.request(endpoint).response().await?;
assert!(response.status().is_success());
# Ok(())
# }
```

Each endpoint has a predictable direct method, public endpoint value, named
request wrapper, and safe description. Generated endpoints, explicit values,
and runtime-defined endpoints all converge on the same executor:

```text
remote contract
    -> declaration compiler
    -> typed client / endpoint / request API
    -> preparation, hooks, credentials, limiter, private HTTP
    -> bounded acquisition, typed result, completion
```

Construction is inert. HTTP, credentials, limiter acquisition, body factories,
stream polling, hooks, and completion begin only when a terminal is awaited.
Built clients are immutable runtime snapshots shared safely by clones. Native
transport objects and authentication material stay inside the runtime.

## Documentation

Start with:

- [Represent an API](docs/guide/01-represent-an-api.md)
- [Understand generated Rust](docs/guide/02-understand-generated-rust.md)
- [Build a client and make requests](docs/guide/03-build-a-client-and-make-requests.md)

Then use the [manual index](docs/README.md), [DSL reference](docs/reference/dsl.md),
[generated API reference](docs/reference/generated-api.md), and
[feature table](docs/reference/features.md).
Existing workspace users should also read the [migration guide](MIGRATION.md)
and [changelog](CHANGELOG.md).

## Workspace

```text
crates/concord/               User façade
crates/concord-core/          Semantic runtime kernel and extension traits
crates/concord-extras/        Downstream implementations of core traits
crates/concord-macros/        Extras-agnostic declaration compiler
crates/concord-examples/      Importable, deterministic product examples
crates/concord-test-support/  Internal deterministic test infrastructure
fixtures/                     Locked Cargo-graph fixtures
tools/xtask/                  Portable documentation and fixture checks
tools/perf/                   Internal performance probes
```

The product packages currently remain `publish = false`; the
[release guide](docs/contributor/release.md) records the metadata and ownership
that must be approved before publication.

## Contributor commands

Run `just --list` for the maintained inventory. The primary gates are:

```text
just fmt
just check
just test
just test-all
just ui
just fixtures
just docs
just security
just verify
```

Tests are deterministic and make no external service calls. Performance smoke
checks are observational and intentionally separate from `just verify`.
