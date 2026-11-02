// Identity errors the macro can prove stay compile errors with native keys: the same constant on
// both sides, or literal names on both sides. An error that depends on what a constant holds (is
// `AUTHORIZATION` perhaps `x-never-set`?) is reported by `prepare()` instead (`tests/native_keys.rs`).
use concord::reqwest::header::{ACCEPT, AUTHORIZATION};

concord::api! {
    pub client Api(header(AUTHORIZATION) token: String) {
        base "http://x.test";
        headers { ACCEPT = "application/json"; "x-a" = "1"; }
        GET twice(again: String) { headers { AUTHORIZATION = again; } }
        GET literal_twice() { headers { "x-a" = "2"; } }
        GET optional(maybe?: String) { headers { override ACCEPT = maybe; } }
        GET deferred() { headers { remove "x-never-set"; } }
    }
}

fn main() {}
