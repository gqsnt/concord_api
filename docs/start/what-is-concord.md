# What Concord is

Use Concord when you want a Rust client whose public API reflects a stable
remote HTTP contract and whose network effects stay inside one managed
runtime.

The core transformation is:

```text
HTTP API contract
    → concord::api! declaration
    → generated client, scope, endpoint, and request types
    → inert request value
    → managed HTTP execution
```

You describe the remote contract once: the base URL, reusable scopes, endpoint
values, public request shaping, authentication placement, response
representation, pagination, timeouts, and lexical rate limits. Concord
generates the Rust names and capabilities that follow from that declaration.
An endpoint that has no pagination declaration has no pagination terminal; a
bodyless endpoint does not ask for a body value.

## What Concord handles

Concord owns the work that must remain consistent across every request:

- validating and encoding the public request head;
- keeping credentials out of public URL and header expressions;
- applying finite request and response limits;
- ordering hooks, authentication, limiter work, transport, decoding, and
  completion;
- reducing failures to safe typed categories;
- routing generated endpoints, explicit endpoint values, runtime-defined
  endpoints, and pagination pages through the same executor.

Constructing a client, endpoint, or request performs no HTTP. Execution begins
only when an async terminal is polled. The built client is an immutable
runtime snapshot, and clones share that snapshot.

## What remains ordinary Rust

The declaration compiler does not recognize application type names. Request
and response types satisfy public Rust traits. Credential providers,
pagination controllers, record decoders, rate limiters, observers, and hooks
are also ordinary trait implementations. First-party JSON, CSV, NDJSON,
OAuth2, and local rate-limit implementations use the same semantic contracts
available to applications.

Rustdoc remains the authority for exact item signatures, associated types, and
method-level requirements. This book owns reader paths, system behavior,
feature usage, composition, and architecture.

## What Concord is not

Concord is not an HTTP server framework, an OpenAPI generator, a general
Reqwest wrapper, or a way to obtain native requests and responses from every
extension. It deliberately keeps native transport objects behind a private
boundary. Safe plans show the public method, URL shape, public headers,
timeout selection, endpoint identity, and capabilities; they are not exact
authenticated request previews.

It also does not make arbitrary extension work finite. Concord bounds the
semantic inputs and the lifecycle it owns, but a custom decoder, callback, or
distributed limiter must bound its own CPU, memory, retries, queues, storage,
and synchronous work.

## Good fits

Concord works well for typed HTTP APIs with stable endpoint families,
including:

- JSON or buffered binary request/response APIs;
- bounded byte or record streams;
- API-key, Bearer, Basic, provider-backed, or OAuth2 credentials;
- sequential page, offset, or cursor pagination;
- lexical per-client, per-scope, or per-endpoint rate policy;
- dynamic public endpoint shapes that still have typed body and response
  contracts.

Protocols requiring native transport ownership, unbounded acquisition,
WebSocket management, or arbitrary request signing are outside the current
contract. Review [Unsupported forms](../reference/unsupported.md) before
designing around a boundary.

## The private transport boundary

Public shaping can use declared public values only. Authentication material is
resolved later and placed immediately before transport. Hooks, request plans,
descriptions, errors, and completion events receive sanitized semantic data,
not credentials or native transport values. This boundary is what lets
Concord apply one redaction and lifecycle policy everywhere.

## Related

- [Install Concord](install.md)
- [The contract model](../concepts/contract-model.md)
- [Security and redaction](../concepts/security-and-redaction.md)
- [Unsupported forms](../reference/unsupported.md)
- [Architecture overview](../contributor/architecture.md)
