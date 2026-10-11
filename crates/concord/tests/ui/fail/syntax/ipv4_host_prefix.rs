// ORIGIN-02: generated labels on an IP-literal base are rejected.
#![allow(unused_imports)]

use concord::{Binary, Bytes};

concord::api! {
    client Ipv4HostApi {
        base "http://127.0.0.1:8080/";
        subdomain replace ["api"];

        GET read() -> Bytes via Binary;
    }
}

fn main() {}
