// ORIGIN-02: generated labels on an IP-literal base are rejected, including
// labels selected by a scope below the client.
#![allow(unused_imports)]

use concord::{Binary, Bytes};

concord::api! {
    client Ipv6HostApi {
        base "http://[2001:db8::1]:8080/";

        scope regional() {
            subdomain ["api"];

            GET read() -> Bytes via Binary;
        }
    }
}

fn main() {}
