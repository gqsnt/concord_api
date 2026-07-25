concord::api! {
    client Ipv6HostApi {
        base "http://[2001:db8::1]:8080/";
    }

    GET read() -> concord::Bytes {
        host ["api"];
    }
}

fn main() {}
