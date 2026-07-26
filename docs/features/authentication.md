# Authentication

Declare protected inputs once, turn them into semantic credentials, and place
those credentials where each endpoint requires them. Concord keeps private
material out of public request shaping, planning, hooks, descriptions, errors,
and completion events.

## Declare credentials and placements

This compiled contract demonstrates API-key, Bearer, Basic, provider-backed,
and endpoint-backed credentials:

## Complete example

```rust,ignore
{{#include ../../crates/concord-examples/src/authentication/api.rs:authentication_contract}}
```

The `credentials` block has two layers. A `secret` names protected builder
input. A `credential` defines the semantic material produced from a secret,
another endpoint, or a provider. Endpoint `auth` clauses then select a
credential and its private placement.

For the usual static cases:

- `auth bearer token` owns the `Authorization: Bearer ...` placement;
- `auth basic login` owns the Basic `Authorization` placement;
- `auth header "X-Api-Key" = application_key` places a credential in an
  ordinary private custom header;
- `auth query "session" = acquired` adds a private authentication query
  value.

The generated builder accepts `SecretString`, so raw protected strings are
bounded before entering client state:

```rust,ignore
{{#include ../../crates/concord-examples/src/authentication/usage.rs:authentication_client}}
```

`build()` verifies every required secret or provider. Built-in API-key,
Bearer, and Basic material also receives its protocol-specific validation
before the immutable client is created. A build error identifies the safe
generated input name and category without formatting its value.

## Understand the private boundary

Public preparation completes before credentials are placed. Concord prepares
and bounds the public request, runs the before-send hook, and acquires
rate-limit authority before it creates the authenticated transport request.
Only the private transport boundary can read credential material.

That ordering means a safe plan can show the method, credential-sanitized URL,
public headers, endpoint identity, timeout selection, and capabilities without
becoming an authenticated request. Authentication query values are absent
from its URL, and authentication headers are absent from its header view.

Do not put credentials into ordinary `query` or `headers` shaping. Public
values are observable application data and may appear in plans or safe hooks.

## Select challenge behavior

An auth placement may classify a response as a credential challenge, for
example:

```rust
auth bearer oauth challenge unauthorized;
```

Provider-backed material can then recover according to its provider contract.
Concord permits at most one authentication recovery attempt for the operation.
The exact current credential version is attributed, so unrelated credentials
are not invalidated. Static credentials do not become refreshable merely
because a challenge policy is present.

Use `challenge never` when the service status must remain an ordinary HTTP
failure. The [credential provider guide](credential-providers.md) explains
expiry, refresh, and OAuth2 behavior.

## Know the supported placements

The built-in model intentionally supports dedicated Bearer and Basic
placements, ordinary private custom headers, and private authentication query
values. A generic private header cannot use `Authorization`. Complete custom
schemes such as request signing, `Signature`, `AWS4-HMAC-SHA256`, or `MAC`
therefore cannot be represented by smuggling a value through public headers.

This limitation preserves one semantic owner for `Authorization`, collision
checking, limiter-before-authentication ordering, and automatic redaction. A
new signing model needs a dedicated extension contract rather than access to a
native request.

Authentication plans and protected strings are finite. Use the canonical
[limits reference](../reference/limits.md) for exact ceilings instead of
copying them into application configuration.

## Related

- [Credential providers and OAuth2](credential-providers.md)
- [Shape the public request](../guide/request-shaping.md)
- [DSL authentication reference](../reference/dsl.md#authentication)
- [Security and redaction](../concepts/security-and-redaction.md)
- [Extension model](../concepts/extension-model.md)
