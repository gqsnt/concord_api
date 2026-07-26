# Concord

Concord compiles a remote HTTP contract into an ordinary typed Rust API and
executes every request through one bounded, redaction-aware managed lifecycle.
Applications depend on one package:

The packages are not currently published. For development from a checkout,
use the façade package by path (adjust the relative path for your workspace):

```toml
[dependencies]
concord = { path = "../path/to/concord/crates/concord", features = ["json"] }
```

After an approved future registry release, the intended dependency form is:

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

Generated construction and navigation keep their domain names:
`UsersApi::builder()`, scope methods, endpoint methods, and endpoint
constructors still construct or navigate. Public value mutation is explicit:
use `.set_name(value)`, `.clear_name()` for an optional value, and
`.reset_name()` for a defaulted value. There are no legacy bare mutation
aliases. See the [generated API reference](docs/reference/generated-api.md)
for the exact client/scope/endpoint/request matrix and reset semantics.

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

Ordinary default features install
`concord::rate_limit::LocalRateLimiter`, Concord's bounded process-local
rate-limit policy. A rule's lexical location determines where it applies. Its
optional `by` clause determines how applicable requests are partitioned into
local buckets; Concord never infers dimensions from a path or method. See
[Pagination and rate limits](docs/guide/06-pagination-and-rate-limits.md).
Generated declaration rule IDs are deterministic within one generated client
contract, not globally namespaced across unrelated `api!` expansions. Default
builds remain isolated by separate limiter objects; sharing one limiter across
unrelated contracts requires compatible rule namespaces.

## Documentation

Start with:

- [Represent an API](docs/guide/01-represent-an-api.md)
- [Understand generated Rust](docs/guide/02-understand-generated-rust.md)
- [Build a client and make requests](docs/guide/03-build-a-client-and-make-requests.md)

Then use the [manual index](docs/README.md), [DSL reference](docs/reference/dsl.md),
[generated API reference](docs/reference/generated-api.md), and
[feature table](docs/reference/features.md).
Current user-facing changes are summarized in the [changelog](CHANGELOG.md).

## Workspace

```text
crates/concord/               User façade
crates/concord-core/          Semantic runtime kernel and extension traits
crates/concord-extras/        Downstream implementations of core traits
crates/concord-macros/        Extras-agnostic declaration compiler
crates/concord-examples/      Importable, deterministic product examples
crates/concord-test-support/  Internal deterministic test infrastructure
fixtures/                     Locked Cargo-graph fixtures
tools/xtask/                  Typed documentation, graph, policy, and package checks
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
just lint
just test
just ui
just msrv
just fixtures
just docs
just package
just deny
just workflow
just analyzer
just bench
just verify
just source-archive
```

Tests are deterministic and make no external service calls. `just verify`
includes workflow linting; its benchmark owner compiles the observational
benchmark without running it. `just source-archive` creates the tracked-source
review/release artifact at `target/concord-source.zip`, including from a fresh
clone. Share that Git source archive rather than a ZIP of the working
directory. Local checks cannot prove remote Actions execution, other operating
systems, interactive editor behavior, registry ownership, or legal approval.
