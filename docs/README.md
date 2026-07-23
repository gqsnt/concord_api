# Concord documentation

Concord is a strongly typed HTTP API client generator with one managed, bounded execution lifecycle. The manual is organized by how you use the project rather than by its internal implementation history.

## Choose a path

| Reader | Start here | Continue with |
|---|---|---|
| New user | [Getting started](getting-started.md) | [Basic usage](basic/README.md) |
| Application developer | [DSL and requests](basic/dsl-and-requests.md) | [Bodies and responses](basic/bodies-and-responses.md), [authentication](basic/authentication.md), [pagination and rate limiting](basic/pagination-and-rate-limiting.md) |
| Extension author | [Advanced usage](advanced/README.md) | [Extension boundaries](advanced/endpoints-bodies-and-codecs.md), [response specifications](advanced/responses-records-and-streams.md), [custom providers and hooks](advanced/authentication-hooks-and-debug.md) |
| Evaluator or architect | [Concepts](concepts/README.md) | [Design and lifecycle](concepts/design-and-lifecycle.md), [security and limitations](concepts/security-bounds-and-limitations.md) |
| Contributor | [Developer documentation](developer/README.md) | [Architecture](developer/architecture-and-contributing.md), [testing and tooling](developer/testing-and-tooling.md) |

## Capability map

Concord supports generated and explicit endpoint values, runtime-defined endpoints, bounded request and response bodies, JSON, text, bytes, streaming, multipart requests, typed CSV and NDJSON records, authentication, pagination, lexical rate limits, custom providers, custom limiters, hooks, and opt-in dangerous response/retry surfaces.

Every capability is documented with its lifecycle, feature requirements, finite limits, extension responsibilities, and unsupported forms. The consolidated [API reference](reference/api.md) maps public namespaces and generated capabilities; the [DSL reference](reference/dsl.md) owns exact macro syntax.

## Documentation paths

- [Getting started](getting-started.md)
- [Basic usage](basic/README.md)
- [Advanced and extensible usage](advanced/README.md)
- [Practical guides](guides/README.md)
- [Concepts and design](concepts/README.md)
- [Reference](reference/README.md)
- [Developer documentation](developer/README.md)

## How to read the manual

Basic pages use `concord_core::prelude` and the generated DSL. Advanced pages introduce `concord_core::advanced` only when an application must implement a trait or configure a lower-level semantic component. Exact Rust signatures belong to generated Rustdoc; the manual owns behavior, examples, limits, and tradeoffs.

All pages are portable Markdown. Mermaid diagrams are accompanied by prose so the material remains readable without diagram rendering.
