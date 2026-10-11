// AUTH-01 / §20.1: an authentication query name permits at most 256 UTF-8
// bytes (boundary: pass/language/auth_placement_name_boundary.rs).
#![allow(unused_imports)]

use concord::{Empty, api};

api! {
    client QueryNameApi {
        base "https://example.test/";
        credentials {
            secret key;
            credential key = api_key(secret.key);
        }
        auth { query "abcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopq" = key; }

        GET read() -> () via Empty;
    }
}

fn main() {}
