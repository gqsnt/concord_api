// §3.2 method: only the listed methods or `METHOD("TOKEN")` start an
// endpoint; an unknown token is an unsupported scope entry.
#![allow(unused_imports)]

use concord::{Empty, api};

api! {
    client MalformedApi {
        base "https://malformed.example.test";

        scope resources(scope_value: String) {
            path "scopes/{scope_value}/resources";

            FETCH by_id(endpoint_value: String) -> () via Empty {
                path "{endpoint_value}";
            }
        }
    }
}

fn main() {}
