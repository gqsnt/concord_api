# Refresh OAuth credentials

## Goal

Call a protected API with OAuth2 client credentials, reuse the acquired access
token across client clones, refresh expiring material, and permit one bounded
recovery when the protected service rejects the exact token used.

## Complete code

Enable `oauth2-client` and `default-tls`. This complete compiled recipe
declares the provider, builds the client, and executes one protected endpoint:

```rust,ignore
{{#include ../../crates/concord-examples/src/recipes/oauth_refresh.rs:oauth_refresh_recipe}}
```

The included `failure_action` function shows one small, compiler-checked
application policy based on stable categories. `RetryLater` still needs an
application-owned finite schedule; `RepairCredentials` should enter the
operator's secret-rotation path; `Propagate` preserves ordinary handling.

## Why these declarations

`credential access = custom OAuth2ClientCredentials` creates a
provider-backed credential slot. The generated builder method is
`access_provider(...)`, making the provider role explicit.

`OAuth2ClientCredentials::builder` validates an HTTPS token URL, client
identifier, protected client secret, and optional scope. The helper sends a
bounded client-credentials form through isolated provider HTTP and returns
Bearer `AccessToken` material. When the token response includes `expires_in`,
the material carries a monotonic expiry.

`auth bearer access challenge unauthorized` owns Bearer placement and
classifies an unauthorized final response as a possible rejection of the
exact token version used. Concord may refresh and repeat the application
operation once. A second challenge is final.

The built client's credential slot caches current material and coalesces
concurrent acquisition or refresh work. Client clones share that slot, so
worker tasks do not independently create token storms.

## Adaptations

Omit `.scope(...)` when the authorization server does not require it, or use
the service's exact space-delimited scope value. Keep the token URL's allowed
query parameters in reviewed configuration; it is protected in provider
debug output.

Use an endpoint-backed credential when the acquisition protocol is itself a
normal declared API endpoint. Implement `CredentialProvider` for protocols
that do not match OAuth2 client credentials. The built-in helper does not
implement authorization-code, browser, device-code, password, or
refresh-token grants.

Change `challenge unauthorized` only when the service uses a different
documented final-status challenge policy. Recovery remains limited to one
additional application attempt.

## Production notes

Configure private trust roots and an explicit proxy on the generated HTTP
builder; those apply to provider HTTP. An application client identity does
not propagate to provider HTTP, so a token endpoint requiring separate mTLS
needs a supported provider design rather than accidental identity reuse.

Bound provider request/response bytes, authentication cooldown, and operation
timeouts from workload evidence. Provider work happens outside application
hooks and rate-limit plans, but remains within its finite semantic HTTP and
deadline boundary.

Keep client ID and client secret in an approved secret store. Provider errors
are redacted categories, not a reason to log token responses. Use finite
backoff and coordinate outages across callers even though Concord coalesces
one slot's in-flight work.

## Related

- [Credential providers and OAuth2](../features/credential-providers.md)
- [Authentication](../features/authentication.md)
- [Production HTTP](../features/production-http.md)
- [Error reference](../reference/errors.md)
- [Security and redaction](../concepts/security-and-redaction.md)
