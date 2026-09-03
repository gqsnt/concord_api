//! Declares static, endpoint-backed, OAuth2, and custom-provider credentials.
//!
//! Required feature: `oauth2-client`; fixed-HTTPS execution also requires
//! `default-tls`. Mode: construction and deterministic local execution.
//! Documentation owner: `docs/auth.md`.
//! Proving tests: `tests/construction.rs` and `tests/local_runtime.rs`.

use concord::api;
use concord::auth::OAuth2ClientCredentials;

// ANCHOR: authentication_contract
api! {
    client AuthenticationApi {
        timeout off;
        base "https://localhost/authentication-example/";
        // ANCHOR: credential_declarations
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
        // ANCHOR_END: credential_declarations


    // ANCHOR: static_authentication_endpoints
    GET login() -> crate::authentication::extensions::ExampleToken using crate::authentication::extensions::ExampleTokenCodec;
    GET api_key() -> () using NoContent { auth { header "X-Api-Key" = application_key; } }
    GET token_authorization() -> () using NoContent {
        auth { authorization "Token" = application_key; }
    }
    GET bearer() -> () using NoContent { auth { bearer static_bearer; } }
    GET basic() -> () using NoContent { auth { basic login; } }
    // ANCHOR_END: static_authentication_endpoints

    // ANCHOR: provider_authentication_endpoints
    GET o_auth() -> () using NoContent { auth { bearer oauth recover on unauthorized; } }
    GET endpoint_backed() -> () using NoContent { auth { query "session" = acquired; } }
    GET provider_backed() -> () using NoContent { auth { bearer custom; } }
    // ANCHOR_END: provider_authentication_endpoints
}}
// ANCHOR_END: authentication_contract
