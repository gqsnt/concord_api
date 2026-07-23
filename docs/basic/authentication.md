# Authentication

> Audience: Basic  
> Covers: static credentials, OAuth2 client credentials, endpoint-backed acquisition, placement, handles, and recovery

Authentication is declared separately from request shaping. Credentials remain protected until the private native boundary, and recovery stays inside the same managed attempt lifecycle.

## Authentication

Authentication is declared in a client `credentials` block and placed at client, scope, or endpoint layers. Static secret values and custom providers are named client-builder inputs; neither is a public request-shaping expression.

```rust
credentials {
    secret api_key: String;
    secret bearer_token: String;
    secret username: String;
    secret password: String;

    credential application_key = api_key(secret.api_key);
    credential static_bearer = bearer(secret.bearer_token);
    credential login = basic(secret.username, secret.password);
}
```

### Credential sources

Concord supports five source forms:

1. static API key;
2. static bearer token;
3. Basic username/password;
4. endpoint-backed acquisition;
5. a Rust type implementing `CredentialProvider`.

OAuth2 client credentials use the ordinary custom-provider form:

```rust
use concord::auth::OAuth2ClientCredentials;

// In the declaration:
credential oauth = custom OAuth2ClientCredentials;
```

The macro treats the provider path as an opaque Rust type and derives credential
placement compatibility from its associated material. Endpoint-backed and
application-defined providers are covered in the advanced guide.

### Placements

```rust
auth bearer static_bearer;
auth basic login;
auth header "X-Api-Key" = application_key;
auth query "session" = acquired;
```

Several placements can compose. `auth replace ...` replaces inherited placements. `auth off;` disables inherited authentication for that layer.

### Challenge policies

An authentication placement can specify:

```rust
challenge unauthorized
challenge unauthorized_or_forbidden
challenge never
```

A challenge-enabled dynamic credential can trigger at most one bounded recovery attempt. Recovery keeps the operation identity and original deadline, re-runs the ordinary attempt lifecycle, and reacquires the rate limiter.

### Managed ordering

```text
body preparation
→ credential preflight
→ limiter wait
→ exact-version credential finalization
→ native request construction
→ private authentication materialization
→ send
```

Authentication material is added only to the native request after safe request debug output. It does not enter the logical URL, public headers, hooks, generated documentation, or ordinary errors.

### Credential handles

Generated clients expose a credentials facade for source-appropriate operations. Clones share credential slots. Depending on source, handles can expose read-only references, presence checks, set/replace, clear, or same-material provider replacement. Generic mutation is intentionally not available for every source.

### HTTPS

Credential materialization fails closed when the final native destination is not allowed by the managed HTTPS policy. Provider traffic uses a separate managed channel and separate finite limits.

For custom acquisition, see [Custom credential providers](../advanced/authentication-hooks-and-debug.md#custom-credential-providers) and [Provider HTTP](../advanced/authentication-hooks-and-debug.md#provider-http).

## OAuth2 client credentials

Enable `oauth2-client`, import the extras-owned provider through the façade, and
declare it as an ordinary provider type:

```rust
use concord::auth::OAuth2ClientCredentials;

credentials {
    credential oauth = custom OAuth2ClientCredentials;
}

auth bearer oauth challenge unauthorized;
```

### What Concord manages

- bounded provider request and response bodies;
- isolated provider HTTP client;
- HTTPS validation;
- single-flight acquisition/refresh;
- expiration and proactive refresh;
- exact-current credential versioning;
- one bounded challenge recovery;
- redacted automatic diagnostics.

### Construction

Construct the provider with named, secret-safe inputs, then pass it to the
generated client builder:

```rust
let oauth = OAuth2ClientCredentials::builder(
    "https://identity.example.com/oauth/token",
)
.client_id(client_id)
.client_secret(client_secret)
.scope("inventory.read")
.build()?;

let client = InventoryApi::builder()
    .oauth_provider(oauth)
    .build()?;
```

The provider builder validates an absolute HTTPS URL with a host and rejects
userinfo, passwords, and fragments. Its `Debug`, build errors, and provider
`Debug` output never include the URL, client identifier, client secret, or
scope. The generated client never accepts these OAuth secrets positionally.

### Recovery

On an allowed final challenge, Concord attributes the rejected exact-current material, refreshes according to source policy, and runs one recovery attempt with the same operation identity and deadline. The limiter is reacquired.

### Limits

Provider request and response limits default to 1 MiB. The token request runs under the operation deadline. Configure smaller provider limits when the identity service has a known envelope.

### When to use a custom provider

Use another `CredentialProvider` when the token protocol is not OAuth2 client credentials, material comes from a signer or secure service, or custom refresh/recovery policy is required. A generated custom-provider handle accepts replacement by any provider with the same associated material type. Keep provider traffic isolated through `ProviderHttp` when HTTP is needed.

### Logging

Never log the client secret, token response, access token, native authenticated URL, or provider body. Built-in formatting is redacted, but application provider logs are outside that boundary.
