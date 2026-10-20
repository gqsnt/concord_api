// AUTH-01 / §20.1: an authentication header name permits at most 256
// UTF-8 bytes (boundary: pass/language/auth_placement_name_boundary.rs).
#![allow(unused_imports)]

use concord::{Empty, api};

api! {
    client HeaderNameApi {
        base "https://example.test/";
        credentials {
            secret key;
            credential key = api_key(secret.key);
        }
        auth { header "abcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopq" = key; }

        GET read() -> () via Empty;
    }
}

fn main() {}
