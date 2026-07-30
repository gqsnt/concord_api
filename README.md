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
let client = UsersApi::new()?;
let user = client.user(42).await?;

let response = client.user(42).response().await?;
assert!(response.status().is_success());

let bytes = client.user(42).response_bytes().await?;
assert!(!bytes.value().is_empty());
# Ok(())
# }
```

Use `Api::new()` for a declaration with no required client inputs. Use the
generated builder when the contract requires values, secrets, providers, or
deployment configuration. Generated construction and navigation keep their
domain names: `UsersApi::builder()`, scope methods, endpoint methods, and
endpoint constructors still construct or navigate. Public value mutation is
explicit: use `.set_name(value)`, `.clear_name()` when the value is
absence-capable, and `.reset_name()` when it has a declaration default. These
capabilities are independent: a defaulted `Option<T>` exposes all three
controls, and its setter accepts `T`, not `Option<T>`. There are no legacy
bare mutation aliases. See the
[generated API reference](docs/reference/generated-api.md) for the exact
client/scope/endpoint/request matrix and reset semantics. Ordinary values may
use `= default` to invoke their Rust `Default` implementation at the existing
owner-specific evaluation point.

Each endpoint has a predictable direct method, public endpoint value, named
request wrapper, and safe description. Generated endpoints, explicit values,
and runtime-defined endpoints all converge on the same executor:

```text
remote contract
    -> declaration compiler
    -> typed client / endpoint / request API
    -> preparation, hooks, credentials, limiter, private HTTP
    -> declared status acceptance
    -> bounded typed or explicit-byte acquisition, completion
```

Construction is inert. HTTP, credentials, limiter acquisition, body factories,
stream polling, hooks, and completion begin only when a terminal is awaited.
Built clients are immutable runtime snapshots shared safely by clones. Native
transport objects and authentication material stay inside the runtime.

Response declarations choose delivery explicitly. `BufferedRecords<D>`
collects any record decoder into `Vec<D::Record>` through the buffered
lifecycle; `Records<D>` returns a lazy `RecordStream<D>`. First-party
`Csv<T>` and `NdJson<T>` are complete buffered response formats directly, and
their `Records<Csv<T>>` / `Records<NdJson<T>>` forms stream the same framing.
Bare `Stream` accepts any response media type, while `Stream<M>` preserves an
exact `MediaType` contract. CSV and NDJSON are response decoders only; Concord
does not provide CSV or NDJSON request encoding.

Ordinary default features install
`concord::rate_limit::LocalRateLimiter`, Concord's bounded process-local
rate-limit policy. Inline rules remain lexical shorthand. A client can also
define a reusable named rule once and attach it in unrelated branches with
`rate_limit use name`. Definition formals own the final logical key-part
names, so binding either `scope.account` or `ep.account` to formal `account`
uses the same bucket schema. A named rule's omitted ID derives as
`<client>.rate_limit.<rule>` and never depends on attachment location. See
[Rate limiting](docs/features/rate-limiting.md). Default builds remain
isolated by separate limiter objects; sharing one limiter across unrelated
contracts requires compatible rule namespaces.

## Documentation

The complete documentation index is [docs/SUMMARY.md](docs/SUMMARY.md).
Start with:

- [Complete documentation index](docs/SUMMARY.md)
- [Documentation site landing](docs/README.md)
- [What Concord is](docs/start/what-is-concord.md)
- [Install Concord](docs/start/install.md)
- [Your first API](docs/start/first-api.md)
- [Make the first request](docs/start/first-request.md)
- [Choose your next path](docs/start/next.md)

Continue through the [Core Guide](docs/concepts/contract-model.md), choose a
[feature](docs/features/authentication.md) or
[recipe](docs/recipes/regional-apis.md), and use the
[DSL reference](docs/reference/dsl.md),
[generated API reference](docs/reference/generated-api.md), and
[feature table](docs/reference/features.md) for exact facts.

Maintainers and forks start with the
[architecture overview](docs/contributor/architecture.md), then use
[runtime contracts](docs/contributor/contracts/authentication.md) and the
[workspace guide](docs/contributor/workspace.md). Build the complete local
site with `just site`; preview it with `just site-serve`. Both use the root
`book.toml`, source from `docs/`, and the pinned mdBook 0.5.4 workflow.

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

Unit, integration, example, and UI tests use cargo-nextest. Install a prebuilt
cargo-nextest 0.9.140 or newer before running the test recipes; on Windows,
`winget install nextest.cargo-nextest` installs the official prebuilt package.
Linux and macOS contributors can use the
[official prebuilt archive installer](https://nexte.st/docs/installation/pre-built-binaries/).
The repository configuration rejects older nextest versions. Do not compile
cargo-nextest from source with Concord's MSRV toolchain.

Run `just --list` for the maintained inventory. The primary gates are:

```text
just fmt
just check
just lint
just test
just ui
just msrv
just fixtures
just site
just site-serve
just docs
just package
just deny
just workflow
just analyzer
just bench
just verify
just source-archive
```

Tests are deterministic and make no external service calls. `just test` and
`just ui` use nextest with retries disabled; `just docs` keeps doctests under
Cargo test because nextest does not execute them. `just verify` includes
workflow linting; its benchmark owner compiles the observational benchmark
without running it. `just source-archive` creates the tracked-source
review/release artifact at `target/concord-source.zip`, including from a fresh
clone. Share that Git source archive rather than a ZIP of the working
directory. Local checks cannot prove remote Actions execution, other operating
systems, interactive editor behavior, registry ownership, or legal approval.
