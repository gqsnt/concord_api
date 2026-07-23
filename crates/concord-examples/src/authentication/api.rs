//! Declares static, endpoint-backed, OAuth2, and custom-provider credentials.
//!
//! Required feature: `oauth2-client`; fixed-HTTPS execution also requires
//! `default-tls`. Mode: construction and deterministic local proof.
//! Documentation owner: `docs/guide/04-authentication.md`.
//! Proving tests: `tests/construction.rs` and `tests/local_runtime.rs`.

use concord::api;
use concord::auth::OAuth2ClientCredentials;

api! {
    client AuthenticationApi {
        base "https://localhost/authentication-example/";
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
    }

    GET login() -> crate::authentication::extensions::ExampleTokenCodec;
    GET api_key() -> NoContent { auth header "X-Api-Key" = application_key; }
    GET bearer() -> NoContent { auth bearer static_bearer; }
    GET basic() -> NoContent { auth basic login; }
    GET o_auth() -> NoContent { auth bearer oauth challenge unauthorized; }
    GET endpoint_backed() -> NoContent { auth query "session" = acquired challenge never; }
    GET provider_backed() -> NoContent { auth bearer custom challenge never; }
}
