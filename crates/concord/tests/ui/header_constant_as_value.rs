#![allow(unused_imports)]
use concord::reqwest::header::ACCEPT;

/// A Rust constant holding a value: it is not a Concord value.
pub const JSON: &str = "application/json";

concord::api! {
    pub client Api {
        base "http://api.invalid";
        headers { ACCEPT = JSON; }
        GET get();
    }
}

concord::api! {
    pub client Other {
        base "http://other.invalid";
        headers { JSON = "x"; }
        GET get();
    }
}

fn main() {}
