# Credentials and providers

Implement a credential provider when protected material must be acquired,
refreshed, or invalidated through an application-defined protocol. The
provider supplies material to a core-owned credential slot; the endpoint's
`auth` declaration still owns private header or query placement.

Use the exact Rustdoc contracts
`concord::auth::CredentialProvider`,
`concord::auth::CredentialMaterial`, and, for single-string placement,
`concord::auth::SecretCredential`.

## Complete example

This compiled example defines redacted token material and an asynchronous
provider that uses Concord's isolated provider HTTP capability:

```rust,ignore
{{#include ../../crates/concord-examples/src/authentication/extensions.rs:credential_provider_extension}}
```

The surrounding canonical API declares:

```text
credential custom = custom crate::authentication::extensions::ExampleProvider;
```

and places the resulting material with:

```text
auth bearer custom challenge never;
```

The client builder accepts `ExampleProvider` through the generated
`custom_provider(...)` method. The declaration, provider, builder path, and
deterministic provider execution are compiler-owned by `concord-examples` and
the façade contract tests.

## Define protected material

`CredentialMaterial` is immutable material retained in a shared credential
slot. It must be `Send + Sync + 'static`. Implement `expires_at` when the
provider can supply a monotonic expiration instant; Concord can then acquire
new material after expiry and may refresh within its configured skew.

Material used by Bearer, exact/static-scheme Authorization, custom-header, or
private-query placement implements `SecretCredential::expose_secret`. Concord
invokes that method only at the private authentication boundary. The
implementation still owns how its bytes are stored, copied, destroyed, logged,
or exposed elsewhere.

Give custom material explicitly redacted `Debug` and `Display`
implementations. The example wraps `SecretString` and renders only its type
and `<redacted>`. Avoid deriving a formatter that prints protected fields.
Prefer the built-in bounded `AccessToken`, `ApiKey`, or `BasicCredential`
unless the remote protocol needs a distinct material type.

## Acquire and refresh

`CredentialProvider::acquire` receives a `CredentialRequest` with:

- `http`, a borrowed isolated `ProviderHttp` capability;
- `reason`, one stable `CredentialReason`.

Reasons distinguish missing, expired, expiring-soon, and rejected material.
Use them only when the provider protocol needs different behavior; the default
`refresh` implementation delegates to `acquire`.

Override `refresh` when the protocol uses current material to obtain its
replacement. Treat the `current` value as sensitive. Do not retain it in error
text, task names, metric labels, or an unbounded history.

`recovery_mode` selects whether a protected-service challenge permits no
provider action, invalidation only, or refresh. The endpoint's `challenge`
clause independently decides which final status constitutes a challenge.
Concord attributes the rejection to the exact current material, coalesces work
for one slot, and permits at most one managed authentication recovery across
the whole operation, including all policy retries. A provider must not add an
unbounded retry loop inside `acquire` or `refresh`.

Calls can overlap across credential slots, clients, or independent provider
instances. Bound and synchronize provider-owned caches explicitly. Client
clones share the original slot; independently built clients do not unless the
application deliberately shares provider state.

## Use provider HTTP

`ProviderHttp` is the only HTTP capability passed to a provider. Construct a
`ProviderRequest` from public HTTP value types, a `ProviderBody`, an optional
shorter timeout, and an optional smaller response limit. `send` returns a
bounded `ProviderResponse`; the provider owns interpretation of success and
non-success statuses.

Provider HTTP:

- uses a separate managed client and connection pool;
- requires HTTPS and an available TLS capability;
- applies configured provider request and response byte ceilings;
- is constrained by the credential operation deadline and any shorter
  request-local timeout;
- suppresses and finitely bounds empty response chunks;
- exposes no Reqwest client, request, response, or middleware handle.

Use `ProviderHttp::request_body_limit` to reject predictable output before a
large allocation. `ProviderBody::Factory` can defer complete body creation,
but its synchronous callback remains trusted work and is checked only after
it returns.

Application trust roots and explicit proxy configuration also configure
provider HTTP. An application client identity does not propagate to the
provider client. Review the exact deployment behavior in
[Production HTTP](../features/production-http.md).

## Private placement, not request signing

A custom provider supplies credential material. It cannot inspect or mutate
the final native request. Public shaping occurs before private authentication,
and the declaration owns whether material becomes a Bearer value, Basic
credential, complete request-independent Authorization value, statically
prefixed Authorization value, custom header, or private query parameter.

For example, `auth authorization "Token" = custom challenge never` formats
the final value only after limiter admission. The provider supplies only the
single-string credential; it does not receive the scheme or request data.

This interface does not support arbitrary native request signing, middleware,
redirect interception, or transport replacement. A request-independent,
already-complete value can use exact Authorization placement. A protocol that
must compute a signature from the final request needs a separate client
contract; do not imply that a provider can reach hidden transport state.

An endpoint-backed credential is often a better fit when acquisition is an
ordinary declared Concord endpoint. Use a custom provider when the protocol
needs isolated provider HTTP or non-endpoint state transitions.

## Redacted failures

Return `AuthError::new(AuthErrorKind::...)` with the narrowest stable kind.
Provider URL text, request/response bodies, parser messages, credentials, and
native errors must not be retained. Convert material-construction failures
through their safe category rather than formatting the input.

Provider futures run under the remaining operation deadline and are dropped
on cancellation. Bytes already sent, secret-store reads already completed, or
external effects already committed are not rolled back.

The exact Rustdoc paths for the bounded transport are
`concord::auth::ProviderHttp`,
`concord::auth::ProviderRequest`,
`concord::auth::ProviderBody`, and
`concord::auth::ProviderResponse`.

## Verification

Test provider behavior through a generated credential slot, not only by
calling the trait directly. Cover missing material, expiry, proactive refresh,
challenge recovery, second challenge, concurrent acquisition coalescing,
cancellation, request and response exact limits, non-success provider status,
invalid TLS configuration, and redacted error formatting.

Use deterministic provider and application channels or loopback TLS support.
Assert that provider traffic never appears on the application channel and
that protected placement never appears in plans or hook events.

## Related

- [Credential providers and OAuth2](../features/credential-providers.md)
- [Authentication](../features/authentication.md)
- [Refresh OAuth credentials](../recipes/oauth-refresh.md)
- [Hooks, trusted work, and bounds](hooks-and-trusted-work.md)
- [Security and redaction](../concepts/security-and-redaction.md)
- Rustdoc: `concord::auth::CredentialProvider` and
  `concord::auth::ProviderHttp`
