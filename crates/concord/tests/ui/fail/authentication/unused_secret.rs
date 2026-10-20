// §3.5 "a dead required input cannot be retained": every secret is a
// required protected builder input, so a secret no credential source reads is
// rejected instead of being demanded from every caller for no effect.
#![allow(unused_imports)]

use concord::{Empty, api};

api! {
    client UnusedSecretApi {
        base "https://example.test/";
        credentials {
            secret unused;
        }

        GET read() -> () via Empty;
    }
}

api! {
    client PartlyUnusedSecretApi {
        base "https://example.test/";
        credentials {
            secret used;
            secret forgotten;
            credential key = api_key(secret.used);
        }

        GET read() -> () via Empty {
            auth { header "X-Key" = key; }
        }
    }
}

fn main() {}
