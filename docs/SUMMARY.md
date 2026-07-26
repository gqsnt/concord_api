# Summary

- [Concord](README.md)

# Start

- [Start: What Concord is](start/what-is-concord.md)
  - [Install Concord](start/install.md)
  - [Your first API](start/first-api.md)
  - [Make the first request](start/first-request.md)
  - [Choose your next path](start/next.md)

# Core guide

- [The contract model](concepts/contract-model.md)
- [Clients, scopes, and endpoints](guide/clients-scopes-endpoints.md)
- [Values, defaults, and controls](guide/values-and-controls.md)
- [Shape the public request](guide/request-shaping.md)
- [Understand generated Rust](guide/generated-rust.md)
- [Build and share clients](guide/client-construction.md)
- [Requests, planning, and errors](guide/requests-planning-errors.md)
- [The managed lifecycle](concepts/managed-lifecycle.md)

# Features

- [Authentication](features/authentication.md)
- [Credential providers and OAuth2](features/credential-providers.md)
- [Request bodies and buffered responses](features/bodies-and-buffered-responses.md)
- [Streaming responses](features/streaming.md)
- [Record streams](features/records.md)
- [Pagination](features/pagination.md)
- [Rate limiting](features/rate-limiting.md)
- [Multipart uploads](features/multipart.md)
- [Hooks and observability](features/hooks-and-observability.md)
- [Runtime-defined endpoints](features/runtime-endpoints.md)
- [Production HTTP](features/production-http.md)

# Recipes

- [Regional APIs](recipes/regional-apis.md)
- [API-key authentication](recipes/api-key-authentication.md)
- [Refresh OAuth credentials](recipes/oauth-refresh.md)
- [Paginate all results](recipes/paginate-all.md)
- [Per-host and per-account rate limits](recipes/rate-limit-partitioning.md)
- [Stream CSV or NDJSON records](recipes/stream-records.md)
- [Testing API clients](recipes/testing-api-clients.md)
- [Production configuration](recipes/production-configuration.md)
- [Riot-style client](recipes/riot.md)

# Extending Concord

- [Extension model](extending/README.md)
- [Codecs](extending/codecs.md)
- [Request and response body specifications](extending/body-specifications.md)
- [Record decoders](extending/record-decoders.md)
- [Credentials and providers](extending/credentials-and-providers.md)
- [Pagination controllers](extending/pagination.md)
- [Rate limiters and observers](extending/rate-limiting.md)
- [Hooks, trusted work, and bounds](extending/hooks-and-trusted-work.md)

# Reference

- [Complete DSL](reference/dsl.md)
- [Generated API](reference/generated-api.md)
- [Feature flags](reference/features.md)
- [Limits](reference/limits.md)
- [Errors](reference/errors.md)
- [Compatibility and MSRV](reference/compatibility.md)
- [Finite behavior](concepts/finite-behavior.md)
- [Security and redaction](concepts/security-and-redaction.md)
- [Dangerous capabilities](reference/dangerous.md)
- [Unsupported forms](reference/unsupported.md)

---

# Architecture

- [Architecture overview](contributor/architecture.md)
- [Crate boundaries](contributor/architecture/crate-boundaries.md)
- [Compile-time to runtime flow](contributor/architecture/compile-runtime-flow.md)
- [Generated surface and contract](contributor/architecture/generated-surface.md)
- [Managed executor](contributor/architecture/managed-executor.md)
- [Feature composition and evolution](contributor/architecture/feature-composition.md)

# Compiler

- [Macro pipeline](contributor/macro-pipeline.md)
- [Generated contract versioning](contributor/generated-contract.md)

# Runtime contracts

- [Authentication](contributor/contracts/authentication.md)
- [Bodies, responses, and finite progress](contributor/contracts/body-response-progress.md)
- [Records and pagination](contributor/contracts/records-pagination.md)
- [Rate limiting](contributor/contracts/rate-limits.md)
- [Errors, redaction, hooks, and completion](contributor/contracts/errors-hooks-completion.md)
- [Extension contracts](contributor/contracts/extensions.md)

# Development

- [Workspace and adding a feature](contributor/workspace.md)
  - [Adding a feature](contributor/adding-a-feature.md)
- [Testing strategy](contributor/testing.md)
- [Performance and scale](contributor/performance.md)
- [Documentation maintenance](contributor/documentation.md)
- [Release process](contributor/release.md)
