# Authentication

- **Audience:** API authors, callers, and credential-provider authors
- **Purpose:** end-to-end concept guide
- **Normative reference:** [DSL authentication rules](spec.md#authentication)

Authentication is a pipeline with distinct owners:

```text
secret input
→ credential source
→ versioned material
→ ordered private placement
→ challenge attribution
→ bounded recovery
```

Collapsing these stages into one generic `auth token` setting would hide
lifetime, reuse, placement, collision, refresh, and observation semantics.

## Declare sources

<!-- concord-source: crates/concord-examples/src/authentication/api.rs#credential_declarations -->
```concord
        credentials {
            secret api_key: String;
            secret bearer_token: String;
            secret username: String;
            secret password: String;
            credential application_key = api_key(secret.api_key);
            credential static_bearer = bearer(secret.bearer_token);
            credential login = basic(secret.username, secret.password);
            credential oauth = custom OAuth2ClientCredentials;
            credential acquired = endpoint login;
            credential custom = custom crate::authentication::extensions::ExampleProvider;
        }
```

Each `secret NAME: String` creates a required named builder input accepting
`SecretString`. Custom provider declarations create required named provider
inputs. Endpoint-backed sources name one endpoint leaf and are checked for
dependency cycles.

## Build the client

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

Secrets are not positional endpoint arguments. They are supplied once when the
client runtime snapshot is built.

## Attach private placement

<!-- concord-source: crates/concord-examples/src/authentication/api.rs#provider_authentication_endpoints -->
```concord
    GET o_auth() -> NoContent { auth bearer oauth challenge unauthorized; }
    GET endpoint_backed() -> NoContent { auth query "session" = acquired challenge never; }
    GET provider_backed() -> NoContent { auth bearer custom challenge never; }
```

Placements may use bearer, Basic, exact or static-scheme Authorization, a
private header, or a private query entry. Placement is an ordered attachment at
a client, scope, or endpoint. `auth replace` replaces inherited placements;
`auth off` removes them.

The `Authorization` header has exclusive ownership. Compiler and runtime
validation reject duplicate targets and public/private collisions.

## Material and slots

`SecretString`, `AccessToken`, `ApiKey`, and `BasicCredential` have bounded
construction and redacted formatting. `SecretString` is not a zeroizing secret
store; the application owns input loading, memory lifetime, and process-level
protection.

Credential slots are shared by client clones, version successful material, and
prevent stale in-flight acquisition or recovery from overwriting newer manual
replacement.

## Providers

`CredentialProvider` receives bounded acquisition context and isolated
`ProviderHttp`. It does not receive the application client, final authenticated
URL, request body, native request, or limiter authority.

Provider acquisition is single-flight per slot, has finite cooldown and
request/response limits, and runs under the operation deadline. Application
client identity PEM is not installed on provider HTTP.

## Endpoint-backed acquisition

A source endpoint executes through the same managed executor. Its generated
`.store_as_NAME().await` terminal stores the declared output into the named
credential slot. This preserves normal hooks, limiting, deadlines, response
handling, errors, and completion.

## Challenge and recovery

- `challenge never` disables status-triggered recovery.
- `challenge unauthorized` attributes 401 to the placement.
- `challenge unauthorized_or_forbidden` attributes 401 or 403.

One operation admits at most one recovery. Recovery replaces only material
that is still current, does not bypass hooks or limiter admission, and remains
inside the original deadline. It is distinct from application retry policy.

## Private placement boundary

The runtime resolves current credential material before limiter acquisition
but places it only after admission. Private query authentication exists only in
the native URL; private header authentication exists only in the native header
map.

Secrets and credentials do not enter public expressions, logical URLs, public
headers, safe plans, hooks, rate keys, completion, generated Rustdoc, or
diagnostics. No public API exposes the final authenticated native request.

Public application values may still be sensitive. Hooks, plans, and custom
limiters require an application logging policy even though credentials are
absent.

## Client certificates

Mutual TLS is HTTP transport configuration through
`configure_http(|http| http.identity_pem(...))`, not an authentication
attachment. It configures the application HTTP client rather than a request
placement.

## Related

- [Planning and observation](planning_and_observation.md)
- [Execution policy](execution_policy.md)
- [Extensions](extensions.md)
