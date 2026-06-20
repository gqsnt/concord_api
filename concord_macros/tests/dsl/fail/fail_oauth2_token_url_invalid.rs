use concord_core::prelude::*;
use concord_macros::api;

api! {
    client InvalidOAuthTokenUrlApi {
        base "https://example.com"

        auth {
            secret client_id: String
            secret client_secret: String
            credential oauth = oauth2_client {
                token_url: "not a url",
                client_id: secret.client_id,
                client_secret: secret.client_secret,
            }
        }
    }

    GET Ping
    path ["ping"]
    auth bearer oauth
    -> Text<String>
}

fn main() {}
