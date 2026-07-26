# API-key authentication

## Goal

Build a client from one protected API key and place that key in a private
custom header on the endpoints that require it. The key must stay absent from
public request plans, hooks, descriptions, and errors.

## Complete code

This entire declaration and builder path is compiled in
`concord-examples`:

```rust,ignore
{{#include ../../crates/concord-examples/src/recipes/api_key.rs:api_key_recipe}}
```

Application startup can call:

```rust
let client = build(settings.api_key)?;
let profile = client.profile();
```

Constructing `profile` remains inert. The private `X-Api-Key` header is added
only when the request executes.

## Why these declarations

The `secret api_key: String` line creates one required protected builder
input. `credential application_key = api_key(secret.api_key)` turns that slot
into built-in API-key material with its tighter validation. The names are
deliberate: the builder method follows the secret name (`api_key`), while the
endpoint placement refers to the credential name (`application_key`).

`auth header "X-Api-Key" = application_key` is a private authentication
placement. It is different from:

```rust
headers { "X-Api-Key" = vars.api_key; }
```

The latter would be public shaping and is rejected for a secret. Private
placement occurs after safe public preparation, hooks, and limiter admission.

`challenge never` keeps a 401 or 403 as an ordinary final HTTP-status failure.
A static API key cannot refresh itself, so retrying it automatically after a
challenge would repeat the same material.

`SecretString::try_new` bounds generic protected storage before the value
enters the generated builder. `build()` then applies the built-in API-key
material limit and reports only safe input metadata on failure.

## Adaptations

For a query-based service, replace the endpoint placement with
`auth query "api_key" = application_key`. The value remains private and is
removed from credential-sanitized URLs.

Use `auth bearer ...` for an actual Bearer token and `auth basic ...` for a
username/password credential. Do not model those protocols as arbitrary
custom headers.

Place authentication at the client or scope when every descendant shares it.
Use `auth off`, `auth replace`, or endpoint-local additions only when the
remote contract requires a different effective plan. The DSL reference owns
the exact inheritance grammar.

If an operator rotates a static key without rebuilding the complete
configuration, use the generated credential handle's fallible replacement
surface. Validation failure preserves the previously installed material;
coordinate rotation policy in application code.

## Production notes

Load the key from an approved secret source, never a command-line argument
that may appear in process listings or a public configuration file. Avoid
formatting it in application errors before wrapping it in `SecretString`.

Require HTTPS for the application client, configure trust roots and an
explicit proxy as needed, and test that plans and hooks contain no
authentication header. Redaction does not make arbitrary public tenant,
account, or path values safe to log.

Treat non-success statuses according to the service contract. A rejected
static key normally needs operator rotation, not an unbounded retry loop.
Keep any retry/backoff finite and distinguish authentication rejection from
connection or timeout failures through typed error categories.

## Related

- [Authentication](../features/authentication.md)
- [Credential providers and OAuth2](../features/credential-providers.md)
- [Security and redaction](../concepts/security-and-redaction.md)
- [Production HTTP](../features/production-http.md)
- [DSL authentication reference](../reference/dsl.md#authentication)
