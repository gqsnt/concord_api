# Choose your next path

You now have the basic loop:

```text
declare a contract → build a client → construct a request → await it
```

Choose the next page by the capability your remote API needs. Feature guides
show a working default before variations; references own exact syntax and
exhaustive tables.

| Task | Continue with |
|---|---|
| declare required, optional, literal, const-path, or trait-default values | [Values, defaults, and controls](../guide/values-and-controls.md) |
| authenticate with Bearer, Basic, API key, or a provider | [Authentication](../features/authentication.md) |
| send JSON or another buffered request body | [Request bodies and buffered responses](../features/bodies-and-buffered-responses.md) |
| decode JSON or buffered bytes | [Request bodies and buffered responses](../features/bodies-and-buffered-responses.md) |
| stream response bytes | [Streaming responses](../features/streaming.md) |
| collect or stream CSV or NDJSON records | [Record delivery](../features/records.md) |
| fetch sequential pages | [Pagination](../features/pagination.md) |
| partition and enforce local rate policy | [Rate limiting](../features/rate-limiting.md) |
| upload multipart text, bytes, or files | [Multipart uploads](../features/multipart.md) |
| configure TLS, proxies, certificates, and finite limits | [Production HTTP](../features/production-http.md) |
| choose public endpoint shape at runtime | [Runtime-defined endpoints](../features/runtime-endpoints.md) |
| implement a custom body, decoder, provider, controller, limiter, retry classifier, or hook | [Extension model](../extending/README.md) |
| look up exact declaration syntax | [DSL reference](../reference/dsl.md) |
| understand compiler and runtime internals | [Contributor architecture](../contributor/architecture.md) |

## A productive reading order

For an application client, continue through the current core path:

1. [Understand generated Rust](../guide/generated-rust.md).
2. [Build and share clients](../guide/client-construction.md).
3. Pick only the feature guides used by your contract.
4. Use a recipe when several features must compose.
5. Consult reference pages for exact grammar, names, flags, limits, and error
   categories.

You do not need to read maintainer architecture before using the crate.
Extension authors should first understand the ordinary generated request path,
then move to the extension model for the relevant trait family.

## Defaults to keep

Start with `Api::new()` when the declaration has no required client inputs.
Move to the generated builder when inputs or deployment configuration require
it. Keep the façade defaults unless the service gives you a reason to change
them:

- one built client per stable configuration, cloned where needed;
- finite runtime limits;
- default TLS for HTTPS;
- redirects and environment proxies disabled;
- the default process-local limiter when declarations use rate rules;
- typed response specifications instead of raw payload handling.

Change a default when the deployment or remote contract provides a concrete
reason. Keep proxy, certificate, provider, limiter, and hook choices on the
client builder; keep remote URL shape, authentication placement, and response
representation in the declaration.

## When to use reference or Rustdoc

Use the book reference for exact DSL grammar, generated naming, feature flags,
numeric limits, error taxonomy, compatibility, security summary, retry
policy, and unsupported forms.

Use Rustdoc when implementing a trait or checking an exact method, associated
type, bound, or generated item signature. The advanced pages name the relevant
item paths so you can move directly from explanation to item-level authority.

## Related

- [Contract model](../concepts/contract-model.md)
- [Generated API reference](../reference/generated-api.md)
- [Feature flags](../reference/features.md)
- [Regional API recipe](../recipes/regional-apis.md)
- [Architecture overview](../contributor/architecture.md)
