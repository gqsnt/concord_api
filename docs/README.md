# Concord Documentation

Concord is a Rust API-tree DSL and contract compiler that generates a facade-first typed client over a syntax-neutral, plan-based HTTP runtime.

## Guides

- [Quick Start](quick_start.md)
- [Mental Model](mental_model.md)
- [Design Invariants](design_invariants.md)
- [DSL](dsl.md) - complete public DSL reference
- [Generated Client](generated_client.md)
- [Auth](auth.md)
- [Pagination](pagination.md)
- [Customization](customization.md)
- [Cache, Retry, And Rate Limit](cache_retry_rate_limit.md)
- [Feature Matrix](features.md)
- [Runtime Config](runtime_config.md)
- [Public Errors](errors.md)
- [Advanced Endpoints](advanced_endpoints.md)
- [Internals](internals.md)

Developer architecture notes live in [`../dev_doc/`](../dev_doc/).

Compile-checked public DSL examples live in [`../concord_examples/src/docs_dsl.rs`](../concord_examples/src/docs_dsl.rs) and [`../concord_examples/src/docs_advanced_dsl.rs`](../concord_examples/src/docs_advanced_dsl.rs). The Riot Web API fixture in [`../concord_examples/src/riot.rs`](../concord_examples/src/riot.rs) and the Data Dragon fixture in [`../concord_examples/src/ddragon.rs`](../concord_examples/src/ddragon.rs) remain the large real-world examples. Their manual smoke functions are gated by environment variables and are not run by tests or normal example execution.
