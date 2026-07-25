use concord::Bytes;

concord::api! {
    client Ipv4BaseApi {
        base "http://127.0.0.1:8080/";
    }

    GET read_v4() -> Bytes;
}

concord::api! {
    client Ipv6BaseApi {
        base "http://[2001:db8::1]:8080/";
    }

    GET read_v6() -> Bytes;
}

fn main() {}
