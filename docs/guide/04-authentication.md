# 4. Authentication

Declare secret slots and semantic credential sources, then place credentials
where the remote contract requires them:

```rust
credentials {
    secret api_token: String;
    credential token = bearer(secret.api_token);
}

auth bearer token challenge unauthorized;
```

Generated builders expose named setters that accept secret-safe wrappers:

```rust
let client = PrivateApi::builder()
    .api_token(concord::SecretString::try_new(token)?)
    .build()?;
```

Supported declaration sources include API key, bearer, Basic, endpoint-backed,
and generic custom providers. A custom provider is an ordinary Rust type
implementing the semantic provider contract. For OAuth2 client credentials,
construct `concord::auth::OAuth2ClientCredentials` with its named builder and
pass it through the generated provider setter; the declaration compiler has
no OAuth-specific or JSON-specific branch. The builder validates its borrowed
URL, identifier, secret, and scope inputs against the exact
`concord::auth::MAX_OAUTH2_*` limits before copying. The `client_id_owned` and
`client_secret_owned` setters accept already-owned `SecretString` values.

`SecretString::try_new` bounds generic protected-string storage, and its
clones share immutable storage. Built-in API-key, bearer, and Basic sources
apply their tighter public limits when the generated client is built and
before a generated credential handle replaces a slot. A rejected replacement
leaves the previous material and version unchanged. Custom
`CredentialMaterial` does not need to implement `Clone`; Concord shares ready
material behind its private slot storage.

Credentials are resolved after public preparation and limiter acquisition, at
the private native boundary. Hooks, descriptions, plans, errors, completion
events, and debug output cannot see material. Authentication challenge
recovery is deterministic and bounded to at most one additional attempt.
Provider HTTP is isolated, finite, redacted, and does not recursively execute
the parent endpoint lifecycle. Custom providers can read the active semantic
request-body ceiling with `CredentialRequest::http.request_body_limit()`
without gaining access to native transport state.

See the [DSL authentication grammar](../reference/dsl.md#authentication), the
[security model](../concepts/security-and-redaction.md), and
[error categories](../reference/errors.md).

## Running proof

The
[authentication usage module](../../crates/concord-examples/src/authentication/usage.rs)
constructs named secret/provider configuration, and the
[extension module](../../crates/concord-examples/src/authentication/extensions.rs)
shows a semantic custom provider. The
[construction test](../../crates/concord-examples/tests/construction.rs) and
[local runtime test](../../crates/concord-examples/tests/local_runtime.rs)
prove construction and bounded recovery without external services.

Next: [Bodies, responses, and streams](05-bodies-responses-and-streams.md).
