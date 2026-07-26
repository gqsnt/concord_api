# Credential providers and OAuth2

Use a credential provider when material must be acquired, cached, expired, or
refreshed instead of supplied as one static secret. The generated client owns
the credential slot; the provider owns how new semantic material is obtained.

## Declare a provider-backed credential

The guide contract declares an OAuth2 provider as an ordinary custom provider:

```rust,ignore
{{#include ../../crates/concord-examples/src/authentication/api.rs:credential_declarations}}
```

`credential oauth = custom OAuth2ClientCredentials` names the provider type.
The endpoint places the resulting access token as Bearer authentication. The
declaration compiler has no OAuth-specific branch: it checks the same provider
contract used by downstream implementations.

Enable the `oauth2-client` façade feature and construct the built-in
client-credentials provider with its secret-safe builder:

## Complete example

```rust,ignore
{{#include ../../crates/concord-examples/src/authentication/usage.rs:oauth2_provider}}
```

Pass that value to the generated `oauth_provider(...)` builder method shown on
the [authentication page](authentication.md). The token URL must be an
absolute HTTPS URL for ordinary use. The named builder bounds and validates
the URL, client identifier, client secret, and optional scope before retaining
them.

## What Concord manages

The first request using an empty provider slot asks the provider to acquire
material. Successful material is cached in that built client's credential
slot and shared by client clones. Independent client builds have independent
slots unless the application deliberately shares a provider's own state.

When material reports an expiration, Concord reuses it while current and asks
the provider to refresh it as it approaches or reaches expiry. Concurrent
callers join one in-flight acquisition or refresh rather than starting a
token-request burst. The built-in OAuth2 provider reads `expires_in` when the
token response supplies it and returns expiring `AccessToken` material.

A placement with `challenge unauthorized` can classify a final unauthorized
response as rejection of the exact material used. A refreshable provider may
refresh and Concord may repeat the application operation once. Recovery is
bounded to that single additional attempt; a second challenge becomes the
final authentication or status failure.

## Provider HTTP is isolated

Providers receive a small semantic HTTP capability, not the application
client or native transport. The OAuth2 helper uses it for a bounded
`application/x-www-form-urlencoded` client-credentials exchange. Provider
requests remain outside application endpoint hooks, application rate-limit
plans, and application transport pools, so token acquisition cannot
recursively enter the parent endpoint lifecycle.

Provider HTTP still inherits the supported security and deployment settings
described by its contract, remains under the enclosing credential/operation
deadline, and enforces finite request and response bodies. Empty response
chunks cannot create unlimited non-progress. Provider errors cross a
redacted, typed authentication boundary.

## Variations and limits

Endpoint-backed credentials use a declared endpoint to acquire material and
store it through `.store_as_*()` before another endpoint consumes the slot.
They remain managed endpoint work, while provider HTTP remains isolated
support work. Choose the form that matches the remote protocol.

The built-in helper implements OAuth2 client credentials only. It is not an
authorization-code, device-code, browser-login, resource-owner-password, or
refresh-token client. Services with another acquisition protocol need a
custom `CredentialProvider`.

Custom providers control their own external calls, CPU, retained state, cache
cardinality, and safe error text. Concord bounds the semantic request/response
surface but cannot make arbitrary provider work finite. Start with the
[extension model](../concepts/extension-model.md) and use the exact public
provider traits from Rustdoc when implementing one.

## Related

- [Authentication](authentication.md)
- [Error reference](../reference/errors.md)
- [Limits reference](../reference/limits.md)
- [Feature flags](../reference/features.md)
- [Extension model](../concepts/extension-model.md)
