//! Declares static, endpoint-backed, OAuth2, and custom-provider credentials.
//!
//! Required feature: `oauth2-client`; fixed-HTTPS execution also requires
//! `default-tls`. Mode: construction and deterministic local execution.
//! Documentation owner: `docs/auth.md`.
//! Proving tests: `tests/construction.rs` and `tests/local_runtime.rs`.
//!
//! Sources are not placements: `credentials` names where material comes from
//! and each endpoint's `auth` block places it. The endpoint-backed `acquired`
//! source starts empty; `login().store_as_acquired()` explicitly requests
//! installation and reports a `StoreOutcome`.

use concord::auth::OAuth2ClientCredentials;
use concord::{Empty, api};

use super::extensions::{ExampleProvider, ExampleToken, ExampleTokenCodec};

// ANCHOR: authentication_contract
api! {
    client AuthenticationApi {
        base "https://localhost/authentication-example/";
        timeout 30s;

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
            credential custom = custom ExampleProvider;
        }
        // ANCHOR_END: credential_declarations

        // ANCHOR: static_authentication_endpoints
        GET login() -> ExampleToken via ExampleTokenCodec;
        GET api_key() -> () via Empty { auth { header "X-Api-Key" = application_key; } }
        GET token_authorization() -> () via Empty {
            auth { authorization "Token" = application_key; }
        }
        GET bearer() -> () via Empty { auth { bearer static_bearer; } }
        GET basic() -> () via Empty { auth { basic login; } }
        // ANCHOR_END: static_authentication_endpoints

        // ANCHOR: provider_authentication_endpoints
        GET o_auth() -> () via Empty { auth { bearer oauth challenge unauthorized; } }
        GET endpoint_backed() -> () via Empty { auth { query "session" = acquired; } }
        GET provider_backed() -> () via Empty { auth { bearer custom; } }
        // ANCHOR_END: provider_authentication_endpoints
    }
}
// ANCHOR_END: authentication_contract
