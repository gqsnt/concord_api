// AUTH-01: a literal Authorization scheme is a validated HTTP token of at
// most 256 bytes.
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

api! {
    client EmptySchemeApi {
        base "https://example.test";
        credentials {
            secret key;
            credential key = api_key(secret.key);
        }

        GET protected() -> Bytes via Binary {
            auth { authorization "" = key; }
        }
    }
}

api! {
    client SpaceSchemeApi {
        base "https://example.test";
        credentials {
            secret key;
            credential key = api_key(secret.key);
        }

        GET protected() -> Bytes via Binary {
            auth { authorization "Token value" = key; }
        }
    }
}

api! {
    client NonAsciiSchemeApi {
        base "https://example.test";
        credentials {
            secret key;
            credential key = api_key(secret.key);
        }

        GET protected() -> Bytes via Binary {
            auth { authorization "Tökén" = key; }
        }
    }
}

api! {
    client OversizedSchemeApi {
        base "https://example.test";
        credentials {
            secret key;
            credential key = api_key(secret.key);
        }

        GET protected() -> Bytes via Binary {
            auth { authorization "ttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttt" = key; }
        }
    }
}

fn main() {}
