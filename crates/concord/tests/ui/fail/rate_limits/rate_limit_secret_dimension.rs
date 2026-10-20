// §12.3: secrets and credentials are never rate dimensions.
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

api! {
    client Api {
        base "https://example.test";
        credentials {
            secret token;
            credential bearer_token = bearer(secret.token);
        }
        rate_limit {
            id "concord.rate_limit_secret_dimension.1";
            key ["token" = secret.token];
            allow 1 per 1s;
        }

        GET read() -> Bytes via Binary {
            auth { bearer bearer_token; }
        }
    }
}

api! {
    client CredentialApi {
        base "https://example.test";
        credentials {
            secret token;
            credential bearer_token = bearer(secret.token);
        }
        rate_limit {
            id "concord.rate_limit_secret_dimension.2";
            key ["token" = credential.bearer_token];
            allow 1 per 1s;
        }
    }
}

fn main() {}
