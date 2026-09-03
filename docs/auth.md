# Authentication

Concord separates protected input, credential lifecycle, and private wire
placement. None is a public value.

## Declare sources

<!-- concord-source: crates/concord-examples/src/authentication/api.rs#credential_declarations -->
```rust
        credentials {
            secret api_key;
            secret bearer_token;
            secret username;
            secret password;
            credential application_key = api_key(secret.api_key);
            credential static_bearer = bearer(secret.bearer_token);
            credential login = basic(secret.username, secret.password);
            credential oauth = custom OAuth2ClientCredentials;
            credential acquired = endpoint login;
            credential custom = custom crate::authentication::extensions::ExampleProvider;
        }
```

Secrets have no pretend `String` type; generated inputs accept exactly
`SecretString`. Static API-key, bearer, and Basic sources retain protected
material. Endpoint-backed sources execute a stable endpoint through the managed
executor. Custom providers use isolated provider HTTP. Secrets and custom
providers are required builder inputs in v2.

<!-- concord-source: crates/concord-examples/src/authentication/usage.rs#authentication_client -->
```rust
    let client = AuthenticationApi::builder()
        .api_key(SecretString::try_new("example-api-key")?)
        .bearer_token(SecretString::try_new("example-bearer")?)
        .username(SecretString::try_new("example-user")?)
        .password(SecretString::try_new("example-password")?)
        .oauth_provider(oauth)
        .custom_provider(ExampleProvider)
        .build()?;
```

## Attach privately

An `auth { ... }` block adds ordered local placements to inherited placements.
`auth replace { ... }` starts without inherited placements and `auth off;`
clears authentication at that site. Bearer, Basic, exact/scheme authorization,
private header, and private query placements are supported.

Duplicate effective targets fail. Authorization forms are mutually exclusive.
Generic private headers cannot target authorization, media, proxy, connection,
cookie, or transport-owned fields. Request body specs remain the only owner of
`Content-Type`.

## Recovery

`recover on unauthorized` attributes 401 to a placement;
`recover on unauthorized_or_forbidden` includes 403. Omission means no recovery.
One request/page may recover at most once, with mechanically replayable body
state, remaining deadline/attempt bounds, and a new limiter admission. Recovery
is separate from policy retry.

The contract assumes the service's challenge makes one resend semantically
acceptable. Do not enable recovery for services that might challenge after an
irreversible accepted side effect.

## Isolation and order

Public shaping and the request gate occur before credential work. Credential
context is validated and material resolved before quota admission so missing or
foreign material does not consume quota. Private placement happens only after
admission, immediately before native dispatch.

Credential slots are versioned and single-flight; stale provider/recovery work
cannot overwrite newer material. Provider HTTP is independently bounded and
does not inherit application client certificate identity.

Secrets and credentials are structurally absent from public expressions,
descriptions, plans, gates, completion, rate keys, errors, debug, and ordinary
diagnostics. Client certificate identity is transport configuration, not DSL
authentication.

See the normative [DSL rules](spec.md#credentials-and-authentication) and
[runtime order](runtime.md#authentication-and-recovery).
