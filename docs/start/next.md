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
| authenticate with Bearer, Basic, API key, or a provider | [Authentication](../guide/04-authentication.md) |
| send JSON or another buffered request body | [Bodies, responses, and streams](../guide/05-bodies-responses-and-streams.md) |
| decode JSON or buffered bytes | [Bodies, responses, and streams](../guide/05-bodies-responses-and-streams.md) |
| stream response bytes or CSV/NDJSON records | [Bodies, responses, and streams](../guide/05-bodies-responses-and-streams.md) |
| fetch sequential pages | [Pagination and rate limits](../guide/06-pagination-and-rate-limits.md) |
| partition and enforce local rate policy | [Pagination and rate limits](../guide/06-pagination-and-rate-limits.md) |
| upload multipart text, bytes, or files | [Bodies, responses, and streams](../guide/05-bodies-responses-and-streams.md) |
| configure TLS, proxies, certificates, and finite limits | [Production configuration](../recipes/production-configuration.md) |
| choose public endpoint shape at runtime | [Explicit and runtime endpoints](../guide/07-explicit-and-runtime-endpoints.md) |
| implement a custom body, decoder, provider, controller, limiter, or hook | [Extension model](../concepts/extension-model.md) |
| look up exact declaration syntax | [DSL reference](../reference/dsl.md) |
| understand compiler and runtime internals | [Contributor architecture](../contributor/architecture.md) |

## A productive reading order

For an application client, continue through the current core path:

1. [Understand generated Rust](../guide/02-understand-generated-rust.md).
2. [Build a client and make requests](../guide/03-build-a-client-and-make-requests.md).
3. Pick only the feature guides used by your contract.
4. Use a recipe when several features must compose.
5. Consult reference pages for exact grammar, names, flags, limits, and error
   categories.

You do not need to read maintainer architecture before using the crate.
Extension authors should first understand the ordinary generated request path,
then move to the extension model for the relevant trait family.

## Defaults to keep

Start with the generated builder and façade defaults:

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
numeric limits, error taxonomy, compatibility, security summary, dangerous
capabilities, and unsupported forms.

Use Rustdoc when implementing a trait or checking an exact method, associated
type, bound, or generated item signature. The advanced pages name the relevant
item paths so you can move directly from explanation to item-level authority.

## Related

- [Contract model](../concepts/contract-model.md)
- [Generated API reference](../reference/generated-api.md)
- [Feature flags](../reference/features.md)
- [Regional API recipe](../recipes/regional-apis.md)
- [Architecture overview](../contributor/architecture.md)
