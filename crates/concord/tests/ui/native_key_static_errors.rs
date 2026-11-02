// Errors the macro can prove stay compile errors in a chain with native keys: literal names on both
// sides, and an optional `override` without `else inherit`. Whether two native paths name the same
// header, even when written with the same tokens, is decided by `prepare()` (`tests/native_keys.rs`,
// `checks/hygiene`): equal tokens can resolve to different constants.
use concord::reqwest::header::{ACCEPT, AUTHORIZATION};

concord::api! {
    pub client Api(header(AUTHORIZATION) token: String) {
        base "http://x.test";
        headers { ACCEPT = "application/json"; "x-a" = "1"; }
        GET literal_twice() { headers { "x-a" = "2"; } }
        GET optional(maybe?: String) { headers { override ACCEPT = maybe; } }
        GET deferred() { headers { remove "x-never-set"; } }
    }
}

fn main() {}
