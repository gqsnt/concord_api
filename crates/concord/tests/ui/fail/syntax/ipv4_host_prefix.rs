concord::api! {
    client Ipv4HostApi {
        base "http://127.0.0.1:8080/";
        host ["api"];
    }

    GET read() -> concord::Bytes;
}

fn main() {}
