// §11.1: each secret is a required protected builder input; optional static
// secrets are not supported (an endpoint-backed source starts empty, and a
// custom provider may report unavailable).
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

api! {
    client OptionalSecretApi {
        base "https://example.test";
        credentials {
            secret optional_api_key?;
            credential key = api_key(secret.optional_api_key);
        }

        GET read() -> Bytes via Binary {
            auth { header "X-Api-Key" = key; }
        }
    }
}

fn main() {}
