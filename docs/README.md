# Concord DSL Book

Concord is a Rust API-client generator built around the `api!` macro. You describe the shape of an HTTP API in a compact DSL, and Concord generates strongly typed endpoint structs plus a runtime client that handles routing, policy inheritance, authentication, retries, rate limits, caching, pagination, decoding, and testing hooks.

This directory is written as a small book. Each chapter focuses on one concept and uses examples based on the code and tests in `concord_examples`.

Start with:

- [Introduction](01-introduction.md)
- [Client Blocks](02-client.md)
- [Routing and Endpoints](03-routing-and-endpoints.md)
- [Runtime Client](12-runtime-client.md)

For focused topics:

- [Parameters, Variables, and Values](04-params-vars-and-values.md)
- [Headers, Query, and Timeout](05-headers-query-timeout.md)
- [Bodies, Responses, and Mapping](06-bodies-responses-mapping.md)
- [Authentication](07-authentication.md)
- [Retry](08-retry.md)
- [Rate Limiting](09-rate-limiting.md)
- [Caching](10-cache.md)
- [Pagination](11-pagination.md)
- [Testing and Debugging](13-testing-and-debugging.md)
- [Customization and Extension Points](14-customization.md)

Most examples assume these imports:

```rust
use concord_core::prelude::*;
use concord_macros::api;
```

The `Json<T>` codec requires the `concord_core/json` feature. The built-in Moka cache backend requires the `concord_core/cache-moka` feature.
