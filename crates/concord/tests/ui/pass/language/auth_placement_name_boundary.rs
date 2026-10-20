// AUTH-01 boundary: 256-byte authentication header and query names compile
// (first excess: fail/authentication/auth_*_name_first_excess.rs).
use concord::{Empty, api};

api! {
    client BoundaryApi {
        base "https://example.test/";
        credentials {
            secret key;
            credential key = api_key(secret.key);
        }
        auth {
            header "abcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnop" = key;
            query "abcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnop" = key;
        }

        GET read() -> () via Empty;
    }
}

fn main() {}
