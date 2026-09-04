concord::api! {
    client Ipv4HostApi {
        timeout 30s;
        base "http://127.0.0.1:8080/";
        subdomain replace ["api"];


    GET read() -> concord::Bytes via concord::Bytes;
}}

fn main() {}
