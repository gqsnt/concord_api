# Authentication

The [DSL specification](spec.md#authentication) owns credential declarations,
placements, merge rules, and the secret boundary. This file defines generated
construction and runtime credential behavior.

## Generated inputs

Each `secret NAME: String;` produces a required named builder method that
accepts `SecretString`.

Each custom provider credential produces a required named builder method that
accepts the declared provider type.

Each credential produces a generated handle on the built client. Static
credential handles expose fallible replacement. Provider handles expose
source-specific operations. A failed replacement preserves the previous
material and version.

Missing generated secret or provider inputs produce `ClientBuildError` with a
safe input name and category.

## Material

`SecretString`, `AccessToken`, `ApiKey`, and `BasicCredential` have bounded
construction and redacted `Debug` and `Display`. `SecretString` clones share
one immutable allocation.

`SecretString` is not a zeroizing secret store. Application code owns input
loading, memory lifetime, and process-level protection.

Custom credential material implements `CredentialMaterial`. Material used in
a private placement implements `SecretCredential`.

## Providers

`CredentialProvider` receives bounded acquisition context and isolated
`ProviderHttp`. It does not receive the application client, native request,
final authenticated URL, body, or limiter authority.

Provider HTTP is HTTPS-only when the runtime HTTPS policy requires it. It has
independent request and response limits. The application client identity PEM
is not installed on provider HTTP.

Concurrent acquisition for one credential slot is single-flight. Successful
material is versioned. Manual replacement, clearing, or provider replacement
supersedes stale in-flight completion.

Provider failure cooldown is finite. Expired material fails closed unless an
allowed acquisition produces valid replacement material.

## Endpoint-backed acquisition

`credential NAME = endpoint PATH;` uses one endpoint leaf as a credential
source. The source endpoint executes through the same runtime and stores its
declared output into the named slot through `.store_as_NAME().await`.

Dependency cycles among endpoint-backed credentials are rejected at compile
time.

## Challenge recovery

`challenge unauthorized` admits status 401.
`challenge unauthorized_or_forbidden` admits status 401 or 403.
`challenge never` disables status-triggered recovery.

One operation admits at most one authentication recovery. Recovery invalidates
only challenged material that is still current. A newer version is preserved.

Recovery does not bypass hooks, limiter admission, retry accounting,
deadlines, or completion.

## Private placement

The runtime prepares credential material before limiter acquisition and places
it only after admission. Query authentication exists only in the native URL.
Header authentication exists only in the native header map.

Collision validation rejects two owners for `Authorization`, duplicate private
header names, duplicate private query names, and collisions between public and
private headers.

No public API exposes the final authenticated native request.

## Client certificates

Mutual TLS uses application client-identity PEM:

```rust
let client = PrivateApi::builder()
    .configure_http(|http| http.identity_pem(identity_pem))
    .build()?;
```

Identity PEM is bounded and parsed during client build. It is installed only
on the application HTTP client. Custom root certificates are installed on
both application and provider clients.

Build errors identify invalid root-certificate or client-identity categories
without retaining PEM bytes or parser errors.

## Related

- [DSL authentication rules](spec.md#authentication)
- [Runtime model](runtime.md)
- [Customization](customization.md)
