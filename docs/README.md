# Concord

Concord turns an explicit HTTP API contract into a typed Rust client and runs
each request through one bounded, redaction-aware lifecycle.

The smallest useful declaration names a client, a fixed base URL, and an
endpoint:

```rust,ignore
{{#include ../crates/concord-examples/src/getting_started/minimal.rs:first_api}}
```

That declaration generates a `MinimalApi` builder, a client, an endpoint type,
and a named request wrapper. Application code builds the client and constructs
requests with ordinary inherent methods:

```rust
let client = MinimalApi::builder().build()?;
let request = client.health();
request.await?;
```

Request construction is inert. Awaiting the request starts Concord's managed
work: public URL and header shaping, hooks, rate-limit admission,
authentication, private HTTP transport, bounded response acquisition, typed
decoding, and completion. Capabilities that an endpoint does not declare are
absent from its generated API.

## Start here

New to Concord? Follow the short path in order:

1. [Understand what Concord is](start/what-is-concord.md).
2. [Install the façade crate](start/install.md).
3. [Declare your first API](start/first-api.md).
4. [Build a client and make the first request](start/first-request.md).
5. [Choose the next feature for your API](start/next.md).

The repository example uses the reserved `.test` domain so it cannot
accidentally contact a public service. Its execution test routes the same
request through a deterministic loopback server and observes a real HTTP
response.

## What Concord generates

One declaration produces a domain-shaped Rust surface:

- a named client builder and immutable client;
- nested scope methods for reusable portions of a remote contract;
- direct endpoint methods and reusable explicit endpoint values;
- named request wrappers with only their valid controls and terminals;
- safe endpoint descriptions and request plans.

The compiler checks declaration structure and generated capability
composition. Rust's type system checks body, response, provider, pagination,
and extension implementations. Exact signatures remain in generated Rustdoc;
this site teaches how the pieces fit together and which path to choose.

## Find your task

| You want to… | Read… |
|---|---|
| understand the declaration-to-request model | [Contract model](concepts/contract-model.md) |
| learn the current declaration grammar | [DSL reference](reference/dsl.md) |
| understand generated names and controls | [Generated API reference](reference/generated-api.md) |
| add authentication | [Authentication](features/authentication.md) |
| send or decode buffered bodies | [Request bodies and buffered responses](features/bodies-and-buffered-responses.md) |
| consume streaming data | [Streaming responses](features/streaming.md) |
| collect sequential pages | [Pagination](features/pagination.md) |
| apply remote quota policy | [Rate limiting](features/rate-limiting.md) |
| choose request shape at runtime | [Runtime-defined endpoints](features/runtime-endpoints.md) |
| configure deployment transport | [Production HTTP](features/production-http.md) |
| compose a realistic client | [Regional API recipe](recipes/regional-apis.md) |
| implement a semantic extension | [Extension model](concepts/extension-model.md) |
| inspect exact features and limits | [Feature reference](reference/features.md) and [limits](reference/limits.md) |

Advanced extension authors should begin with the
[extension model](concepts/extension-model.md). Maintainers and fork authors
should begin with the [architecture overview](contributor/architecture.md).
For an exact Rust item, use the `concord` Rustdoc path named by the relevant
guide.

## Related

- [What Concord is](start/what-is-concord.md)
- [Your first API](start/first-api.md)
- [Complete DSL reference](reference/dsl.md)
- [Architecture overview](contributor/architecture.md)
