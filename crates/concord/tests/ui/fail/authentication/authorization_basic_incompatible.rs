// AUTH-01: Basic material has the username/password capability; only a
// `basic` placement applies it, and a single-secret source cannot back a
// `basic` placement.
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

api! {
    client BasicAsAuthorizationApi {
        base "https://example.test";
        credentials {
            secret username;
            secret password;
            credential login = basic(secret.username, secret.password);
        }

        GET protected() -> Bytes via Binary {
            auth { authorization = login; }
        }
    }
}

api! {
    client BearerAsBasicApi {
        base "https://example.test";
        credentials {
            secret token;
            credential token = bearer(secret.token);
        }

        GET protected() -> Bytes via Binary {
            auth { basic token; }
        }
    }
}

api! {
    client SharedBasicSecretApi {
        base "https://example.test";
        credentials {
            secret both;
            credential login = basic(secret.both, secret.both);
        }

        GET protected() -> Bytes via Binary {
            auth { basic login; }
        }
    }
}

fn main() {}
