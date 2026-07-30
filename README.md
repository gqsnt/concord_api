# Concord

Concord is a proc macro that compiles a declarative HTTP API contract into a
typed Rust client. The runtime executes resolved request plans and never sees
DSL syntax.

```rust
use concord::{Bytes, api};

api! {
    client StatusApi {
        base "https://status.example.test/";
    }

    GET status() -> Bytes {
        path ["v1", "status"];
    }
}

# async fn run() -> Result<(), Box<dyn std::error::Error>> {
let client = StatusApi::new()?;
let bytes = client.status().await?;
# let _ = bytes;
# Ok(())
# }
```

Request construction is inert. Awaiting a request starts public request
shaping, hooks, credential acquisition, rate-limit admission, private
authentication placement, HTTP, bounded response acquisition, decoding, and
completion.

## Use from this checkout

```toml
[dependencies]
concord = { path = "../concord_api/crates/concord", features = ["json"] }
```

The packages have `publish = false`.

## Documentation

- [Quick start](docs/quick_start.md)
- [DSL specification](docs/spec.md)
- [Runtime model](docs/runtime.md)
- [Authentication](docs/auth.md)
- [Bodies and responses](docs/bodies_and_responses.md)
- [Pagination](docs/pagination.md)
- [Retry and rate limiting](docs/retry_and_rate_limit.md)
- [Generated client](docs/generated_client.md)
- [Advanced endpoints](docs/advanced_endpoints.md)
- [Customization](docs/customization.md)
- [Design invariants](docs/design_invariants.md)
- [Maintainer internals](dev_doc/internals.md)
- [Finalization decisions](dev_doc/finalization_decisions.md)

## Validation

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo doc --workspace --no-deps
pwsh -NoLogo -NoProfile -File scripts/check_docs_examples.ps1
```

Internal project. v1 final. Not published.
