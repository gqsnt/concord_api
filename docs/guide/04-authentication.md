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
    .api_token(concord::SecretString::new(token))
    .build()?;
```

Supported declaration sources include API key, bearer, Basic, endpoint-backed,
and generic custom providers. A custom provider is an ordinary Rust type
implementing the semantic provider contract. For OAuth2 client credentials,
construct `concord::OAuth2ClientCredentials` with its named builder and pass it
through the generated provider setter; the declaration compiler has no
OAuth-specific or JSON-specific branch.

Credentials are resolved after public preparation and limiter acquisition, at
the private native boundary. Hooks, descriptions, plans, errors, completion
events, and debug output cannot see material. Authentication challenge
recovery is deterministic and bounded to at most one additional attempt.
Provider HTTP is isolated, finite, redacted, and does not recursively execute
the parent endpoint lifecycle.

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
